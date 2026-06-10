//! System columns always present on every per-type table.

use crate::field::FieldKind;

#[derive(Debug, Clone, Copy)]
pub struct SystemColumn {
    pub name: &'static str,
    pub kind: FieldKind,
}

pub const SYSTEM_COLUMNS: &[SystemColumn] = &[
    SystemColumn {
        name: "id",
        kind: FieldKind::Uuid,
    },
    SystemColumn {
        name: "created_at",
        kind: FieldKind::Datetime,
    },
    SystemColumn {
        name: "updated_at",
        kind: FieldKind::Datetime,
    },
];

pub fn is_system_column(name: &str) -> bool {
    SYSTEM_COLUMNS.iter().any(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_system_columns() {
        assert!(is_system_column("id"));
        assert!(is_system_column("created_at"));
        assert!(is_system_column("updated_at"));
        assert!(!is_system_column("title"));
    }
}
