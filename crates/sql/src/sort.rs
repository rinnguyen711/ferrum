//! Sort directives applied to list queries.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    pub fn as_sql(&self) -> &'static str {
        match self {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "asc" => Some(SortDir::Asc),
            "desc" => Some(SortDir::Desc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sort {
    pub column: String,
    pub dir: SortDir,
}

impl Sort {
    pub fn default_created_at() -> Self {
        Self {
            column: "created_at".into(),
            dir: SortDir::Desc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dir() {
        assert_eq!(SortDir::parse("asc"), Some(SortDir::Asc));
        assert_eq!(SortDir::parse("DESC"), Some(SortDir::Desc));
        assert_eq!(SortDir::parse("foo"), None);
    }
}
