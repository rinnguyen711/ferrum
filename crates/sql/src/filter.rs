//! Filter expressions. v1: only `None`. Phase 2 will add operator variants.

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert!(matches!(Filter::default(), Filter::None));
    }
}
