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
    /// An API token, built from a DB lookup. Carries explicit action scopes.
    ApiToken { id: Uuid, scopes: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SchemaRead,
    SchemaWrite,
    SchemaDelete,
    ContentRead,
    ContentWrite,
    ContentDelete,
    UserRead,
    UserWrite,
    UserDelete,
}

impl Principal {
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::User { .. } => "user",
            Principal::ApiToken { .. } => "api_token",
        }
    }
}

/// Maps an `Action` to its wire scope string.
pub fn action_to_scope(action: Action) -> &'static str {
    match action {
        Action::ContentRead => "content:read",
        Action::ContentWrite => "content:write",
        Action::ContentDelete => "content:delete",
        Action::SchemaRead => "schema:read",
        Action::SchemaWrite => "schema:write",
        Action::SchemaDelete => "schema:delete",
        Action::UserRead => "user:read",
        Action::UserWrite => "user:write",
        Action::UserDelete => "user:delete",
    }
}

/// Hardcoded role → permission map. Unknown roles grant nothing.
/// `admin` = full access; `editor` = content read/write; `viewer` = content read.
pub fn role_allows(role: &str, action: Action) -> bool {
    use Action::*;
    match role {
        "admin" => true,
        "editor" => matches!(action, ContentRead | ContentWrite | ContentDelete),
        "viewer" => matches!(action, ContentRead),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_everything() {
        for a in [
            Action::SchemaRead,
            Action::SchemaWrite,
            Action::SchemaDelete,
            Action::ContentRead,
            Action::ContentWrite,
            Action::ContentDelete,
            Action::UserRead,
            Action::UserWrite,
            Action::UserDelete,
        ] {
            assert!(role_allows("admin", a), "admin should allow {a:?}");
        }
    }

    #[test]
    fn non_admin_denied_user_actions() {
        for role in ["editor", "viewer", "ghost"] {
            assert!(!role_allows(role, Action::UserRead), "{role} UserRead");
            assert!(!role_allows(role, Action::UserWrite), "{role} UserWrite");
            assert!(!role_allows(role, Action::UserDelete), "{role} UserDelete");
        }
    }

    #[test]
    fn editor_content_only() {
        assert!(role_allows("editor", Action::ContentRead));
        assert!(role_allows("editor", Action::ContentWrite));
        assert!(role_allows("editor", Action::ContentDelete));
        assert!(!role_allows("editor", Action::SchemaRead));
        assert!(!role_allows("editor", Action::SchemaWrite));
        assert!(!role_allows("editor", Action::SchemaDelete));
    }

    #[test]
    fn viewer_read_only() {
        assert!(role_allows("viewer", Action::ContentRead));
        assert!(!role_allows("viewer", Action::ContentWrite));
        assert!(!role_allows("viewer", Action::ContentDelete));
        assert!(!role_allows("viewer", Action::SchemaRead));
    }

    #[test]
    fn unknown_role_allows_nothing() {
        for a in [
            Action::SchemaRead,
            Action::SchemaWrite,
            Action::SchemaDelete,
            Action::ContentRead,
            Action::ContentWrite,
            Action::ContentDelete,
        ] {
            assert!(!role_allows("ghost", a));
        }
    }

    #[test]
    fn action_to_scope_round_trips() {
        assert_eq!(action_to_scope(Action::ContentRead), "content:read");
        assert_eq!(action_to_scope(Action::ContentWrite), "content:write");
        assert_eq!(action_to_scope(Action::ContentDelete), "content:delete");
        assert_eq!(action_to_scope(Action::SchemaRead), "schema:read");
        assert_eq!(action_to_scope(Action::SchemaWrite), "schema:write");
        assert_eq!(action_to_scope(Action::SchemaDelete), "schema:delete");
        assert_eq!(action_to_scope(Action::UserRead), "user:read");
        assert_eq!(action_to_scope(Action::UserWrite), "user:write");
        assert_eq!(action_to_scope(Action::UserDelete), "user:delete");
    }

    #[test]
    fn api_token_kind() {
        let p = Principal::ApiToken {
            id: Uuid::nil(),
            scopes: vec![],
        };
        assert_eq!(p.kind(), "api_token");
    }
}
