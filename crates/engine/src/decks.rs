//! Decklists: the data format of `data/decks/*.json`.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::cards::CardDefId;
use crate::data::CardDatabase;

/// Identifies a decklist, e.g. `deck_a`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct DeckId(pub String);

impl fmt::Display for DeckId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A decklist. Lands and spells share one list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DeckDef {
    pub id: DeckId,
    pub name: String,
    pub cards: Vec<DeckEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeckEntry {
    pub card: CardDefId,
    pub count: u32,
}

impl DeckDef {
    /// Total number of cards in the deck.
    pub fn size(&self) -> u32 {
        self.cards.iter().map(|entry| entry.count).sum()
    }

    /// How many copies of `card` the deck plays.
    pub fn count_of(&self, card: &str) -> u32 {
        self.cards
            .iter()
            .filter(|entry| entry.card.as_str() == card)
            .map(|entry| entry.count)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeckError {
    #[error("name is empty")]
    EmptyName,
    #[error("deck has no cards")]
    Empty,
    #[error("unknown card `{0}`")]
    UnknownCard(CardDefId),
    #[error("card `{0}` has a count of zero")]
    ZeroCount(CardDefId),
    #[error("card `{0}` is listed more than once")]
    DuplicateEntry(CardDefId),
}

/// Checks a decklist against the card database.
pub fn validate(deck: &DeckDef, cards: &CardDatabase) -> Result<(), DeckError> {
    if deck.name.trim().is_empty() {
        return Err(DeckError::EmptyName);
    }
    if deck.cards.is_empty() {
        return Err(DeckError::Empty);
    }
    let mut seen = BTreeSet::new();
    for entry in &deck.cards {
        // A token is not a card a deck can hold.
        if cards.get(&entry.card).is_none_or(|card| card.token) {
            return Err(DeckError::UnknownCard(entry.card.clone()));
        }
        if entry.count == 0 {
            return Err(DeckError::ZeroCount(entry.card.clone()));
        }
        if !seen.insert(&entry.card) {
            return Err(DeckError::DuplicateEntry(entry.card.clone()));
        }
    }
    Ok(())
}
