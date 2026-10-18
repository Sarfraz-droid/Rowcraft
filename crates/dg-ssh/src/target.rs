//! Which remote host:port pairs need a tunnel. A data source names one, or several separated by
//! commas for clusters (`a:27017,b:27018`); DataGrip extracts every pair and tunnels each.

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
}

fn port(text: &str) -> Result<u16> {
    match text.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(Error::Target(format!("port `{text}` must be a number from 1 to 65535"))),
    }
}

/// Splits `host` into targets. An entry without its own port uses `port`, then `default_port`;
/// a bracketed IPv6 address may carry one (`[::1]:5432`), a bare one never does.
pub fn parse_targets(
    host: &str,
    port_hint: Option<u16>,
    default_port: Option<u16>,
) -> Result<Vec<Target>> {
    let fallback = port_hint.or(default_port);
    let mut targets = Vec::new();
    for entry in host.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            return Err(Error::Target(format!("empty host in `{host}`")));
        }
        let (name, own_port) = if let Some(rest) = entry.strip_prefix('[') {
            let (name, after) = rest
                .split_once(']')
                .ok_or_else(|| Error::Target(format!("unclosed `[` in `{entry}`")))?;
            match after.strip_prefix(':') {
                Some(p) => (name, Some(port(p)?)),
                None if after.is_empty() => (name, None),
                None => return Err(Error::Target(format!("unexpected `{after}` after `{name}`"))),
            }
        } else if entry.matches(':').count() == 1 {
            let (name, p) = entry.split_once(':').unwrap_or((entry, ""));
            (name, Some(port(p)?))
        } else {
            (entry, None) // a name, or a bare IPv6 address
        };
        if name.is_empty() {
            return Err(Error::Target(format!("empty host in `{entry}`")));
        }
        let port = own_port.or(fallback).ok_or_else(|| {
            Error::Target(format!(
                "no port for `{entry}`: give one or choose a database with a default"
            ))
        })?;
        targets.push(Target { host: name.to_owned(), port });
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(host: &str, port: u16) -> Target {
        Target { host: host.into(), port }
    }

    #[test]
    fn one_host_takes_the_hint_then_the_default() {
        assert_eq!(parse_targets("db", Some(1), Some(2)).unwrap(), [t("db", 1)]);
        assert_eq!(parse_targets("db", None, Some(2)).unwrap(), [t("db", 2)]);
        assert!(parse_targets("db", None, None).unwrap_err().to_string().contains("no port"));
    }

    #[test]
    fn a_cluster_gets_one_target_per_node() {
        assert_eq!(
            parse_targets("a:27017, b:27018 ,c", None, Some(27017)).unwrap(),
            [t("a", 27017), t("b", 27018), t("c", 27017)]
        );
    }

    #[test]
    fn ipv6_is_bracketed_to_carry_a_port() {
        assert_eq!(parse_targets("[::1]:5432", None, None).unwrap(), [t("::1", 5432)]);
        assert_eq!(parse_targets("[fe80::1]", Some(9), None).unwrap(), [t("fe80::1", 9)]);
        assert_eq!(parse_targets("::1", Some(9), None).unwrap(), [t("::1", 9)]);
    }

    #[test]
    fn malformed_entries_are_refused() {
        for bad in ["", "a,,b", ":5432", "a:0", "a:99999", "a:x", "[::1", "[::1]x", "[]:1"] {
            assert!(parse_targets(bad, Some(1), None).is_err(), "{bad:?}");
        }
    }
}
