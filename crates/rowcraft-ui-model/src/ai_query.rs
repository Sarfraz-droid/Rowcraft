//! Reject mutations in AI database tools before opening a read-only connection.
use sqlparser::{
    ast::{Expr, Query, Select, SetExpr, Statement, TableFactor, Visit, Visitor},
    dialect::GenericDialect,
    parser::Parser,
};
use std::ops::ControlFlow;

pub fn validate(statement: &str) -> Result<String, String> {
    if statement.len() > 16_000 {
        return Err("Keep tool queries below 16 KB.".into());
    }
    let statements = Parser::parse_sql(&GenericDialect {}, statement)
        .map_err(|e| format!("Use a single read-only SELECT query: {e}"))?;
    if statements.len() != 1 || !matches!(&statements[0], Statement::Query(_)) {
        return Err("Database tools accept one SELECT query at a time. Run changes explicitly from the console.".into());
    }
    struct ReadOnly;
    impl Visitor for ReadOnly {
        type Break = &'static str;
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
            if !query.locks.is_empty()
                || query.settings.is_some()
                || !query.pipe_operators.is_empty()
            {
                return ControlFlow::Break("Locking queries are unavailable to AI tools.");
            }
            if !matches!(
                query.body.as_ref(),
                SetExpr::Select(_)
                    | SetExpr::Query(_)
                    | SetExpr::SetOperation { .. }
                    | SetExpr::Values(_)
            ) {
                return ControlFlow::Break("Data-changing statements are unavailable to AI tools.");
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<Self::Break> {
            if select.into.is_some() {
                return ControlFlow::Break("SELECT INTO is unavailable to AI tools.");
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<Self::Break> {
            if matches!(
                table,
                TableFactor::Table { args: Some(_), .. }
                    | TableFactor::TableFunction { .. }
                    | TableFactor::Function { .. }
            ) {
                return ControlFlow::Break("Table functions are unavailable to AI database tools.");
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
            if let Expr::Function(function) = expr {
                let name = function.name.to_string().to_ascii_lowercase();
                let normalized = name.trim_matches(['"', '`', '[', ']']);
                let allowed = [
                    "count",
                    "sum",
                    "avg",
                    "min",
                    "max",
                    "coalesce",
                    "nullif",
                    "ifnull",
                    "lower",
                    "upper",
                    "length",
                    "char_length",
                    "trim",
                    "ltrim",
                    "rtrim",
                    "substring",
                    "substr",
                    "concat",
                    "concat_ws",
                    "replace",
                    "round",
                    "floor",
                    "ceil",
                    "ceiling",
                    "abs",
                    "mod",
                    "power",
                    "sqrt",
                    "date",
                    "datetime",
                    "strftime",
                    "date_trunc",
                    "date_part",
                    "extract",
                    "date_format",
                    "year",
                    "month",
                    "day",
                    "now",
                    "current_date",
                    "current_timestamp",
                    "row_number",
                    "rank",
                    "dense_rank",
                    "lag",
                    "lead",
                    "first_value",
                    "last_value",
                    "string_agg",
                    "group_concat",
                    "array_agg",
                    "json_agg",
                    "json_extract",
                    "json_extract_path_text",
                    "greatest",
                    "least",
                    "to_char",
                    "to_date",
                    "to_timestamp",
                ];
                if !allowed.contains(&normalized) {
                    return ControlFlow::Break(
                        "Only standard aggregate, string, date, and math functions are available to AI database tools.",
                    );
                }
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
            if !matches!(statement, Statement::Query(_)) {
                return ControlFlow::Break("Mutating statements are unavailable to AI tools.");
            }
            ControlFlow::Continue(())
        }
    }
    match statements[0].visit(&mut ReadOnly) {
        ControlFlow::Continue(()) => Ok(statements[0].to_string()),
        ControlFlow::Break(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allows_selects_joins_and_readonly_ctes() {
        for sql in [
            "SELECT count(*) FROM people",
            "WITH p AS (SELECT id FROM people) SELECT * FROM p",
            "SELECT p.id FROM people p LEFT JOIN teams t ON p.team_id = t.id LIMIT 25",
        ] {
            assert!(validate(sql).is_ok(), "{sql}");
        }
    }
    #[test]
    fn executes_only_the_validated_ast_and_rejects_executable_comment_payloads() {
        assert_eq!(validate("SELECT 1 /* ordinary comment */").expect("plain SELECT"), "SELECT 1");
        assert!(validate("SELECT 1 /*! INTO OUTFILE '/tmp/hidden' */").is_err());
    }
    #[test]
    fn rejects_mutations_including_ctes_and_stacked_queries() {
        for sql in [
            "DELETE FROM people",
            "SELECT 1; DROP TABLE people",
            "WITH gone AS (DELETE FROM people RETURNING *) SELECT * FROM gone",
            "SELECT * INTO backup FROM people",
            "SELECT * FROM people FOR UPDATE",
            "SELECT pg_read_file('/etc/passwd')",
            "SELECT nextval('seq')",
            "SELECT * FROM pg_read_file('/etc/passwd')",
            "SELECT \"pg_read_file\"('/etc/passwd')",
            "COPY people TO '/tmp/export'",
        ] {
            assert!(validate(sql).is_err(), "{sql}");
        }
    }
}
