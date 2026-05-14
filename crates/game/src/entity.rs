use core::types::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id:       EntityId,
    pub name:     String,
    pub class_id: Option<u32>,
    pub is_local: bool,
}

impl Entity {
    pub fn new(id: EntityId) -> Self {
        Self {
            id,
            name: format!("{id}"),
            class_id: None,
            is_local: false,
        }
    }
}
