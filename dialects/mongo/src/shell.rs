//! A small parser for the part of the MongoDB shell that console users type:
//!
//! ```text
//! db.people.find({age: {$gt: 30}}, {name: 1}).sort({name: 1}).limit(10)
//! db.people.aggregate([{$match: {...}}, {$count: "n"}])
//! db.getCollectionNames()      show collections      show dbs      use other
//! {"ping": 1}                  (a bare document runs as a command)
//! ```
//!
//! Arguments are relaxed JSON: unquoted keys, single quotes, trailing commas, comments, regex
//! literals, and the shell constructors `ObjectId("…")`, `ISODate("…")`, `NumberLong(…)`,
//! `NumberInt(…)`, `NumberDecimal("…")`, `UUID("…")`. It is not JavaScript: no variables,
//! functions or arithmetic.

use mongodb::bson::{Bson, DateTime, Decimal128, Document, Regex, oid::ObjectId, raw::CString};

#[derive(Debug, PartialEq)]
pub enum Statement {
    ShowDatabases,
    ShowCollections,
    Use(String),
    /// `db.getCollectionNames()`, `db.runCommand(...)` and bare documents.
    Database {
        method: String,
        args: Vec<Bson>,
    },
    Collection {
        name: String,
        method: String,
        args: Vec<Bson>,
        chain: Vec<(String, Vec<Bson>)>,
    },
}

pub fn parse_statement(src: &str) -> Result<Statement, String> {
    let src = src.trim().trim_end_matches(';').trim();
    let lower = src.to_ascii_lowercase();
    match lower.as_str() {
        "show dbs" | "show databases" => return Ok(Statement::ShowDatabases),
        "show collections" | "show tables" => return Ok(Statement::ShowCollections),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix("use ") {
        let name = src[src.len() - rest.len()..].trim().trim_matches(|c| c == '"' || c == '\'');
        return Ok(Statement::Use(name.to_owned()));
    }
    if src.starts_with('{') {
        let mut p = Parser::new(src);
        let value = p.value()?;
        p.end()?;
        return Ok(Statement::Database { method: "runCommand".into(), args: vec![value] });
    }
    let mut p = Parser::new(src);
    p.expect_word("db")?;
    p.expect('.')?;
    let first = p.ident()?;
    // `db.method(...)`, `db.collection.method(...)` or `db.getCollection("a.b").method(...)`.
    let (collection, method) = if first == "getCollection" && p.peek() == Some('(') {
        let name = match p.arguments()?.as_slice() {
            [Bson::String(n)] => n.clone(),
            _ => return Err("getCollection() needs one collection name".into()),
        };
        p.expect('.')?;
        (Some(name), p.ident()?)
    } else if p.peek() == Some('(') {
        (None, first)
    } else {
        p.expect('.')?;
        (Some(first), p.ident()?)
    };
    let args = p.arguments()?;
    let mut chain = Vec::new();
    while p.peek() == Some('.') {
        p.expect('.')?;
        let name = p.ident()?;
        chain.push((name, p.arguments()?));
    }
    p.end()?;
    Ok(match collection {
        None => Statement::Database { method, args },
        Some(name) => Statement::Collection { name, method, args, chain },
    })
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { s: src.as_bytes(), src, i: 0 }
    }

    fn skip(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if self.s[self.i..].starts_with(b"/*") {
                self.i += 2;
                while self.i < self.s.len() && !self.s[self.i..].starts_with(b"*/") {
                    self.i += 1;
                }
                self.i = (self.i + 2).min(self.s.len());
            } else {
                return;
            }
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.s.get(self.i).map(|&b| b as char)
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{c}`")))
        }
    }

    fn expect_word(&mut self, w: &str) -> Result<(), String> {
        if self.ident()? == w { Ok(()) } else { Err(self.error(&format!("expected `{w}`"))) }
    }

    fn end(&mut self) -> Result<(), String> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(self.error("unexpected text after the statement")),
        }
    }

    fn error(&self, msg: &str) -> String {
        let at = self.i.min(self.src.len());
        let around: String = self.src[at..].chars().take(20).collect();
        format!("{msg} at position {at} (near `{around}`)")
    }

    fn ident(&mut self) -> Result<String, String> {
        self.skip();
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || matches!(self.s[self.i], b'_' | b'$'))
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(self.error("expected a name"));
        }
        Ok(self.src[start..self.i].to_owned())
    }

    fn arguments(&mut self) -> Result<Vec<Bson>, String> {
        self.expect('(')?;
        let mut args = Vec::new();
        loop {
            if self.peek() == Some(')') {
                self.i += 1;
                return Ok(args);
            }
            args.push(self.value()?);
            match self.peek() {
                Some(',') => self.i += 1,
                Some(')') => {}
                _ => return Err(self.error("expected `,` or `)`")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.skip();
        let quote = self.s[self.i];
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(&b) = self.s.get(self.i) else {
                return Err(self.error("unterminated string"));
            };
            if b == quote {
                self.i += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.i += 1;
                let Some(&e) = self.s.get(self.i) else {
                    return Err(self.error("unterminated string"));
                };
                self.i += 1;
                match e {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'u' => {
                        let hex = self
                            .src
                            .get(self.i..self.i + 4)
                            .ok_or_else(|| self.error("bad \\u escape"))?;
                        let code = u32::from_str_radix(hex, 16)
                            .map_err(|_| self.error("bad \\u escape"))?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        self.i += 4;
                    }
                    other => out.push(other as char),
                }
            } else {
                // Copy a whole UTF-8 character.
                let ch = self.src[self.i..].chars().next().unwrap_or('\u{fffd}');
                out.push(ch);
                self.i += ch.len_utf8();
            }
        }
    }

    fn value(&mut self) -> Result<Bson, String> {
        match self.peek() {
            None => Err(self.error("expected a value")),
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"' | '\'') => Ok(Bson::String(self.string()?)),
            Some('/') => self.regex(),
            Some(c) if c == '-' || c == '+' || c.is_ascii_digit() || c == '.' => self.number(),
            Some(_) => {
                let start = self.i;
                let word = self.ident()?;
                match word.as_str() {
                    "true" => Ok(Bson::Boolean(true)),
                    "false" => Ok(Bson::Boolean(false)),
                    "null" | "undefined" => Ok(Bson::Null),
                    "NaN" => Ok(Bson::Double(f64::NAN)),
                    "Infinity" => Ok(Bson::Double(f64::INFINITY)),
                    "new" => self.value(),
                    _ if self.peek() == Some('(') => {
                        let args = self.arguments()?;
                        constructor(&word, &args).map_err(|e| format!("{e} (at position {start})"))
                    }
                    other => Err(self.error(&format!("unknown name `{other}`"))),
                }
            }
        }
    }

    fn number(&mut self) -> Result<Bson, String> {
        let start = self.i;
        if matches!(self.s[self.i], b'-' | b'+') {
            self.i += 1;
        }
        if self.s[self.i..].starts_with(b"Infinity") {
            self.i += 8;
            return Ok(Bson::Double(if self.s[start] == b'-' {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }));
        }
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_digit()
                || matches!(self.s[self.i], b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            // A sign only continues the number straight after an exponent marker.
            if matches!(self.s[self.i], b'+' | b'-') && !matches!(self.s[self.i - 1], b'e' | b'E') {
                break;
            }
            self.i += 1;
        }
        let text = self.src[start..self.i].trim_start_matches('+');
        if let Ok(n) = text.parse::<i64>() {
            return Ok(i32::try_from(n).map_or(Bson::Int64(n), Bson::Int32));
        }
        text.parse::<f64>().map(Bson::Double).map_err(|_| self.error("bad number"))
    }

    fn regex(&mut self) -> Result<Bson, String> {
        self.i += 1;
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != b'/' {
            if self.s[self.i] == b'\\' {
                self.i += 1;
            }
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return Err(self.error("unterminated regular expression"));
        }
        let pattern = self.src[start..self.i].to_owned();
        self.i += 1;
        let flags_start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_alphabetic() {
            self.i += 1;
        }
        // The server wants the option letters in alphabetical order.
        let mut flags: Vec<char> = self.src[flags_start..self.i].chars().collect();
        flags.sort_unstable();
        let nul = |_| self.error("regular expressions cannot contain NUL");
        Ok(Bson::RegularExpression(Regex {
            pattern: CString::try_from(pattern).map_err(nul)?,
            options: CString::try_from(flags.into_iter().collect::<String>()).map_err(nul)?,
        }))
    }

    fn array(&mut self) -> Result<Bson, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            if self.peek() == Some(']') {
                self.i += 1;
                return Ok(Bson::Array(items));
            }
            items.push(self.value()?);
            match self.peek() {
                Some(',') => self.i += 1,
                Some(']') => {}
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn object(&mut self) -> Result<Bson, String> {
        self.expect('{')?;
        let mut doc = Document::new();
        loop {
            if self.peek() == Some('}') {
                self.i += 1;
                return Ok(Bson::Document(doc));
            }
            let key = match self.peek() {
                Some('"' | '\'') => self.string()?,
                _ => self.ident()?,
            };
            self.expect(':')?;
            doc.insert(key, self.value()?);
            match self.peek() {
                Some(',') => self.i += 1,
                Some('}') => {}
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }
}

/// Parses one relaxed-JSON document, e.g. the text of an index key.
pub fn parse_document(src: &str) -> Result<Document, String> {
    let mut p = Parser::new(src);
    match p.value()? {
        Bson::Document(d) => {
            p.end()?;
            Ok(d)
        }
        _ => Err("expected a document".into()),
    }
}

fn constructor(name: &str, args: &[Bson]) -> Result<Bson, String> {
    let text = |i: usize| match args.get(i) {
        Some(Bson::String(s)) => Ok(s.clone()),
        Some(Bson::Int32(n)) => Ok(n.to_string()),
        Some(Bson::Int64(n)) => Ok(n.to_string()),
        _ => Err(format!("{name}() needs a string argument")),
    };
    match name {
        "ObjectId" => match args.first() {
            None => Ok(Bson::ObjectId(ObjectId::new())),
            Some(_) => ObjectId::parse_str(text(0)?)
                .map(Bson::ObjectId)
                .map_err(|e| format!("ObjectId: {e}")),
        },
        "ISODate" | "Date" => match args.first() {
            None => Ok(Bson::DateTime(DateTime::now())),
            Some(Bson::Int32(_) | Bson::Int64(_)) => {
                let ms = match &args[0] {
                    Bson::Int32(n) => i64::from(*n),
                    Bson::Int64(n) => *n,
                    _ => 0,
                };
                Ok(Bson::DateTime(DateTime::from_millis(ms)))
            }
            Some(_) => DateTime::parse_rfc3339_str(text(0)?).map(Bson::DateTime).map_err(|e| {
                format!("{name}: {e} (use an RFC 3339 date such as 2026-01-31T10:00:00Z)")
            }),
        },
        "NumberInt" | "Int32" => {
            text(0)?.parse::<i32>().map(Bson::Int32).map_err(|e| format!("{name}: {e}"))
        }
        "NumberLong" | "Long" | "Int64" => {
            text(0)?.parse::<i64>().map(Bson::Int64).map_err(|e| format!("{name}: {e}"))
        }
        "NumberDecimal" | "Decimal128" => {
            text(0)?.parse::<Decimal128>().map(Bson::Decimal128).map_err(|e| format!("{name}: {e}"))
        }
        "Double" => match args.first() {
            Some(Bson::Int32(n)) => Ok(Bson::Double(f64::from(*n))),
            Some(Bson::Int64(n)) => Ok(Bson::Double(*n as f64)),
            Some(Bson::Double(d)) => Ok(Bson::Double(*d)),
            _ => Err("Double() needs a number".into()),
        },
        "UUID" => {
            let hex: String = text(0)?.chars().filter(|c| *c != '-').collect();
            if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("UUID() needs 32 hex digits".into());
            }
            let bytes = (0..16)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0))
                .collect();
            Ok(Bson::Binary(mongodb::bson::Binary {
                subtype: mongodb::bson::spec::BinarySubtype::Uuid,
                bytes,
            }))
        }
        other => Err(format!("unknown function `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::doc;

    fn args(src: &str) -> Vec<Bson> {
        match parse_statement(src).unwrap() {
            Statement::Collection { args, .. } | Statement::Database { args, .. } => args,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn plain_statements() {
        assert_eq!(parse_statement("show dbs").unwrap(), Statement::ShowDatabases);
        assert_eq!(parse_statement("SHOW COLLECTIONS;").unwrap(), Statement::ShowCollections);
        assert_eq!(parse_statement("use 'shop'").unwrap(), Statement::Use("shop".into()));
        assert_eq!(
            parse_statement("db.getCollectionNames()").unwrap(),
            Statement::Database { method: "getCollectionNames".into(), args: vec![] }
        );
    }

    #[test]
    fn collection_calls_with_chains() {
        let Statement::Collection { name, method, args, chain } = parse_statement(
            "db.people.find({age: {$gt: 30}}, {name: 1}).sort({name: -1}).limit(5);",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!((name.as_str(), method.as_str()), ("people", "find"));
        assert_eq!(args[0], Bson::Document(doc! {"age": {"$gt": 30}}));
        assert_eq!(chain[0].0, "sort");
        assert_eq!(chain[1], ("limit".to_owned(), vec![Bson::Int32(5)]));
        let Statement::Collection { name, method, .. } =
            parse_statement("db.getCollection('a.b').countDocuments({})").unwrap()
        else {
            panic!()
        };
        assert_eq!((name.as_str(), method.as_str()), ("a.b", "countDocuments"));
    }

    #[test]
    fn relaxed_json() {
        let a = args(
            "db.t.insertOne({a: 'x', \"b\": [1, 2.5, -3,], c: null, d: true, /* note */ e: 1e3,})",
        );
        assert_eq!(
            a[0],
            Bson::Document(
                doc! {"a": "x", "b": [1, 2.5, -3], "c": Bson::Null, "d": true, "e": 1000.0}
            )
        );
        assert_eq!(
            args("db.t.find({n: 3000000000})")[0],
            Bson::Document(doc! {"n": 3_000_000_000_i64})
        );
    }

    #[test]
    fn constructors_and_regex() {
        let a = args(
            r#"db.t.find({_id: ObjectId("507f1f77bcf86cd799439011"), at: ISODate("2026-01-02T03:04:05Z"), n: NumberLong("5"), r: /^ab.c/i})"#,
        );
        let d = a[0].as_document().unwrap();
        assert_eq!(d.get_object_id("_id").unwrap().to_hex(), "507f1f77bcf86cd799439011");
        assert_eq!(d.get_datetime("at").unwrap().timestamp_millis(), 1_767_323_045_000);
        assert_eq!(d.get_i64("n").unwrap(), 5);
        assert!(
            matches!(d.get("r"), Some(Bson::RegularExpression(r)) if r.pattern.as_str() == "^ab.c" && r.options.as_str() == "i")
        );
        assert!(parse_statement("db.t.find({_id: ObjectId('zz')})").is_err());
    }

    #[test]
    fn bare_document_is_a_command() {
        assert_eq!(
            parse_statement(r#"{"ping": 1}"#).unwrap(),
            Statement::Database {
                method: "runCommand".into(),
                args: vec![Bson::Document(doc! {"ping": 1})]
            }
        );
    }

    #[test]
    fn errors_point_at_the_problem() {
        let e = parse_statement("db.t.find({a: })").unwrap_err();
        assert!(e.contains("position"), "{e}");
        assert!(parse_statement("db.t.find(").is_err());
        assert!(parse_statement("db.t.find() garbage").is_err());
        assert!(parse_statement("select 1").is_err());
    }
}
