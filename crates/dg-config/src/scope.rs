//! The Schemas tab: which schemas to introspect and which objects to show.
//!
//! Grammar (from DataGrip's own hint text): a schema pattern is `db1:sc1,sc2|db2:sc3` where each
//! name is `@` (the default database/schema), `*` (all), `{regexp}` (names matching) or a literal.
//! An entry without `:` lists schemas in the default database. An object filter is a pattern or
//! `type:[-]pattern||type:[-]pattern`, the minus excluding; patterns are regular expressions
//! that must match the whole name.

use regex::Regex;

/// Splits at `sep` characters that are not inside `{...}`, so regexes may contain them.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn anchored(pattern: &str) -> Result<Regex, String> {
    Regex::new(&format!("^(?:{pattern})$"))
        .map_err(|e| format!("bad regular expression `{pattern}`: {e}"))
}

#[derive(Debug, Clone)]
enum Sel {
    Default,
    All,
    Name(String),
    Pattern(Regex),
}

impl Sel {
    fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        match text {
            "" => Err("empty name in the pattern".into()),
            "@" => Ok(Self::Default),
            "*" => Ok(Self::All),
            t if t.starts_with('{') && t.ends_with('}') && t.len() > 2 => {
                anchored(&t[1..t.len() - 1]).map(Self::Pattern)
            }
            t => Ok(Self::Name(t.to_owned())),
        }
    }

    fn matches(&self, name: &str, default: Option<&str>) -> bool {
        match self {
            Self::Default => default == Some(name),
            Self::All => true,
            Self::Name(n) => n == name,
            Self::Pattern(re) => re.is_match(name),
        }
    }
}

/// What `@` stands for in the connected database.
#[derive(Debug, Clone, Copy, Default)]
pub struct Defaults<'a> {
    pub database: Option<&'a str>,
    pub schema: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct SchemaPattern {
    entries: Vec<(Sel, Vec<Sel>)>,
}

impl SchemaPattern {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for part in split_top(text, '|') {
            let part = part.trim();
            let colon = split_top(part, ':');
            let (db, schemas) = match colon.as_slice() {
                [schemas] => ("@", *schemas),
                [db, schemas] => (*db, *schemas),
                _ => return Err(format!("`{part}`: expected database:schemas")),
            };
            let schemas = split_top(schemas, ',')
                .into_iter()
                .map(Sel::parse)
                .collect::<Result<Vec<_>, _>>()?;
            entries.push((Sel::parse(db)?, schemas));
        }
        Ok(Self { entries })
    }

    pub fn matches(&self, database: &str, schema: &str, defaults: Defaults<'_>) -> bool {
        self.entries.iter().any(|(db, schemas)| {
            db.matches(database, defaults.database)
                && schemas.iter().any(|s| s.matches(schema, defaults.schema))
        })
    }
}

#[derive(Debug, Clone)]
struct Rule {
    kind: Option<String>,
    exclude: bool,
    pattern: Regex,
}

#[derive(Debug, Clone)]
pub struct ObjectFilter {
    rules: Vec<Rule>,
}

impl ObjectFilter {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rules = Vec::new();
        for part in text.split("||") {
            let part = part.trim();
            if part.is_empty() {
                return Err("empty rule in the object filter".into());
            }
            // `type:pattern` only when what precedes the colon looks like a type name.
            let (kind, rest) = match part.split_once(':') {
                Some((k, r))
                    if !k.is_empty()
                        && k.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == ' ') =>
                {
                    (Some(k.trim().to_lowercase()), r)
                }
                _ => (None, part),
            };
            let (exclude, pattern) = match rest.strip_prefix('-') {
                Some(p) => (true, p),
                None => (false, rest),
            };
            rules.push(Rule { kind, exclude, pattern: anchored(pattern)? });
        }
        Ok(Self { rules })
    }

    /// Whether the object of `kind` (`table`, `view`, `routine`, …) called `name` is shown.
    pub fn allows(&self, kind: &str, name: &str) -> bool {
        let kind = kind.to_lowercase();
        let applies = |r: &&Rule| r.kind.as_deref().is_none_or(|k| k == kind);
        let mut included = None;
        for rule in self.rules.iter().filter(applies) {
            let hit = rule.pattern.is_match(name);
            if rule.exclude && hit {
                return false;
            }
            if !rule.exclude {
                included = Some(included.unwrap_or(false) || hit);
            }
        }
        included.unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Defaults<'static> = Defaults { database: Some("app"), schema: Some("public") };

    fn pat(text: &str) -> SchemaPattern {
        SchemaPattern::parse(text).unwrap()
    }

    #[test]
    fn the_examples_from_the_datagrip_hint() {
        assert!(pat("*:*").matches("any", "thing", D));
        // `@:*` everything from the default database
        assert!(pat("@:*").matches("app", "x", D) && !pat("@:*").matches("other", "x", D));
        // `@:@` the default schema
        assert!(pat("@:@").matches("app", "public", D) && !pat("@:@").matches("app", "x", D));
        // `{t.*}:@` default schema in databases starting with t
        let p = pat("{t.*}:@");
        assert!(p.matches("test", "public", D) && !p.matches("app", "public", D));
        // the combined example: dbo everywhere, the default schema, three schemas from db1
        let p = pat("*:dbo|@:@|db1:s1,s2,s3");
        assert!(p.matches("zzz", "dbo", D));
        assert!(p.matches("app", "public", D));
        assert!(p.matches("db1", "s2", D) && !p.matches("db1", "s4", D));
        assert!(!p.matches("db2", "s1", D));
    }

    #[test]
    fn an_entry_without_a_database_means_the_default_database() {
        let p = pat("public,{tmp_.*}");
        assert!(p.matches("app", "public", D) && p.matches("app", "tmp_1", D));
        assert!(!p.matches("other", "public", D));
    }

    #[test]
    fn regexes_may_contain_the_separators() {
        let p = pat("*:{a|b},c");
        assert!(p.matches("d", "a", D) && p.matches("d", "b", D) && p.matches("d", "c", D));
        assert!(!p.matches("d", "ab", D), "patterns match the whole name");
    }

    #[test]
    fn at_needs_a_known_default() {
        assert!(!pat("*:@").matches("app", "public", Defaults::default()));
    }

    #[test]
    fn bad_patterns_are_rejected_with_a_reason() {
        for bad in ["", "a:b:c", "*:", "*:{(}", "a||b"] {
            assert!(SchemaPattern::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn object_filter_examples_from_the_hint() {
        let f = ObjectFilter::parse("table:CUR_.*").unwrap();
        assert!(f.allows("table", "CUR_1") && !f.allows("table", "OTHER"));
        assert!(f.allows("view", "OTHER"), "a rule for tables leaves other kinds alone");

        let f = ObjectFilter::parse("table:-OLD_.*||routine:DO.*||view:V_.*").unwrap();
        assert!(f.allows("table", "T") && !f.allows("table", "OLD_T"));
        assert!(f.allows("routine", "DOIT") && !f.allows("routine", "other"));
        assert!(f.allows("view", "V_1") && !f.allows("view", "w"));
    }

    #[test]
    fn a_bare_pattern_applies_to_every_kind() {
        let f = ObjectFilter::parse("-tmp.*").unwrap();
        assert!(!f.allows("table", "tmp1") && !f.allows("view", "tmp2") && f.allows("table", "t"));
        let f = ObjectFilter::parse("a.*").unwrap();
        assert!(f.allows("table", "abc") && !f.allows("view", "xyz"));
    }

    #[test]
    fn exclusion_beats_inclusion() {
        let f = ObjectFilter::parse("table:t.*||table:-tmp.*").unwrap();
        assert!(f.allows("table", "t1") && !f.allows("table", "tmp"));
    }

    #[test]
    fn bad_filters_are_rejected() {
        assert!(ObjectFilter::parse("table:(").is_err());
        assert!(ObjectFilter::parse("a||").is_err());
    }
}
