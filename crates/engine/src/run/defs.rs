//! Run content: planeswalkers, enemies and the modifiers (relics, boons,
//! passives, boss emblems) that change how a duel is set up. The data
//! formats of `data/run/*.json` and of the boons in `data/balance.json`.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::cards::{
    AbilityDef, CardDef, CardDefId, Color, EmblemDef, EmblemError, validate_emblem,
};
use crate::data::CardDatabase;
use crate::decks::DeckEntry;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(
    /// Identifies a modifier: a relic, a boon, a passive or an enemy's emblem.
    ModifierId
);
id_type!(PlaneswalkerId);
id_type!(EnemyId);

/// Something that changes how a duel is set up for its owner. Relics, boons,
/// planeswalker passives and boss emblems are all modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct ModifierDef {
    pub id: ModifierId,
    pub name: String,
    /// What it does, in words, for the player.
    pub text: String,
    pub effects: Vec<ModifierEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModifierEffect {
    /// Changes a player's starting life.
    StartingLife { who: Who, change: i32 },
    /// Changes the owner's opening hand size.
    OpeningHand { change: i8 },
    /// The owner starts with a basic land of their deck's main colour on
    /// the battlefield.
    StartingBasicLand {},
    /// The owner gets an emblem with these abilities, named after the
    /// modifier.
    Abilities { abilities: Vec<AbilityDef> },
    /// Extra gold for each duel won. It acts on the run, not the duel.
    GoldPerWin { amount: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    Owner,
    Opponent,
}

impl ModifierDef {
    /// The emblem this modifier puts in its owner's command zone, if any.
    pub fn emblem(&self) -> Option<EmblemDef> {
        let abilities: Vec<AbilityDef> = self
            .effects
            .iter()
            .filter_map(|effect| match effect {
                ModifierEffect::Abilities { abilities } => Some(abilities.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        (!abilities.is_empty()).then(|| EmblemDef {
            name: self.name.clone(),
            text: self.text.clone(),
            abilities,
        })
    }
}

/// A character to start a run with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct PlaneswalkerDef {
    pub id: PlaneswalkerId,
    pub name: String,
    pub description: String,
    pub colors: Vec<Color>,
    /// The starting deck without its basic lands, which the run adds.
    pub spells: Vec<DeckEntry>,
    /// The legendary creature this character casts from the command zone,
    /// in the character's colours. It is not part of the deck.
    #[serde(default)]
    pub commander: Option<CardDefId>,
    /// The starting emblem (one of `data/run/emblems.json`) this character
    /// takes when the player doesn't choose another.
    pub emblem: ModifierId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EnemyTier {
    Easy,
    Normal,
    Elite,
    Boss,
}

/// An opponent: a hand-built deck, lands included, and any emblems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct EnemyDef {
    pub id: EnemyId,
    pub name: String,
    pub tier: EnemyTier,
    pub cards: Vec<DeckEntry>,
    #[serde(default)]
    pub modifiers: Vec<ModifierDef>,
    /// The legendary creature this enemy casts from the command zone, in
    /// its deck's colours.
    #[serde(default)]
    pub commander: Option<CardDefId>,
}

/// The run content loaded from `data/run/`. Each list is in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunData {
    pub planeswalkers: Vec<PlaneswalkerDef>,
    /// The starting emblems: the player takes one into the run.
    pub emblems: Vec<ModifierDef>,
    pub relics: Vec<ModifierDef>,
    pub enemies: Vec<EnemyDef>,
}

impl RunData {
    pub fn planeswalker(&self, id: &PlaneswalkerId) -> Option<&PlaneswalkerDef> {
        self.planeswalkers.iter().find(|def| def.id == *id)
    }

    pub fn emblem(&self, id: &ModifierId) -> Option<&ModifierDef> {
        self.emblems.iter().find(|def| def.id == *id)
    }

    pub fn relic(&self, id: &ModifierId) -> Option<&ModifierDef> {
        self.relics.iter().find(|def| def.id == *id)
    }

    pub fn enemy(&self, id: &EnemyId) -> Option<&EnemyDef> {
        self.enemies.iter().find(|def| def.id == *id)
    }

    /// The enemies of one tier, in file order.
    pub fn enemies_of(&self, tier: EnemyTier) -> impl Iterator<Item = &EnemyDef> {
        self.enemies.iter().filter(move |enemy| enemy.tier == tier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunDataError {
    #[error("id `{0}` must be lower snake_case")]
    BadId(String),
    #[error("id `{0}` is used more than once")]
    DuplicateId(String),
    #[error("`{0}`: name or text is empty")]
    EmptyText(String),
    #[error("`{0}` has no effects")]
    NoEffects(String),
    #[error("`{0}`: a change of zero does nothing")]
    ZeroChange(String),
    #[error("`{id}`: {source}")]
    Emblem { id: String, source: EmblemError },
    #[error("`{0}`: only a relic can give gold")]
    GoldOnNonRelic(String),
    #[error("`{0}` has no colours")]
    NoColors(String),
    #[error("`{0}` has no cards")]
    NoCards(String),
    #[error("`{id}`: unknown card `{card}`")]
    UnknownCard { id: String, card: CardDefId },
    #[error("`{id}`: `{card}` is listed more than once or with a count of zero")]
    BadEntry { id: String, card: CardDefId },
    #[error("`{id}`: `{card}` is a land; the run adds a starting deck's lands itself")]
    LandInSpells { id: String, card: CardDefId },
    #[error("`{id}`: the commander `{card}` is not a legendary creature card")]
    BadCommander { id: String, card: CardDefId },
    #[error("`{id}`: the commander `{card}` is not in the deck's colours")]
    CommanderColors { id: String, card: CardDefId },
    #[error("`{id}`: there is no starting emblem `{emblem}`")]
    UnknownEmblem { id: String, emblem: ModifierId },
    #[error("there must be at least one starting emblem")]
    NoEmblems,
    #[error("there must be at least one planeswalker")]
    NoPlaneswalkers,
    #[error("there must be at least one enemy of tier {0:?}")]
    NoEnemies(EnemyTier),
}

fn check_id(id: &str) -> Result<(), RunDataError> {
    let ok = !id.is_empty()
        && !id.starts_with('_')
        && !id.ends_with('_')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if ok {
        Ok(())
    } else {
        Err(RunDataError::BadId(id.to_string()))
    }
}

/// What kind of thing a modifier is, which decides what it may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKind {
    Relic,
    Boon,
    Passive,
    Enemy,
}

pub fn validate_modifier(modifier: &ModifierDef, kind: ModifierKind) -> Result<(), RunDataError> {
    let id = modifier.id.0.clone();
    check_id(&id)?;
    if modifier.name.trim().is_empty() || modifier.text.trim().is_empty() {
        return Err(RunDataError::EmptyText(id));
    }
    if modifier.effects.is_empty() {
        return Err(RunDataError::NoEffects(id));
    }
    for effect in &modifier.effects {
        match effect {
            ModifierEffect::StartingLife { change: 0, .. }
            | ModifierEffect::OpeningHand { change: 0 }
            | ModifierEffect::GoldPerWin { amount: 0 } => {
                return Err(RunDataError::ZeroChange(id));
            }
            ModifierEffect::GoldPerWin { .. } if kind != ModifierKind::Relic => {
                return Err(RunDataError::GoldOnNonRelic(id));
            }
            _ => {}
        }
    }
    if let Some(emblem) = modifier.emblem() {
        validate_emblem(&emblem).map_err(|source| RunDataError::Emblem { id, source })?;
    }
    Ok(())
}

/// Checks a list of deck entries: known cards, each once, with a count.
fn validate_entries(
    id: &str,
    entries: &[DeckEntry],
    cards: &CardDatabase,
) -> Result<(), RunDataError> {
    if entries.is_empty() {
        return Err(RunDataError::NoCards(id.to_string()));
    }
    let mut seen = BTreeSet::new();
    for entry in entries {
        // A token is not a card a deck can hold.
        if cards.get(&entry.card).is_none_or(|card| card.token) {
            return Err(RunDataError::UnknownCard {
                id: id.to_string(),
                card: entry.card.clone(),
            });
        }
        if entry.count == 0 || !seen.insert(&entry.card) {
            return Err(RunDataError::BadEntry {
                id: id.to_string(),
                card: entry.card.clone(),
            });
        }
    }
    Ok(())
}

/// Checks a commander: a legendary creature card whose colours are all
/// among `colors`, the colours of the deck it leads.
fn validate_commander(
    id: &str,
    commander: &CardDefId,
    colors: &[Color],
    cards: &CardDatabase,
) -> Result<(), RunDataError> {
    let def = cards
        .get(commander)
        .filter(|def| def.is_legendary() && def.is_creature() && !def.token)
        .ok_or_else(|| RunDataError::BadCommander {
            id: id.to_string(),
            card: commander.clone(),
        })?;
    if def.colors.iter().all(|color| colors.contains(color)) {
        Ok(())
    } else {
        Err(RunDataError::CommanderColors {
            id: id.to_string(),
            card: commander.clone(),
        })
    }
}

/// A deck entry can't be a legendary card: those are commanders.
fn no_legends(id: &str, entries: &[DeckEntry], cards: &CardDatabase) -> Result<(), RunDataError> {
    match entries
        .iter()
        .find(|entry| cards.get(&entry.card).is_some_and(CardDef::is_legendary))
    {
        Some(entry) => Err(RunDataError::BadEntry {
            id: id.to_string(),
            card: entry.card.clone(),
        }),
        None => Ok(()),
    }
}

fn unique<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), RunDataError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(RunDataError::DuplicateId(id.to_string()));
        }
    }
    Ok(())
}

/// Checks the run content against the card database.
pub fn validate(run: &RunData, cards: &CardDatabase) -> Result<(), RunDataError> {
    if run.planeswalkers.is_empty() {
        return Err(RunDataError::NoPlaneswalkers);
    }
    if run.emblems.is_empty() {
        return Err(RunDataError::NoEmblems);
    }
    unique(run.emblems.iter().map(|def| def.id.as_str()))?;
    for emblem in &run.emblems {
        validate_modifier(emblem, ModifierKind::Passive)?;
    }

    unique(run.planeswalkers.iter().map(|def| def.id.as_str()))?;
    for planeswalker in &run.planeswalkers {
        let id = planeswalker.id.as_str();
        check_id(id)?;
        if planeswalker.name.trim().is_empty() || planeswalker.description.trim().is_empty() {
            return Err(RunDataError::EmptyText(id.to_string()));
        }
        if planeswalker.colors.is_empty() {
            return Err(RunDataError::NoColors(id.to_string()));
        }
        validate_entries(id, &planeswalker.spells, cards)?;
        for entry in &planeswalker.spells {
            if cards.get(&entry.card).is_some_and(|card| card.is_land()) {
                return Err(RunDataError::LandInSpells {
                    id: id.to_string(),
                    card: entry.card.clone(),
                });
            }
        }
        no_legends(id, &planeswalker.spells, cards)?;
        if run.emblem(&planeswalker.emblem).is_none() {
            return Err(RunDataError::UnknownEmblem {
                id: id.to_string(),
                emblem: planeswalker.emblem.clone(),
            });
        }
        if let Some(commander) = &planeswalker.commander {
            validate_commander(id, commander, &planeswalker.colors, cards)?;
        }
    }

    unique(run.relics.iter().map(|def| def.id.as_str()))?;
    for relic in &run.relics {
        validate_modifier(relic, ModifierKind::Relic)?;
    }

    unique(run.enemies.iter().map(|def| def.id.as_str()))?;
    for enemy in &run.enemies {
        let id = enemy.id.as_str();
        check_id(id)?;
        if enemy.name.trim().is_empty() {
            return Err(RunDataError::EmptyText(id.to_string()));
        }
        validate_entries(id, &enemy.cards, cards)?;
        no_legends(id, &enemy.cards, cards)?;
        if let Some(commander) = &enemy.commander {
            // An enemy's colours are those of the cards in its deck.
            let colors: Vec<Color> = enemy
                .cards
                .iter()
                .filter_map(|entry| cards.get(&entry.card))
                .flat_map(|card| card.colors.iter().copied())
                .collect();
            validate_commander(id, commander, &colors, cards)?;
        }
        for modifier in &enemy.modifiers {
            validate_modifier(modifier, ModifierKind::Enemy)?;
        }
    }
    for tier in [
        EnemyTier::Easy,
        EnemyTier::Normal,
        EnemyTier::Elite,
        EnemyTier::Boss,
    ] {
        if run.enemies_of(tier).next().is_none() {
            return Err(RunDataError::NoEnemies(tier));
        }
    }
    Ok(())
}
