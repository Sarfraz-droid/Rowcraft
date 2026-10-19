//! Splits a script into statements for databases whose API runs one statement per request
//! (ClickHouse HTTP, Snowflake REST, BigQuery jobs, …).

/// Splits `sql` at top-level `;`. Quotes (`'…'`, `"…"`, `` `…` ``, with doubled-quote and
/// backslash escapes), `-- …` and `/* … */` comments are respected, and `$$ … $$` bodies stay in
/// one piece. Empty statements and statements that are only comments are dropped.
pub fn split_statements(sql: &str) -> Vec<&str> {
    let b = sql.as_bytes();
    let (mut out, mut start, mut i) = (Vec::new(), 0, 0);
    fn flush<'a>(sql: &'a str, from: usize, to: usize, out: &mut Vec<&'a str>) {
        let piece = sql[from..to].trim();
        if has_code(piece) {
            out.push(piece);
        }
    }
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' && q != b'`' {
                        i += 2;
                        continue;
                    }
                    if b[i] == q {
                        if b.get(i + 1) == Some(&q) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'$' if b.get(i + 1) == Some(&b'$') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'$' && b[i + 1] == b'$') {
                    i += 1;
                }
                i += 2;
            }
            b';' => {
                flush(sql, start, i, &mut out);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    flush(sql, start, sql.len(), &mut out);
    out
}

/// Whether `s` holds anything besides whitespace and comments.
fn has_code(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            _ => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::split_statements as split;

    #[test]
    fn splits_at_top_level_semicolons() {
        assert_eq!(split("select 1; select 2;"), ["select 1", "select 2"]);
        assert_eq!(split("select 1"), ["select 1"]);
        assert!(split("  ;; ").is_empty());
    }

    #[test]
    fn ignores_semicolons_in_quotes_comments_and_bodies() {
        assert_eq!(split("select 'a;b'; select \"c;d\""), ["select 'a;b'", "select \"c;d\""]);
        assert_eq!(split("select 'it''s;'; select 2"), ["select 'it''s;'", "select 2"]);
        assert_eq!(split("select 'a\\';b'; select 2"), ["select 'a\\';b'", "select 2"]);
        assert_eq!(split("select 1 -- x;y\n; select 2"), ["select 1 -- x;y", "select 2"]);
        assert_eq!(split("/* a;b */ select 1; select 2"), ["/* a;b */ select 1", "select 2"]);
        assert_eq!(
            split("create function f() as $$ a; b $$; select 1"),
            ["create function f() as $$ a; b $$", "select 1"]
        );
    }

    #[test]
    fn drops_comment_only_statements() {
        assert_eq!(split("select 1; -- done\n"), ["select 1"]);
        assert_eq!(split("select 1; /* c */"), ["select 1"]);
    }
}
