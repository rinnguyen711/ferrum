//! Identity and authorization actions.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Principal {
    /// An authenticated user, built from verified JWT claims.
    User {
        id: Uuid,
        email: String,
        roles: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SchemaRead,
    SchemaWrite,
    ContentRead,
    ContentWrite,
    UserRead,
    UserWrite,
}

impl Principal {
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::User { .. } => "user",
        }
    }
}

/// Hardcoded role → permission map. Unknown roles grant nothing.
/// `admin` = full access; `editor` = content read/write; `viewer` = content read.
pub fn role_allows(role: &str, action: Action) -> bool {
    use Action::*;
    match role {
        "admin" => true,
        "editor" => matches!(action, ContentRead | ContentWrite),
        "viewer" => matches!(action, ContentRead),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_everything() {
        for a in [Action::SchemaRead, Action::SchemaWrite, Action::ContentRead, Action::ContentWrite] {
            assert!(role_allows("admin", a), "admin should allow {a:?}");
        }
    }

    #[test]
    fn admin_allows_user_actions() {
        assert!(role_allows("admin", Action::UserRead));
        assert!(role_allows("admin", Action::UserWrite));
    }

    #[test]
    fn non_admin_denied_user_actions() {
        for role in ["editor", "viewer", "ghost"] {
            assert!(!role_allows(role, Action::UserRead), "{role} UserRead");
            assert!(!role_allows(role, Action::UserWrite), "{role} UserWrite");
        }
    }

    #[test]
    fn editor_content_only() {
        assert!(role_allows("editor", Action::ContentRead));
        assert!(role_allows("editor", Action::ContentWrite));
        assert!(!role_allows("editor", Action::SchemaRead));
        assert!(!role_allows("editor", Action::SchemaWrite));
    }

    #[test]
    fn viewer_read_only() {
        assert!(role_allows("viewer", Action::ContentRead));
        assert!(!role_allows("viewer", Action::ContentWrite));
        assert!(!role_allows("viewer", Action::SchemaRead));
    }

    #[test]
    fn unknown_role_allows_nothing() {
        for a in [Action::SchemaRead, Action::SchemaWrite, Action::ContentRead, Action::ContentWrite] {
            assert!(!role_allows("ghost", a));
        }
    }

    #[test]
    fn user_principal_carries_roles() {
        let p = Principal::User {
            id: uuid::Uuid::nil(),
            email: "a@b.c".into(),
            roles: vec!["admin".into()],
        };
        assert_eq!(p.kind(), "user");
    }
}
