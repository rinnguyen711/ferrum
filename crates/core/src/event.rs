//! Domain events emitted by handlers.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Event {
    SchemaCreated { name: String },
    SchemaUpdated { name: String },
    SchemaDeleted { name: String },
    EntryCreated { content_type: String, id: Uuid },
    EntryUpdated { content_type: String, id: Uuid },
    EntryDeleted { content_type: String, id: Uuid },
    EntryPublished { content_type: String, id: Uuid },
    EntryUnpublished { content_type: String, id: Uuid },
}
