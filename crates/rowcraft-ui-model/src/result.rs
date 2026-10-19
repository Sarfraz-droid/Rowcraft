//! A query result as the frontend receives it.

use std::time::Duration;

use rowcraft_dialect::{ResultSet, Value};
use serde::Serialize;
use serde_json::{Value as Json, json};

/// Rows kept in memory per query. Paging through bigger results comes with cursor support.
pub const ROW_LIMIT: usize = 2000;

/// Largest integer a JSON number (an IEEE double in JavaScript) holds exactly.
const MAX_SAFE_INT: i64 = (1 << 53) - 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryOutput {
    pub columns: Vec<String>,
    /// One JSON value per cell, see [`cell_json`].
    pub rows: Vec<Vec<Json>>,
    /// The status-bar text, e.g. `3 rows in 12 ms`.
    pub summary: String,
    /// Default limit applied to an unrestricted SELECT; the UI can offer an explicit rerun.
    pub row_limit: Option<usize>,
}

impl QueryOutput {
    pub fn new(rs: ResultSet, elapsed: Duration) -> Self {
        let summary = describe_result(&rs, elapsed);
        let rows = rs.rows.iter().map(|r| r.iter().map(cell_json).collect()).collect();
        Self { columns: rs.columns, rows, summary, row_limit: None }
    }
}

/// How one cell crosses to the frontend:
/// `null` is SQL NULL, a string is text, `{"bytes": n}` is binary data of `n` bytes. Numbers and
/// booleans are themselves, except integers a JavaScript number cannot hold exactly and
/// non-finite floats (JSON has no NaN), which travel as text so nothing is silently rounded.
pub fn cell_json(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) if i.unsigned_abs() <= MAX_SAFE_INT.unsigned_abs() => json!(i),
        Value::Int(i) => json!(i.to_string()),
        Value::Float(f) if f.is_finite() => json!(f),
        Value::Float(f) => json!(f.to_string()),
        Value::Text(s) => json!(s),
        Value::Bytes(b) => json!({ "bytes": b.len() }),
    }
}

pub fn describe_result(rs: &ResultSet, elapsed: Duration) -> String {
    let took = format_duration(elapsed);
    if rs.columns.is_empty() {
        return match rs.rows_affected {
            Some(n) => format!("OK, {n} row{} affected in {took}", plural(n as usize)),
            None => format!("OK in {took}"),
        };
    }
    let n = rs.rows.len();
    let limited = if rs.truncated { format!(" (first {ROW_LIMIT} only)") } else { String::new() };
    format!("{n} row{}{limited} in {took}", plural(n))
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn format_duration(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.1} s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_keep_null_text_and_binary_apart() {
        assert_eq!(cell_json(&Value::Null), Json::Null);
        assert_eq!(cell_json(&Value::Text("NULL".into())), json!("NULL"));
        assert_eq!(cell_json(&Value::Bytes(vec![1, 2, 3])), json!({ "bytes": 3 }));
        assert_eq!(cell_json(&Value::Int(-7)), json!(-7));
        assert_eq!(cell_json(&Value::Bool(true)), json!(true));
    }

    #[test]
    fn numbers_javascript_cannot_hold_are_not_rounded() {
        assert_eq!(cell_json(&Value::Int(MAX_SAFE_INT)), json!(MAX_SAFE_INT));
        assert_eq!(cell_json(&Value::Int(i64::MAX)), json!("9223372036854775807"));
        assert_eq!(cell_json(&Value::Int(i64::MIN)), json!("-9223372036854775808"));
        assert_eq!(cell_json(&Value::Float(1.5)), json!(1.5));
        assert_eq!(cell_json(&Value::Float(f64::NAN)), json!("NaN"));
        assert_eq!(cell_json(&Value::Float(f64::INFINITY)), json!("inf"));
    }

    #[test]
    fn result_summaries() {
        let ms = Duration::from_millis(12);
        let select = |n: usize, truncated| ResultSet {
            columns: vec!["a".into()],
            rows: vec![vec![Value::Int(1)]; n],
            truncated,
            ..ResultSet::default()
        };
        assert_eq!(describe_result(&select(1, false), ms), "1 row in 12 ms");
        assert_eq!(describe_result(&select(3, false), ms), "3 rows in 12 ms");
        assert_eq!(
            describe_result(&select(ROW_LIMIT, true), Duration::from_millis(2500)),
            format!("{ROW_LIMIT} rows (first {ROW_LIMIT} only) in 2.5 s")
        );
        let dml = ResultSet { rows_affected: Some(2), ..ResultSet::default() };
        assert_eq!(describe_result(&dml, ms), "OK, 2 rows affected in 12 ms");
        assert_eq!(describe_result(&ResultSet::default(), ms), "OK in 12 ms");
    }

    #[test]
    fn output_carries_columns_rows_and_summary() {
        let rs = ResultSet {
            columns: vec!["a".into(), "b".into()],
            rows: vec![vec![Value::Int(1), Value::Null]],
            ..ResultSet::default()
        };
        let out = QueryOutput::new(rs, Duration::from_millis(1));
        assert_eq!(out.columns, ["a", "b"]);
        assert_eq!(out.rows, vec![vec![json!(1), Json::Null]]);
        assert_eq!(out.summary, "1 row in 1 ms");
    }
}
