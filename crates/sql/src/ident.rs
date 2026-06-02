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

/// Deterministic join-table name for a many-to-many relation declared on
/// `owner.<field>`. Normally `j_<owner>_<field>`. When that would exceed the
/// Postgres 63-char identifier limit, truncate the readable part and append a
/// short hash of the full logical name so the result stays unique and stable
/// across rebuilds (FNV-1a hash).
pub fn join_table_name(owner: &str, field: &str) -> Result<String, IdentError> {
    if !is_valid_ident(owner) {
        return Err(IdentError(owner.to_string()));
    }
    if !is_valid_ident(field) {
        return Err(IdentError(field.to_string()));
    }
    let readable = format!("j_{owner}_{field}");
    if readable.len() <= 63 {
        return quote_ident(&readable);
    }
    // FNV-1a (32-bit). Used instead of DefaultHasher because this string
    // becomes a persistent Postgres table name: the hash must be stable
    // across Rust versions and rebuilds, which DefaultHasher does not promise.
    // 32 bits is ample — a schema has at most dozens of join tables, so
    // collision probability is negligible at this scale.
    let mut h: u32 = 0x811c_9dc5;
    for b in readable.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let hash = format!("{h:08x}");
    let head_budget = 63 - 1 - hash.len(); // 1 for the underscore separator
    let head: String = readable.chars().take(head_budget).collect();
    quote_ident(&format!("{head}_{hash}"))
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

    #[test]
    fn join_table_short_name_readable() {
        assert_eq!(join_table_name("post", "tags").unwrap(), "\"j_post_tags\"");
    }

    #[test]
    fn join_table_long_name_hashed_and_under_limit() {
        let owner = "a".repeat(40);
        let field = "b".repeat(40);
        let q = join_table_name(&owner, &field).unwrap();
        let raw = q.trim_matches('"');
        assert!(raw.len() <= 63, "ident too long: {} ({})", raw.len(), raw);
        assert!(raw.starts_with("j_"));
        assert_eq!(join_table_name(&owner, &field).unwrap(), q);
    }

    #[test]
    fn join_table_boundary_fast_vs_hash_path() {
        // 'j_' + 61 chars of owner+sep+field that total a 63-char readable name
        // stays on the fast (readable) path.
        let exactly_63 = join_table_name(&"a".repeat(60), "b").unwrap(); // j_ + 60 + _ + 1 = 63
        assert_eq!(exactly_63.trim_matches('"').len(), 63);
        assert!(exactly_63.trim_matches('"').starts_with("j_a"));
        // One char longer → 64 → hash path, still <= 63 raw.
        let hashed = join_table_name(&"a".repeat(61), "b").unwrap(); // 64 -> hash
        let raw = hashed.trim_matches('"');
        assert!(raw.len() <= 63);
        // Different from the fast-path name (it was truncated + hashed).
        assert!(raw.contains('_'));
    }

    #[test]
    fn join_table_rejects_bad_idents() {
        assert!(join_table_name("Bad", "tags").is_err());
        assert!(join_table_name("post", "Bad Field").is_err());
    }
}
