//! The General tab URL overrides host, port, database and user.
//!
//! Accepts `scheme://[user@]host[:port][/database][?params]`, with an optional `jdbc:` prefix so
//! URLs copied from JDBC tools work. A password in the URL is refused to prevent credential exposure
//! too, and ours would end up in `datasources.toml`.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlParts {
    pub scheme: String,
    pub user: Option<String>,
    /// Several comma-separated hosts (replica sets) are kept as one string, without a port.
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
}

fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3).ok_or("truncated %-escape in the URL")?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| format!("bad %-escape `%{hex}`"))?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "the URL is not valid UTF-8 after decoding".into())
}

fn non_empty(text: &str) -> Result<Option<String>, String> {
    Ok(Some(percent_decode(text)?).filter(|s| !s.is_empty()))
}

pub fn parse(url: &str) -> Result<UrlParts, String> {
    let url = url.trim();
    // JDBC URLs include opaque forms such as jdbc:h2:mem:test and
    // jdbc:oracle:thin:@host:1521/service. Leave their interpretation to JDBC.
    let jdbc = url.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("jdbc:"));
    if jdbc {
        let lower = url.to_ascii_lowercase();
        if let Some(oracle) = lower.strip_prefix("jdbc:oracle:thin:") {
            if let Some((credentials, _)) = oracle.split_once('@') {
                if credentials.contains('/') {
                    return Err(
                        "Avoid a password in the URL: enter it in the separate Password field."
                            .into(),
                    );
                }
            }
        }
        if lower.split(['?', '&', ';']).any(|p| {
            p.split_once('=')
                .is_some_and(|(key, _)| matches!(key.trim(), "password" | "passwd" | "pwd"))
        }) {
            return Err(
                "Avoid a password in the URL: enter it in the separate Password field.".into()
            );
        }
        if !url[5..].contains("://") || url[5..].matches("://").count() > 1 {
            let (scheme, rest) =
                url[5..].split_once(':').ok_or("expected jdbc:subprotocol:target")?;
            if scheme.is_empty()
                || rest.is_empty()
                || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err("expected jdbc:subprotocol:target".into());
            }
            return Ok(UrlParts { scheme: scheme.to_owned(), ..Default::default() });
        }
    }
    let url = match url.get(..5) {
        Some(p) if p.eq_ignore_ascii_case("jdbc:") => &url[5..],
        _ => url,
    };
    let (scheme, rest) =
        url.split_once("://").ok_or("expected a URL like scheme://host:port/database")?;
    if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.:".contains(c))
    {
        return Err(format!("`{scheme}` is not a valid URL scheme"));
    }
    let (rest, query) = rest
        .split_once(if jdbc { ['?', ';'] } else { ['?', '?'] })
        .map_or((rest, ""), |(a, q)| (a, q));
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));

    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    let mut parts = UrlParts { scheme: scheme.to_owned(), ..UrlParts::default() };
    if let Some(info) = userinfo {
        if info.contains(':') {
            return Err(
                "Avoid a password in the URL: enter it in the separate Password field.".into()
            );
        }
        parts.user = non_empty(info)?;
    }

    if let Some(v6) = hostport.strip_prefix('[') {
        let (host, after) = v6.split_once(']').ok_or("unclosed `[` in the host")?;
        parts.host = Some(host.to_owned());
        if let Some(port) = after.strip_prefix(':') {
            parts.port = Some(parse_port(port)?);
        } else if !after.is_empty() {
            return Err(format!("unexpected `{after}` after the host"));
        }
    } else if hostport.contains(',') {
        parts.host = Some(hostport.to_owned());
    } else {
        match hostport.rsplit_once(':') {
            Some((host, port)) => {
                parts.host = non_empty(host)?;
                parts.port = Some(parse_port(port)?);
            }
            None => parts.host = non_empty(hostport)?,
        }
    }
    parts.database = non_empty(path)?;
    if parts.user.is_none() {
        for pair in query.split(['&', ';']) {
            if let Some(user) = pair.strip_prefix("user=") {
                parts.user = non_empty(user)?;
            }
        }
    }
    Ok(parts)
}

fn parse_port(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(format!("port `{text}` must be a number from 1 to 65535")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(url: &str) -> UrlParts {
        parse(url).unwrap()
    }

    #[test]
    fn full_url_with_and_without_the_jdbc_prefix() {
        let expected = UrlParts {
            scheme: "postgresql".into(),
            user: Some("me".into()),
            host: Some("db.example.com".into()),
            port: Some(5433),
            database: Some("app".into()),
        };
        assert_eq!(ok("postgresql://me@db.example.com:5433/app"), expected);
        assert_eq!(ok("jdbc:postgresql://me@db.example.com:5433/app?sslmode=require"), expected);
        assert_eq!(ok("JDBC:postgresql://me@db.example.com:5433/app"), expected);
    }

    #[test]
    fn opaque_jdbc_urls_are_preserved_for_the_driver() {
        for url in [
            "jdbc:h2:mem:test",
            "jdbc:oracle:thin:@localhost:1521/xe",
            "jdbc:sqlite:/tmp/data.db",
            "jdbc:derby:memory:db;create=true",
        ] {
            assert!(parse(url).is_ok(), "{url}");
        }
        assert!(parse("jdbc:sqlserver://localhost:1433;databaseName=app").is_ok());
        assert!(parse("jdbc:h2:mem:test;PASSWORD=secret").is_err());
        assert!(
            parse("jdbc:bigquery://https://www.googleapis.com/bigquery/v2;ProjectId=x").is_ok()
        );
        assert!(parse("jdbc:es://https://localhost:9200").is_ok());
        assert!(parse("jdbc:oracle:thin:user/secret@host:1521:db").is_err());
        assert!(parse("jdbc:").is_err());
    }

    #[test]
    fn missing_pieces_stay_none() {
        let p = ok("mysql://localhost");
        assert_eq!(
            (p.host.as_deref(), p.port, p.database, p.user),
            (Some("localhost"), None, None, None)
        );
        assert_eq!(ok("mysql://localhost/").database, None);
        assert_eq!(ok("redis://:6380").host, None);
    }

    #[test]
    fn ipv6_and_multiple_hosts() {
        let p = ok("postgresql://[::1]:5432/db");
        assert_eq!((p.host.as_deref(), p.port), (Some("::1"), Some(5432)));
        assert_eq!(ok("postgresql://[::1]/db").port, None);
        let p = ok("mongodb://a:27017,b:27018/app");
        assert_eq!((p.host.as_deref(), p.port), (Some("a:27017,b:27018"), None));
    }

    #[test]
    fn percent_escapes_and_user_in_the_query() {
        assert_eq!(ok("pg://u%40x@h/my%20db").user.as_deref(), Some("u@x"));
        assert_eq!(ok("pg://u%40x@h/my%20db").database.as_deref(), Some("my db"));
        assert_eq!(ok("jdbc:mysql://h/db?user=bob&x=1").user.as_deref(), Some("bob"));
    }

    #[test]
    fn a_password_in_the_url_is_refused() {
        let err = parse("postgresql://me:secret@h/db").unwrap_err();
        assert!(err.contains("password") && !err.contains("secret"), "{err}");
    }

    #[test]
    fn malformed_urls_explain_themselves() {
        for bad in [
            "",
            "localhost:5432",
            "://h",
            "pg://h:99999",
            "pg://h:0",
            "pg://[::1",
            "pg://h/%zz",
            "pg://[::1]x/db",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }
}
