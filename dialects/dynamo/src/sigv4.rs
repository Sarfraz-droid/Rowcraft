//! AWS Signature Version 4 for a JSON-protocol POST to `/` with no query string.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub struct Signed {
    pub amz_date: String,
    pub authorization: String,
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    // HMAC accepts keys of any length, so `new_from_slice` cannot fail.
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap_or_else(|_| unreachable!());
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date);
    let k_region = hmac(&k_date, region);
    let k_service = hmac(&k_region, service);
    hmac(&k_service, "aws4_request")
}

/// `now` is Unix seconds; it is a parameter so tests can pin the clock.
#[allow(clippy::too_many_arguments)]
pub fn sign(
    access_key: &str,
    secret_key: &str,
    session_token: Option<&str>,
    region: &str,
    host: &str,
    target: &str,
    payload: &[u8],
    now: u64,
) -> Signed {
    let amz_date = format_time(now);
    let date = &amz_date[..8];
    let payload_hash = hex(&Sha256::digest(payload));

    // Headers must be sorted by lowercase name.
    let mut headers: Vec<(&str, String)> = vec![
        ("content-type", "application/x-amz-json-1.0".into()),
        ("host", host.to_owned()),
        ("x-amz-date", amz_date.clone()),
        ("x-amz-target", target.to_owned()),
    ];
    if let Some(token) = session_token {
        headers.push(("x-amz-security-token", token.to_owned()));
    }
    headers.sort_by(|a, b| a.0.cmp(b.0));
    let canonical_headers: String =
        headers.iter().map(|(k, v)| format!("{k}:{}\n", v.trim())).collect();
    let signed_headers = headers.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(";");
    let canonical = format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");

    let scope = format!("{date}/{region}/dynamodb/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical.as_bytes()))
    );
    let signature = hex(&hmac(&signing_key(secret_key, date, region, "dynamodb"), &to_sign));
    Signed {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
        ),
        amz_date,
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `20150830T123600Z` from Unix seconds (proleptic Gregorian, UTC).
fn format_time(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_matches_the_aws_documentation_example() {
        // From "Examples of how to derive a signing key" in the AWS General Reference.
        let key =
            signing_key("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", "20150830", "us-east-1", "iam");
        assert_eq!(hex(&key), "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9");
    }

    #[test]
    fn formats_unix_time_as_amz_date() {
        assert_eq!(format_time(0), "19700101T000000Z");
        assert_eq!(format_time(1_440_938_160), "20150830T123600Z");
        assert_eq!(format_time(1_709_164_800 + 86_399), "20240229T235959Z"); // leap day
    }

    #[test]
    fn authorization_header_has_the_expected_shape() {
        let s = sign(
            "AKID",
            "secret",
            Some("tok"),
            "eu-west-1",
            "dynamodb.eu-west-1.amazonaws.com",
            "DynamoDB_20120810.ListTables",
            b"{}",
            1_440_938_160,
        );
        assert_eq!(s.amz_date, "20150830T123600Z");
        assert!(s.authorization.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKID/20150830/eu-west-1/dynamodb/aws4_request, "
        ));
        assert!(s.authorization.contains(
            "SignedHeaders=content-type;host;x-amz-date;x-amz-security-token;x-amz-target, "
        ));
        let sig = s.authorization.rsplit("Signature=").next().unwrap();
        assert_eq!(sig.len(), 64);
        // Deterministic for a pinned clock, and sensitive to the payload.
        let again = sign(
            "AKID",
            "secret",
            Some("tok"),
            "eu-west-1",
            "dynamodb.eu-west-1.amazonaws.com",
            "DynamoDB_20120810.ListTables",
            b"{}",
            1_440_938_160,
        );
        assert_eq!(s.authorization, again.authorization);
        let other = sign(
            "AKID",
            "secret",
            Some("tok"),
            "eu-west-1",
            "dynamodb.eu-west-1.amazonaws.com",
            "DynamoDB_20120810.ListTables",
            b"{ }",
            1_440_938_160,
        );
        assert_ne!(s.authorization, other.authorization);
    }
}
