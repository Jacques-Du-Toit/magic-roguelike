//! Player, object and card ids.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum PlayerId {
    P1,
    P2,
}

impl PlayerId {
    pub const ALL: [PlayerId; 2] = [PlayerId::P1, PlayerId::P2];

    pub fn opponent(self) -> PlayerId {
        match self {
            PlayerId::P1 => PlayerId::P2,
            PlayerId::P2 => PlayerId::P1,
        }
    }

    /// Position in per-player arrays such as `DuelState::players`.
    pub fn index(self) -> usize {
        match self {
            PlayerId::P1 => 0,
            PlayerId::P2 => 1,
        }
    }
}

/// Identifies a game object. Ids are assigned in creation order, and a card
/// that changes zones becomes a new object with a new id, so anything that
/// referred to the old object stops pointing at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct ObjectId(pub u32);

/// Identifies one physical card for the whole duel. It never changes, so the
/// UI can follow a card as it moves between zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct CardId(pub u32);
