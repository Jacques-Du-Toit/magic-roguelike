//! Player actions and the events they produce.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::{CardId, ObjectId, PlayerId};
use crate::cards::CardDefId;

/// Something a player does. Every action returned by `legal_actions` is
/// concrete and can be passed straight to `apply_action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Keep the current opening hand.
    Keep,
    /// Shuffle the hand away and draw a new one.
    Mulligan,
    /// Put a card from hand on the bottom of the library, once per mulligan
    /// taken, after keeping.
    BottomCard { card: ObjectId },
}

/// Something that happened, in order. The UI animates these rather than
/// diffing state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameEvent {
    /// The player shuffled their hand into their library. `mulligans_taken`
    /// includes this one.
    MulliganTaken {
        player: PlayerId,
        mulligans_taken: u8,
    },
    HandKept {
        player: PlayerId,
    },
    CardDrawn {
        player: PlayerId,
        card: EventCard,
    },
    CardPutOnBottom {
        player: PlayerId,
        card: EventCard,
    },
}

/// A card as it appears in an event. `face` is `None` once the event has
/// been redacted for a viewer who can't see the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EventCard {
    pub object: ObjectId,
    pub face: Option<CardFace>,
}

/// Which card an object is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardFace {
    pub card: CardId,
    pub def: CardDefId,
}
