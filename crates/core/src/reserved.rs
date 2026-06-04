//! Reserved names and identifier validation.

use std::sync::OnceLock;

pub const RESERVED_FIELD_NAMES: &[&str] = &[
    "id", "created_at", "updated_at", "published_at",
    "select", "from", "where", "table", "order", "group", "having",
    "user", "null", "true", "false", "default", "primary", "foreign", "index",
];

pub fn is_valid_ident(s: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^[a-z][a-z0-9_]{0,62}$").unwrap());
    re.is_match(s)
}

pub fn is_reserved(s: &str) -> bool {
    RESERVED_FIELD_NAMES.contains(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_regex_accepts_snake_case() {
        assert!(is_valid_ident("title"));
        assert!(is_valid_ident("blog_post"));
        assert!(is_valid_ident("a"));
        assert!(is_valid_ident("a_1"));
    }

    #[test]
    fn ident_regex_rejects_bad_inputs() {
        assert!(!is_valid_ident(""));
        assert!(!is_valid_ident("Title"));
        assert!(!is_valid_ident("1foo"));
        assert!(!is_valid_ident("with space"));
        assert!(!is_valid_ident("with-dash"));
        assert!(!is_valid_ident(&"a".repeat(64)));
    }

    #[test]
    fn reserved_detection() {
        assert!(is_reserved("id"));
        assert!(is_reserved("select"));
        assert!(!is_reserved("title"));
    }

    #[test]
    fn published_at_is_reserved() {
        assert!(is_reserved("published_at"));
    }
}
