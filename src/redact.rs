//! Redaction of secrets in diagnostics.
//!
//! The CLI must never echo a full connection string — normal errors, and
//! especially `--verbose`/debug output, must not leak credentials. This
//! module owns the one rule for turning a database URL into something safe
//! to print, and it is covered by its own tests so the guarantee does not
//! regress silently.

/// Redact the userinfo portion of a database URL so it is safe to log.
///
/// `postgres://user:secret@host:5432/db` → `postgres://***@host:5432/db`.
/// A URL with no userinfo is returned unchanged; an unparseable string is
/// replaced with a fully redacted marker rather than echoed back.
pub fn redact_db_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return "<redacted>".into();
    };
    let remainder = &url[scheme_end + 3..];
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    match authority.rfind('@') {
        Some(at) => format!("{}://***@{}", &url[..scheme_end], &remainder[at + 1..]),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_userinfo_password() {
        assert_eq!(
            redact_db_url("postgres://keycloak:supersecret@localhost:5432/keycloak"),
            "postgres://***@localhost:5432/keycloak"
        );
    }

    #[test]
    fn redacts_userinfo_without_password() {
        assert_eq!(
            redact_db_url("postgres://keycloak@localhost:5432/keycloak"),
            "postgres://***@localhost:5432/keycloak"
        );
    }

    #[test]
    fn keeps_urls_without_userinfo_unchanged() {
        assert_eq!(
            redact_db_url("postgres://localhost:5432/keycloak"),
            "postgres://localhost:5432/keycloak"
        );
    }

    #[test]
    fn handles_query_strings() {
        assert_eq!(
            redact_db_url("postgres://user:pw@db:5432/k?sslmode=require"),
            "postgres://***@db:5432/k?sslmode=require"
        );
    }

    #[test]
    fn unparseable_input_is_fully_redacted() {
        assert_eq!(redact_db_url("not a url at all"), "<redacted>");
        assert_eq!(redact_db_url(""), "<redacted>");
    }
}
