//! Identifier quoting. The only place per-type table names and column names
//! are turned into SQL fragments. All other modules MUST go through here.

use rustapi_core::reserved::is_valid_ident;

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("invalid SQL identifier: {0}")]
pub struct IdentError(pub String);

pub fn quote_ident(s: &str) -> Result<String, IdentError> {
    if !is_valid_ident(s) {
        return Err(IdentError(s.to_string()));
    }
    // Validated against ^[a-z][a-z0-9_]{0,62}$ so no escapes needed,
    // but we still double-quote so reserved words can be used safely.
    Ok(format!("\"{s}\""))
}

pub fn table_name(content_type: &str) -> Result<String, IdentError> {
    if !is_valid_ident(content_type) {
        return Err(IdentError(content_type.to_string()));
    }
    quote_ident(&format!("ct_{content_type}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_basic() {
        assert_eq!(quote_ident("title").unwrap(), "\"title\"");
    }

    #[test]
    fn quote_rejects_bad() {
        assert!(quote_ident("bad name").is_err());
        assert!(quote_ident("Bad").is_err());
    }

    #[test]
    fn table_name_prefixes() {
        assert_eq!(table_name("post").unwrap(), "\"ct_post\"");
    }

    #[test]
    fn table_name_rejects_bad_input() {
        assert!(table_name("Bad").is_err());
    }
}
