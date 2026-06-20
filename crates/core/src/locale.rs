//! Locale code validation. A locale tag is a lowercase language subtag,
//! optionally followed by `-` and a region subtag, e.g. `en`, `pt-br`.

/// True if `s` is a syntactically valid locale tag for this CMS:
/// `^[a-z]{2,3}(-[a-z0-9]{2,8})?$`. Deliberately stricter and lowercase-only
/// so a code maps 1:1 to a row value with no case ambiguity.
pub fn is_valid_locale_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let lang = match parts.next() {
        Some(l) => l,
        None => return false,
    };
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(region) => {
            parts.next().is_none()
                && (2..=8).contains(&region.len())
                && region
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        }
    }
}

/// The two physical columns added to a localized content type's table, in the
/// order they should be emitted/read. Not part of `is_system_column` because
/// they exist only on localized types.
pub const LOCALIZATION_COLUMNS: [&str; 2] = ["document_id", "locale"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lang_only() {
        assert!(is_valid_locale_tag("en"));
        assert!(is_valid_locale_tag("fra"));
    }

    #[test]
    fn accepts_lang_region() {
        assert!(is_valid_locale_tag("pt-br"));
        assert!(is_valid_locale_tag("en-001"));
    }

    #[test]
    fn rejects_uppercase_empty_and_garbage() {
        assert!(!is_valid_locale_tag("EN"));
        assert!(!is_valid_locale_tag("en-BR"));
        assert!(!is_valid_locale_tag(""));
        assert!(!is_valid_locale_tag("e"));
        assert!(!is_valid_locale_tag("en-br-x"));
        assert!(!is_valid_locale_tag("en_us"));
    }
}
