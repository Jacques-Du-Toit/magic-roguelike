//! Helpers shared by the duel tests.

use super::config::DuelConfig;
use super::ids::{CardId, PlayerId};
use super::state::DuelState;
use crate::data;
use crate::decks::DeckId;

/// A default duel between two of the decks in `data/decks/`.
pub(crate) fn config(p1_deck: &str, p2_deck: &str) -> DuelConfig {
    let data = data::embedded().unwrap();
    let deck = |id: &str| &data.decks[&DeckId(id.to_string())];
    DuelConfig::from_decks(deck(p1_deck), deck(p2_deck), &data.balance.duel)
}

/// How many lands are in the player's hand.
pub(crate) fn hand_lands(state: &DuelState, player: PlayerId) -> usize {
    state
        .player(player)
        .hand
        .iter()
        .filter(|id| state.def(**id).unwrap().is_land())
        .count()
}

/// The physical cards in the player's hand, in hand order.
pub(crate) fn hand_cards(state: &DuelState, player: PlayerId) -> Vec<CardId> {
    state
        .player(player)
        .hand
        .iter()
        .map(|id| state.objects[id].card)
        .collect()
}
