//! Apply a default SQL row limit without confusing nested limits or quoted text.
use sqlparser::{
    ast::{LimitClause, SetExpr, Statement},
    dialect::{GenericDialect, dialect_from_str},
    parser::Parser,
};

/// Returns rewritten SQL only for a single unrestricted query. Other statements are untouched.
pub fn limited_sql(sql: &str, dialect: &str, limit: usize) -> Option<String> {
    if ["mongo", "redis", "neo4j", "graphql"].contains(&dialect) {
        return None;
    }
    let name = match dialect {
        "pg" => "postgresql",
        "mariadb" => "mysql",
        other => other,
    };
    let parser_dialect = dialect_from_str(name).unwrap_or_else(|| Box::new(GenericDialect {}));
    let mut statements = Parser::parse_sql(parser_dialect.as_ref(), sql).ok()?;
    if statements.len() != 1 {
        return None;
    }
    let Statement::Query(query) = &mut statements[0] else {
        return None;
    };
    if query.fetch.is_some() {
        return None;
    }
    if let Some(clause) = &query.limit_clause {
        match clause {
            LimitClause::LimitOffset { limit: None, .. } => {}
            _ => return None,
        }
    }
    if let SetExpr::Select(select) = query.body.as_ref() {
        if select.top.is_some() || select.into.is_some() {
            return None;
        }
    }
    // Parse the dialect's limit syntax once, then attach it to the outer query.
    let template = match dialect {
        "mssql" => format!("SELECT TOP {limit} * FROM t"),
        "oracle" | "db2" => format!("SELECT * FROM t FETCH FIRST {limit} ROWS ONLY"),
        _ => format!("SELECT * FROM t LIMIT {limit}"),
    };
    let mut templates = Parser::parse_sql(&GenericDialect {}, &template).ok()?;
    let Statement::Query(mut bounded) = templates.remove(0) else {
        return None;
    };
    if dialect == "mssql" {
        let SetExpr::Select(select) = query.body.as_mut() else {
            return None;
        };
        let SetExpr::Select(template) = bounded.body.as_mut() else {
            return None;
        };
        select.top = template.top.take();
    } else if bounded.fetch.is_some() {
        query.fetch = bounded.fetch;
    } else {
        if let (
            Some(LimitClause::LimitOffset { offset, .. }),
            Some(LimitClause::LimitOffset { offset: target, .. }),
        ) = (&mut query.limit_clause, &mut bounded.limit_clause)
        {
            *target = offset.take();
        }
        query.limit_clause = bounded.limit_clause;
    }
    Some(statements[0].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_only_the_outer_query() {
        let sql = limited_sql(
            "-- preview\nselect * from (select * from reviews limit 5) r;",
            "mysql",
            2000,
        )
        .unwrap();
        assert!(sql.ends_with("LIMIT 2000"));
        assert!(sql.contains("LIMIT 5"));
        assert!(
            limited_sql("select 'limit 10' from reviews", "mysql", 2000)
                .unwrap()
                .ends_with("LIMIT 2000")
        );
        assert!(
            limited_sql("with r as (select * from reviews) select * from r", "pg", 2000)
                .unwrap()
                .ends_with("LIMIT 2000")
        );
        assert!(
            limited_sql("select * from reviews offset 10", "pg", 2000)
                .unwrap()
                .ends_with("LIMIT 2000 OFFSET 10")
        );
    }
    #[test]
    fn preserves_explicit_limits_and_changes() {
        for sql in [
            "select * from reviews limit 5",
            "select top 5 * from reviews",
            "select * from reviews fetch first 5 rows only",
            "delete from reviews",
            "select * from reviews; delete from reviews",
            "select * into backup from reviews",
        ] {
            assert!(limited_sql(sql, "generic", 2000).is_none(), "{sql}");
        }
    }
    #[test]
    fn uses_dialect_limit_syntax() {
        assert!(limited_sql("select * from reviews", "mssql", 2000).unwrap().contains("TOP 2000"));
        assert!(
            limited_sql("select * from reviews", "oracle", 2000)
                .unwrap()
                .ends_with("FETCH FIRST 2000 ROWS ONLY")
        );
    }
}
