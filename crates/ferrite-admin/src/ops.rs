//! The operator list (`ops.json`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{Shared, read, write};

/// One `ops.json` entry, in vanilla's serialization order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpRecord {
    pub(crate) uuid: Uuid,
    pub(crate) name: String,
    pub(crate) level: u8,
    #[serde(rename = "bypassesPlayerLimit")]
    pub(crate) bypasses_player_limit: bool,
}

/// The operator list backing `ops.json`.
///
/// ```no_run
/// # use ferrite_admin::AdminLists;
/// # use uuid::Uuid;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let lists = AdminLists::load(".")?;
/// let uuid = Uuid::nil();
/// lists.ops.add_op(uuid, "TestUser", 4, false);
/// assert_eq!(lists.ops.level_of(uuid), Some(4));
/// # Ok(())
/// # }
/// ```
pub struct Ops {
    pub(crate) shared: Arc<Shared>,
}

impl Ops {
    /// Vanilla permission level of `uuid`, if that player is an operator.
    pub fn level_of(&self, uuid: Uuid) -> Option<u8> {
        read(&self.shared.state)
            .ops
            .iter()
            .find(|op| op.uuid == uuid)
            .map(|op| op.level)
    }

    /// Adds an operator, or updates the existing entry for `uuid`.
    pub fn add_op(&self, uuid: Uuid, name: &str, level: u8, bypasses_player_limit: bool) {
        let record = OpRecord {
            uuid,
            name: name.to_owned(),
            level,
            bypasses_player_limit,
        };
        let mut state = write(&self.shared.state);
        match state.ops.iter_mut().find(|op| op.uuid == uuid) {
            Some(existing) => *existing = record,
            None => state.ops.push(record),
        }
        state.dirty.ops = true;
    }

    /// Removes an operator, returning whether the entry existed.
    pub fn remove_op(&self, uuid: Uuid) -> bool {
        let mut state = write(&self.shared.state);
        let before = state.ops.len();
        state.ops.retain(|op| op.uuid != uuid);
        let removed = state.ops.len() != before;
        if removed {
            state.dirty.ops = true;
        }
        removed
    }
}
