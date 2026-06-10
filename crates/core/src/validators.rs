//! Format validators for new field kinds (phase 2.5).
//! Email and slug use compiled regex (LazyLock); url uses the `url` crate.

use regex::Regex;
use std::sync::LazyLock;

static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap());
static SLUG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap());

pub const SLUG_MAX_LEN: usize = 200;

pub fn is_valid_email(s: &str) -> bool {
    EMAIL_RE.is_match(s)
}

pub fn is_valid_slug(s: &str) -> bool {
    s.len() <= SLUG_MAX_LEN && SLUG_RE.is_match(s)
}

pub fn is_valid_http_url(s: &str) -> bool {
    match url::Url::parse(s) {
        Ok(u) => matches!(u.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_ok() {
        assert!(is_valid_email("a@b.co"));
        assert!(is_valid_email("user.name+tag@example.com"));
    }

    #[test]
    fn email_bad() {
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("no-at-sign"));
        assert!(!is_valid_email("missing@tld"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("white space@b.co"));
    }

    #[test]
    fn slug_ok() {
        assert!(is_valid_slug("hello"));
        assert!(is_valid_slug("hello-world"));
        assert!(is_valid_slug("a1-b2-c3"));
        assert!(is_valid_slug("1"));
    }

    #[test]
    fn slug_bad() {
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Hello"));
        assert!(!is_valid_slug("-leading"));
        assert!(!is_valid_slug("trailing-"));
        assert!(!is_valid_slug("two--dashes"));
        assert!(!is_valid_slug("under_score"));
    }

    #[test]
    fn slug_too_long() {
        let s: String = "a".repeat(SLUG_MAX_LEN + 1);
        assert!(!is_valid_slug(&s));
    }

    #[test]
    fn url_http_ok() {
        assert!(is_valid_http_url("http://example.com"));
        assert!(is_valid_http_url("https://example.com/path?q=1"));
    }

    #[test]
    fn url_non_http_rejected() {
        assert!(!is_valid_http_url("ftp://example.com"));
        assert!(!is_valid_http_url("mailto:a@b.co"));
        assert!(!is_valid_http_url("javascript:alert(1)"));
    }

    #[test]
    fn url_bad_parse() {
        assert!(!is_valid_http_url(""));
        assert!(!is_valid_http_url("not a url"));
    }
}
