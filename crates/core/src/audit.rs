//! Audit-log domain types: actor identity, request context, and the rich
//! entry a handler hands to the audit sink.

use crate::Principal;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorKind {
    User,
    ApiToken,
    System,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::ApiToken => "api_token",
            ActorKind::System => "system",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Actor {
    pub kind: ActorKind,
    pub id: Option<Uuid>,
    pub label: String,
}

impl Actor {
    /// Build an actor from an authenticated principal. The label is a
    /// denormalized snapshot stored on the row so it survives deletion.
    /// `token_label` supplies a name for API tokens (the principal only
    /// carries the id); pass `None` to fall back to the token id.
    pub fn from_principal(p: &Principal, token_label: Option<&str>) -> Self {
        match p {
            Principal::User { id, email, .. } => Actor {
                kind: ActorKind::User,
                id: Some(*id),
                label: email.clone(),
            },
            Principal::ApiToken { id, .. } => Actor {
                kind: ActorKind::ApiToken,
                id: Some(*id),
                label: token_label
                    .map(str::to_string)
                    .unwrap_or_else(|| id.to_string()),
            },
        }
    }

    /// A non-authenticated actor (e.g. a failed login with an unknown user).
    pub fn system(label: impl Into<String>) -> Self {
        Actor {
            kind: ActorKind::System,
            id: None,
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FieldChange {
    pub field: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub action: String,
    pub category: String,
    pub status: String,
    pub actor: Actor,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub changes: Vec<FieldChange>,
    pub note: Option<String>,
    pub ctx: RequestContext,
}

impl AuditEntry {
    /// Start a successful entry. `action` is a dotted key (e.g. `entry.publish`);
    /// the category is derived from its prefix via [`category_for`].
    pub fn new(action: impl Into<String>, actor: Actor) -> Self {
        let action = action.into();
        let category = category_for(&action).to_string();
        AuditEntry {
            action,
            category,
            status: "success".into(),
            actor,
            target_type: None,
            target_id: None,
            target_label: None,
            changes: Vec::new(),
            note: None,
            ctx: RequestContext::default(),
        }
    }

    pub fn target(
        mut self,
        ty: impl Into<String>,
        id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        self.target_type = Some(ty.into());
        self.target_id = Some(id.into());
        self.target_label = Some(label.into());
        self
    }

    pub fn failed(mut self) -> Self {
        self.status = "failed".into();
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn changes(mut self, changes: Vec<FieldChange>) -> Self {
        self.changes = changes;
        self
    }

    pub fn ctx(mut self, ctx: RequestContext) -> Self {
        self.ctx = ctx;
        self
    }
}

/// Maps a dotted action key to its audit category. Unknown prefixes → "settings".
pub fn category_for(action: &str) -> &'static str {
    match action.split('.').next().unwrap_or("") {
        "entry" | "schema" => "content",
        "auth" => "auth",
        "role" | "user" => "perm",
        _ => "settings", // token.*, webhook.*, settings.*
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_derives_from_action_prefix() {
        assert_eq!(category_for("entry.publish"), "content");
        assert_eq!(category_for("schema.create"), "content");
        assert_eq!(category_for("auth.login"), "auth");
        assert_eq!(category_for("auth.login_failed"), "auth");
        assert_eq!(category_for("role.change"), "perm");
        assert_eq!(category_for("user.suspend"), "perm");
        assert_eq!(category_for("token.create"), "settings");
        assert_eq!(category_for("webhook.create"), "settings");
        assert_eq!(category_for("settings.update"), "settings");
        assert_eq!(category_for("nonsense"), "settings");
    }

    #[test]
    fn user_principal_becomes_user_actor() {
        let p = Principal::User {
            id: Uuid::nil(),
            email: "a@b.test".into(),
            roles: vec!["admin".into()],
        };
        let a = Actor::from_principal(&p, None);
        assert_eq!(a.kind, ActorKind::User);
        assert_eq!(a.id, Some(Uuid::nil()));
        assert_eq!(a.label, "a@b.test");
    }

    #[test]
    fn token_principal_uses_supplied_label() {
        let p = Principal::ApiToken {
            id: Uuid::nil(),
            scopes: vec![],
        };
        let a = Actor::from_principal(&p, Some("Website ISR"));
        assert_eq!(a.kind, ActorKind::ApiToken);
        assert_eq!(a.label, "Website ISR");
    }

    #[test]
    fn entry_builder_sets_category_and_target() {
        let e = AuditEntry::new("entry.publish", Actor::system("x"))
            .target("article", "id-1", "My Title");
        assert_eq!(e.category, "content");
        assert_eq!(e.status, "success");
        assert_eq!(e.target_type.as_deref(), Some("article"));
        assert_eq!(e.target_label.as_deref(), Some("My Title"));
    }
}
