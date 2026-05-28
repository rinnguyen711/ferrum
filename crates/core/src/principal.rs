//! Identity and authorization actions.

#[derive(Debug, Clone)]
pub enum Principal {
    Admin,
    // future: User { id: Uuid, roles: Vec<String> }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SchemaRead,
    SchemaWrite,
    ContentRead,
    ContentWrite,
}

impl Principal {
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::Admin => "admin",
        }
    }
}
