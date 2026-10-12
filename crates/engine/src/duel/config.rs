//! What a duel is started from.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::balance::{DuelBalance, FatigueBalance};
use crate::cards::{CardDefId, EmblemDef};
use crate::decks::DeckDef;

/// Per-player from the start, because boons, relics, bosses and planeswalker
/// passives all need asymmetric setups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuelConfig {
    pub players: [PlayerConfig; 2],
    pub rules: RulesConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerConfig {
    /// Lands and spells together.
    pub library: Vec<CardInstance>,
    pub starting_life: i32,
    pub opening_hand_size: u8,
    /// Cards that begin the duel on the battlefield, e.g. a boon's land.
    #[serde(default)]
    pub starting_permanents: Vec<CardInstance>,
    /// Relics, boons, passives and boss emblems, in the command zone.
    #[serde(default)]
    pub emblems: Vec<EmblemDef>,
    /// A legendary creature that starts in the command zone and can be cast
    /// from there. It is not in the library.
    #[serde(default)]
    pub commander: Option<CardInstance>,
}

/// Duel-wide numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesConfig {
    pub max_hand_size: u8,
    /// How many candidate opening hands are dealt; 1 turns smoothing off.
    pub smoothing_candidates: u8,
    pub fatigue: FatigueBalance,
    /// Generic mana added to a commander's cost for each earlier cast from
    /// the command zone.
    #[serde(default)]
    pub commander_tax: u32,
}

/// One card in a deck: a definition plus its run-specific upgrades.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardInstance {
    pub card_id: CardDefId,
    pub upgrades: Vec<CardUpgrade>,
}

/// No upgrades exist yet (M4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CardUpgrade {}

impl CardInstance {
    pub fn new(card_id: CardDefId) -> CardInstance {
        CardInstance {
            card_id,
            upgrades: Vec::new(),
        }
    }
}

impl PlayerConfig {
    /// A player with `deck` as their library and the default numbers.
    pub fn from_deck(deck: &DeckDef, balance: &DuelBalance) -> PlayerConfig {
        let library = deck
            .cards
            .iter()
            .flat_map(|entry| (0..entry.count).map(|_| CardInstance::new(entry.card.clone())))
            .collect();
        PlayerConfig {
            library,
            starting_life: balance.starting_life,
            opening_hand_size: balance.opening_hand_size,
            starting_permanents: Vec::new(),
            emblems: Vec::new(),
            commander: None,
        }
    }
}

impl RulesConfig {
    pub fn from_balance(balance: &DuelBalance) -> RulesConfig {
        RulesConfig {
            max_hand_size: balance.max_hand_size,
            smoothing_candidates: balance.smoothing_candidates,
            fatigue: balance.fatigue.clone(),
            commander_tax: balance.commander_tax,
        }
    }
}

impl DuelConfig {
    /// A symmetric duel between two decks with the default numbers.
    pub fn from_decks(p1: &DeckDef, p2: &DeckDef, balance: &DuelBalance) -> DuelConfig {
        DuelConfig {
            players: [
                PlayerConfig::from_deck(p1, balance),
                PlayerConfig::from_deck(p2, balance),
            ],
            rules: RulesConfig::from_balance(balance),
        }
    }
}
