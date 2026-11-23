//! A read-only check on SQL text. It catches an assistant running `DELETE` or `DROP` by mistake;
//! it does not stop a determined caller (a function with side effects, a vendor extension).
//! Real enforcement is a `read_only` data source or a read-only database user.

/// Statements that start with one of these read data.
const READ_STARTS: [&str; 8] =
    ["select", "with", "show", "describe", "desc", "explain", "values", "table"];

/// Words that change data, schema, privileges or server state. Any of them anywhere in the
/// statement (outside strings, quoted names and comments) refuses it.
const WRITE_WORDS: [&str; 24] = [
    "insert", "update", "delete", "merge", "replace", "upsert", "drop", "alter", "create",
    "truncate", "rename", "grant", "revoke", "call", "exec", "execute", "into", "copy", "load",
    "vacuum", "attach", "detach", "lock", "set",
];

/// `Ok` when `sql` is a single read-only statement; otherwise why it was refused.
pub fn check_read_only(sql: &str) -> Result<(), String> {
    let words = words(sql)?;
    let Some(first) = words.first() else {
        return Err("no statement found".to_owned());
    };
    if !READ_STARTS.contains(&first.as_str()) {
        return Err(refusal(&format!("`{}` statements are not read-only", first.to_uppercase())));
    }
    if let Some(bad) = words.iter().find(|w| WRITE_WORDS.contains(&w.as_str())) {
        return Err(refusal(&format!("`{}` can change data", bad.to_uppercase())));
    }
    if words.iter().any(|w| w == ";") {
        return Err(refusal("only one statement per call is allowed"));
    }
    Ok(())
}

fn refusal(why: &str) -> String {
    format!(
        "refused: {why}. This server is read-only; restart it with --allow-write \
         (`rowcraft mcp --allow-write`) to let the assistant change data."
    )
}

/// Lower-cased words of `sql` with strings, quoted names and comments removed. A `";"` entry
/// marks a statement separator that is followed by more text.
fn words(sql: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut separator_seen = false;
    let mut i = 0;
    // Pushes the pending word, or a separator marker first when text follows a `;`.
    let emit = |token: Option<String>, out: &mut Vec<String>, separator: &mut bool| {
        if let Some(token) = token {
            if *separator {
                out.push(";".to_owned());
                *separator = false;
            }
            out.push(token);
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let word_char = c.is_alphanumeric() || c == '_';
        if word_char {
            word.push(c);
            i += 1;
            continue;
        }
        if !word.is_empty() {
            emit(Some(std::mem::take(&mut word).to_lowercase()), &mut out, &mut separator_seen);
        }
        match c {
            '-' if next == Some('-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            '\'' | '"' | '`' => {
                i = skip_quoted(&chars, i, c)?;
                // A string or quoted name is text too, but not a word to match against.
                emit(Some(String::new()), &mut out, &mut separator_seen);
            }
            ';' => {
                separator_seen = !out.is_empty();
                i += 1;
            }
            _ => i += 1,
        }
    }
    if !word.is_empty() {
        emit(Some(word.to_lowercase()), &mut out, &mut separator_seen);
    }
    Ok(out)
}

/// Index just past the quote that closes the one at `start`; a doubled quote is an escape.
fn skip_quoted(chars: &[char], start: usize, quote: char) -> Result<usize, String> {
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '\\' && quote == '\'' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            if chars.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return Ok(i + 1);
        }
        i += 1;
    }
    Err("unterminated quote in the statement".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pass() {
        for sql in [
            "SELECT * FROM people LIMIT 10",
            "  select 1;",
            "SELECT 1; -- trailing comment",
            "WITH a AS (SELECT 1) SELECT * FROM a",
            "EXPLAIN SELECT * FROM t",
            "SHOW TABLES",
            "SELECT update_time, delete_flag FROM t",
            "SELECT 'DROP TABLE x; DELETE' AS note",
            "SELECT \"insert\" FROM t",
            "/* DELETE */ SELECT 1",
        ] {
            assert_eq!(check_read_only(sql), Ok(()), "{sql}");
        }
    }

    #[test]
    fn writes_are_refused() {
        for sql in [
            "DELETE FROM people",
            "insert into t values (1)",
            "DROP TABLE t",
            "UPDATE t SET a = 1",
            "SELECT 1; DROP TABLE t",
            "SELECT 1; 'x'",
            "SELECT * INTO copy FROM t",
            "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d",
            "EXPLAIN ANALYZE DELETE FROM t",
            "SET search_path = x",
            "CALL cleanup()",
            "",
            "-- only a comment",
        ] {
            assert!(check_read_only(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn unterminated_quote_is_refused() {
        assert!(check_read_only("SELECT 'oops").is_err());
    }
}
