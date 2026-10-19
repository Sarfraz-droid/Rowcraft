use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::str::FromStr;

use rowcraft_dialect::{ResultSet, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Csv,
    Tsv,
    Json,
    Ndjson,
    Markdown,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown format `{0}` (expected table, csv, tsv, json, ndjson or md)")]
pub struct ParseFormatError(String);

impl FromStr for Format {
    type Err = ParseFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "table" => Self::Table,
            "csv" => Self::Csv,
            "tsv" => Self::Tsv,
            "json" => Self::Json,
            "ndjson" | "jsonl" => Self::Ndjson,
            "md" | "markdown" => Self::Markdown,
            _ => return Err(ParseFormatError(s.to_owned())),
        })
    }
}

pub fn write_result(rs: &ResultSet, format: Format, out: &mut dyn Write) -> io::Result<()> {
    match format {
        Format::Table => table(rs, out),
        Format::Csv => delimited(rs, out, ',', csv_field),
        Format::Tsv => delimited(rs, out, '\t', tsv_field),
        Format::Json => json(rs, out),
        Format::Ndjson => ndjson(rs, out),
        Format::Markdown => markdown(rs, out),
    }
}

fn cell_text(v: &Value) -> Cow<'_, str> {
    match v {
        Value::Null => Cow::Borrowed(""),
        Value::Bool(b) => Cow::Owned(b.to_string()),
        Value::Int(i) => Cow::Owned(i.to_string()),
        Value::Float(f) => Cow::Owned(f.to_string()),
        Value::Text(s) => Cow::Borrowed(s),
        Value::Bytes(b) => Cow::Owned(hex(b)),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("\\x");
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// psql-style aligned table. NULL is shown as `NULL` here (and only here) so it can be told apart
/// from an empty string on a terminal.
fn table(rs: &ResultSet, out: &mut dyn Write) -> io::Result<()> {
    if rs.columns.is_empty() {
        return match rs.rows_affected {
            Some(n) => writeln!(out, "OK, {n} row{} affected", plural(n as usize)),
            None => writeln!(out, "OK"),
        };
    }
    let show = |v: &Value| -> String {
        match v {
            Value::Null => "NULL".to_owned(),
            other => cell_text(other).replace('\n', "\\n").replace('\r', "\\r"),
        }
    };
    let cells: Vec<Vec<String>> = rs.rows.iter().map(|r| r.iter().map(show).collect()).collect();
    let mut widths: Vec<usize> = rs.columns.iter().map(|c| c.chars().count()).collect();
    for row in &cells {
        for (w, c) in widths.iter_mut().zip(row) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |cols: &[String]| -> String {
        let parts: Vec<String> = cols
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}", w = *w + c.len() - c.chars().count()))
            .collect();
        parts.join(" | ").trim_end().to_owned()
    };
    writeln!(out, "{}", line(&rs.columns))?;
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    writeln!(out, "{}", rule.join("-+-"))?;
    for row in &cells {
        writeln!(out, "{}", line(row))?;
    }
    writeln!(out, "({} row{})", cells.len(), plural(cells.len()))
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn delimited(
    rs: &ResultSet,
    out: &mut dyn Write,
    sep: char,
    field: fn(&str) -> Cow<'_, str>,
) -> io::Result<()> {
    let join = |cells: Vec<Cow<'_, str>>| cells.join(&sep.to_string());
    writeln!(out, "{}", join(rs.columns.iter().map(|c| field(c)).collect()))?;
    for row in &rs.rows {
        writeln!(
            out,
            "{}",
            join(row.iter().map(|v| field(&cell_text(v)).into_owned().into()).collect())
        )?;
    }
    Ok(())
}

/// RFC 4180: quote when the field has a comma, quote, or line break.
fn csv_field(s: &str) -> Cow<'_, str> {
    if s.contains([',', '"', '\n', '\r']) {
        Cow::Owned(format!("\"{}\"", s.replace('"', "\"\"")))
    } else {
        Cow::Borrowed(s)
    }
}

fn tsv_field(s: &str) -> Cow<'_, str> {
    if s.contains(['\t', '\n', '\r', '\\']) {
        Cow::Owned(
            s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r"),
        )
    } else {
        Cow::Borrowed(s)
    }
}

fn json_value(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) => J::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
        Value::Text(s) => J::String(s.clone()),
        Value::Bytes(b) => J::String(hex(b)),
    }
}

fn json_rows(rs: &ResultSet) -> impl Iterator<Item = serde_json::Value> + '_ {
    rs.rows.iter().map(|row| {
        let map: serde_json::Map<String, serde_json::Value> =
            rs.columns.iter().cloned().zip(row.iter().map(json_value)).collect();
        serde_json::Value::Object(map)
    })
}

fn json(rs: &ResultSet, out: &mut dyn Write) -> io::Result<()> {
    let rows: Vec<_> = json_rows(rs).collect();
    serde_json::to_writer_pretty(&mut *out, &rows)?;
    writeln!(out)
}

fn ndjson(rs: &ResultSet, out: &mut dyn Write) -> io::Result<()> {
    for row in json_rows(rs) {
        serde_json::to_writer(&mut *out, &row)?;
        writeln!(out)?;
    }
    Ok(())
}

fn markdown(rs: &ResultSet, out: &mut dyn Write) -> io::Result<()> {
    let esc = |s: &str| s.replace('|', "\\|").replace('\n', "<br>").replace('\r', "");
    writeln!(out, "| {} |", rs.columns.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | "))?;
    writeln!(out, "| {} |", rs.columns.iter().map(|_| "---").collect::<Vec<_>>().join(" | "))?;
    for row in &rs.rows {
        writeln!(
            out,
            "| {} |",
            row.iter().map(|v| esc(&cell_text(v))).collect::<Vec<_>>().join(" | ")
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ResultSet {
        ResultSet {
            columns: vec!["id".into(), "name".into()],
            rows: vec![
                vec![Value::Int(1), Value::Text("a,b".into())],
                vec![Value::Int(22), Value::Null],
            ],
            rows_affected: None,
            truncated: false,
        }
    }

    fn render(rs: &ResultSet, f: Format) -> String {
        let mut buf = Vec::new();
        write_result(rs, f, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn table_is_aligned_and_counts_rows() {
        assert_eq!(
            render(&sample(), Format::Table),
            "id | name\n---+-----\n1  | a,b\n22 | NULL\n(2 rows)\n"
        );
    }

    #[test]
    fn table_for_statement_without_columns() {
        let rs = ResultSet { rows_affected: Some(1), ..ResultSet::default() };
        assert_eq!(render(&rs, Format::Table), "OK, 1 row affected\n");
    }

    #[test]
    fn csv_quotes_and_leaves_null_empty() {
        assert_eq!(render(&sample(), Format::Csv), "id,name\n1,\"a,b\"\n22,\n");
    }

    #[test]
    fn tsv_escapes_control_characters() {
        let rs = ResultSet {
            columns: vec!["x".into()],
            rows: vec![vec![Value::Text("a\tb\nc".into())]],
            rows_affected: None,
            truncated: false,
        };
        assert_eq!(render(&rs, Format::Tsv), "x\na\\tb\\nc\n");
    }

    #[test]
    fn json_keeps_column_order_and_real_nulls() {
        let text = render(&sample(), Format::Ndjson);
        assert_eq!(text, "{\"id\":1,\"name\":\"a,b\"}\n{\"id\":22,\"name\":null}\n");
        assert!(render(&sample(), Format::Json).starts_with("[\n  {\n    \"id\": 1,"));
    }

    #[test]
    fn markdown_escapes_pipes() {
        let rs = ResultSet {
            columns: vec!["x".into()],
            rows: vec![vec![Value::Text("a|b".into())]],
            rows_affected: None,
            truncated: false,
        };
        assert_eq!(render(&rs, Format::Markdown), "| x |\n| --- |\n| a\\|b |\n");
    }

    #[test]
    fn parses_format_names() {
        assert_eq!("MD".parse::<Format>().unwrap(), Format::Markdown);
        assert!("xml".parse::<Format>().is_err());
    }
}
