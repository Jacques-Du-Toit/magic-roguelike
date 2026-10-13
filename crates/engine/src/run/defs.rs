//! Run content: planeswalkers, enemies and the modifiers (relics, wounds,
//! passives, boss emblems) that change how a duel is set up. The data
//! formats of `data/run/*.json`.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::cards::{
    AbilityDef, CardDef, CardDefId, Color, Effect, EmblemDef, EmblemError, validate_emblem,
};
use crate::data::CardDatabase;
use crate::decks::DeckEntry;

use super::feats::FeatDef;

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
    /// Identifies a modifier: a relic, a wound, a passive or an enemy's emblem.
    ModifierId
);
id_type!(PlaneswalkerId);
id_type!(EnemyId);
id_type!(EventId);
id_type!(FeatId);

/// Something that changes how a duel is set up for its owner. Relics, wounds,
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
    /// The owner starts with `count` of this token on the battlefield.
    StartingToken {
        token: CardDefId,
        #[serde(default = "one")]
        count: u32,
    },
    /// Extra gold for each duel won. It acts on the run, not the duel.
    GoldPerWin { amount: u32 },
    /// Everything a shop sells or does costs this many percent less. It
    /// acts on the run, not the duel.
    ShopDiscount { percent: u32 },
}

fn one() -> u32 {
    1
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

    /// The tokens this modifier can put on the battlefield: those it starts
    /// a duel with and those its abilities create.
    fn tokens(&self) -> Vec<&CardDefId> {
        let mut tokens = Vec::new();
        for effect in &self.effects {
            match effect {
                ModifierEffect::StartingToken { token, .. } => tokens.push(token),
                ModifierEffect::Abilities { abilities } => {
                    for ability in abilities {
                        let effects = match ability {
                            AbilityDef::Spell { effects }
                            | AbilityDef::Triggered { effects, .. }
                            | AbilityDef::Activated { effects, .. } => effects.as_slice(),
                            _ => &[],
                        };
                        tokens.extend(effects.iter().filter_map(|effect| match effect {
                            Effect::CreateToken { token, .. } => Some(token),
                            _ => None,
                        }));
                    }
                }
                _ => {}
            }
        }
        tokens
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
    /// takes when the player doesn't choose another. None: it starts
    /// without one.
    #[serde(default)]
    pub emblem: Option<ModifierId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EnemyTier {
    Easy,
    Normal,
    /// The duels of the act's later rows.
    Hard,
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

/// A text event met at an event node: a few options, each a trade, of
/// which one may be taken. An event can always be walked away from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct EventDef {
    pub id: EventId,
    pub name: String,
    /// The scene, in words, for the player.
    pub text: String,
    /// How many relics the player doesn't own it shows, for `gain_relic`.
    #[serde(default)]
    pub relics: u8,
    /// How many cards in the player's colours it shows, for `gain_cards`.
    #[serde(default)]
    pub cards: u8,
    pub options: Vec<EventOptionDef>,
}

/// One thing an event offers: everything in `effects` happens, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EventOptionDef {
    /// What the option gives and costs, in words.
    pub text: String,
    pub effects: Vec<EventEffect>,
}

/// A cost or a gain of an event's option. An option is on offer only while
/// every cost can be paid and every gain can be had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventEffect {
    /// The player takes a wound. Never the one that would end the run: an
    /// event doesn't do that.
    TakeWound {},
    /// The player's latest wound is healed.
    HealWound {},
    GainGold {
        amount: u32,
    },
    PayGold {
        amount: u32,
    },
    /// One of the relics the event shows, of the player's choosing.
    GainRelic {},
    /// Every card the event shows joins the deck.
    GainCards {},
    /// A card of the player's choosing leaves the deck, and a random card
    /// of the next rarity up in their colours takes its place.
    Transmute {},
    /// The modifier is the player's for their next duel only.
    NextDuel {
        modifier: ModifierDef,
    },
}

impl EventEffect {
    /// Whether the player has to pick something for this effect: which
    /// relic, or which card of their deck.
    pub fn needs_pick(&self) -> bool {
        matches!(self, EventEffect::GainRelic {} | EventEffect::Transmute {})
    }
}

/// The run content loaded from `data/run/`. Each list is in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunData {
    pub planeswalkers: Vec<PlaneswalkerDef>,
    /// The starting emblems: the player takes one into the run.
    pub emblems: Vec<ModifierDef>,
    pub relics: Vec<ModifierDef>,
    /// What a lost duel leaves the player with until it is healed.
    pub wounds: Vec<ModifierDef>,
    pub enemies: Vec<EnemyDef>,
    pub events: Vec<EventDef>,
    /// What can be set out to do in a duel beyond winning it.
    pub feats: Vec<FeatDef>,
}

impl RunData {
    pub fn feat(&self, id: &FeatId) -> Option<&FeatDef> {
        self.feats.iter().find(|def| def.id == *id)
    }

    /// The feats offered to a character, in file order.
    pub fn feats_for<'a>(
        &'a self,
        planeswalker: &'a PlaneswalkerId,
    ) -> impl Iterator<Item = &'a FeatDef> {
        self.feats
            .iter()
            .filter(move |feat| feat.is_for(planeswalker))
    }

    pub fn wound(&self, id: &ModifierId) -> Option<&ModifierDef> {
        self.wounds.iter().find(|def| def.id == *id)
    }

    pub fn event(&self, id: &EventId) -> Option<&EventDef> {
        self.events.iter().find(|def| def.id == *id)
    }

    /// A modifier some event gives for the next duel.
    pub fn next_duel_modifier(&self, id: &ModifierId) -> Option<&ModifierDef> {
        self.events
            .iter()
            .flat_map(|event| &event.options)
            .flat_map(|option| &option.effects)
            .find_map(|effect| match effect {
                EventEffect::NextDuel { modifier } if modifier.id == *id => Some(modifier),
                _ => None,
            })
    }

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
    #[error("`{0}`: only a relic can give gold or lower prices")]
    RunEffectOnNonRelic(String),
    #[error("`{0}`: a discount is between 1 and 100 percent")]
    BadDiscount(String),
    #[error("`{id}`: `{token}` is not a file in tokens/")]
    UnknownToken { id: String, token: CardDefId },
    #[error("`{0}`: an event needs options, and each option an effect")]
    NoOptions(String),
    #[error("`{0}`: an option asks for a relic or cards the event doesn't show")]
    NothingShown(String),
    #[error("`{0}`: an option can ask the player to pick only one thing")]
    TwoPicks(String),
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
    #[error("there must be at least one wound")]
    NoWounds,
    #[error("`{0}`: a feat's text needs `{{n}}` where a tier's number goes")]
    FeatText(String),
    #[error("`{0}`: a feat needs one to three tiers, each harder than the one before")]
    FeatTiers(String),
    #[error("`{0}`: `by_turn` is above zero, and only for a measure that is a high point")]
    FeatDeadline(String),
    #[error("`{id}`: there is no planeswalker `{planeswalker}`")]
    UnknownPlaneswalker {
        id: String,
        planeswalker: PlaneswalkerId,
    },
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
    Wound,
    Passive,
    Enemy,
    /// What an event gives for the next duel.
    Event,
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
            | ModifierEffect::StartingToken { count: 0, .. }
            | ModifierEffect::GoldPerWin { amount: 0 } => {
                return Err(RunDataError::ZeroChange(id));
            }
            ModifierEffect::GoldPerWin { .. } | ModifierEffect::ShopDiscount { .. }
                if kind != ModifierKind::Relic =>
            {
                return Err(RunDataError::RunEffectOnNonRelic(id));
            }
            ModifierEffect::ShopDiscount { percent } if !(1..=100).contains(percent) => {
                return Err(RunDataError::BadDiscount(id));
            }
            _ => {}
        }
    }
    if let Some(emblem) = modifier.emblem() {
        validate_emblem(&emblem).map_err(|source| RunDataError::Emblem { id, source })?;
    }
    Ok(())
}

/// Checks that every token a modifier names is one of the token files.
pub fn validate_modifier_tokens(
    modifier: &ModifierDef,
    cards: &CardDatabase,
) -> Result<(), RunDataError> {
    match modifier
        .tokens()
        .into_iter()
        .find(|token| !cards.get(token).is_some_and(|def| def.token))
    {
        Some(token) => Err(RunDataError::UnknownToken {
            id: modifier.id.0.clone(),
            token: token.clone(),
        }),
        None => Ok(()),
    }
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
    unique(run.emblems.iter().map(|def| def.id.as_str()))?;
    for emblem in &run.emblems {
        validate_modifier(emblem, ModifierKind::Passive)?;
        validate_modifier_tokens(emblem, cards)?;
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
        if let Some(emblem) = &planeswalker.emblem
            && run.emblem(emblem).is_none()
        {
            return Err(RunDataError::UnknownEmblem {
                id: id.to_string(),
                emblem: emblem.clone(),
            });
        }
        if let Some(commander) = &planeswalker.commander {
            validate_commander(id, commander, &planeswalker.colors, cards)?;
        }
    }

    unique(run.relics.iter().map(|def| def.id.as_str()))?;
    for relic in &run.relics {
        validate_modifier(relic, ModifierKind::Relic)?;
        validate_modifier_tokens(relic, cards)?;
    }

    if run.wounds.is_empty() {
        return Err(RunDataError::NoWounds);
    }
    unique(run.wounds.iter().map(|def| def.id.as_str()))?;
    for wound in &run.wounds {
        validate_modifier(wound, ModifierKind::Wound)?;
        validate_modifier_tokens(wound, cards)?;
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
            validate_modifier_tokens(modifier, cards)?;
        }
    }
    unique(run.events.iter().map(|def| def.id.as_str()))?;
    let mut next_duel = BTreeSet::new();
    for event in &run.events {
        let id = event.id.as_str();
        check_id(id)?;
        if event.name.trim().is_empty() || event.text.trim().is_empty() {
            return Err(RunDataError::EmptyText(id.to_string()));
        }
        if event.options.is_empty() || event.options.iter().any(|o| o.effects.is_empty()) {
            return Err(RunDataError::NoOptions(id.to_string()));
        }
        for option in &event.options {
            if option.text.trim().is_empty() {
                return Err(RunDataError::EmptyText(id.to_string()));
            }
            if option.effects.iter().filter(|e| e.needs_pick()).count() > 1 {
                return Err(RunDataError::TwoPicks(id.to_string()));
            }
            for effect in &option.effects {
                match effect {
                    EventEffect::GainGold { amount: 0 } | EventEffect::PayGold { amount: 0 } => {
                        return Err(RunDataError::ZeroChange(id.to_string()));
                    }
                    EventEffect::GainRelic {} if event.relics == 0 => {
                        return Err(RunDataError::NothingShown(id.to_string()));
                    }
                    EventEffect::GainCards {} if event.cards == 0 => {
                        return Err(RunDataError::NothingShown(id.to_string()));
                    }
                    EventEffect::NextDuel { modifier } => {
                        validate_modifier(modifier, ModifierKind::Event)?;
                        validate_modifier_tokens(modifier, cards)?;
                        if !next_duel.insert(modifier.id.as_str()) {
                            return Err(RunDataError::DuplicateId(modifier.id.0.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    unique(run.feats.iter().map(|def| def.id.as_str()))?;
    for feat in &run.feats {
        validate_feat(feat, run)?;
    }

    // Hard is not asked for: with no hard enemy a later row meets a normal
    // one.
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

fn validate_feat(feat: &FeatDef, run: &RunData) -> Result<(), RunDataError> {
    let id = feat.id.as_str();
    check_id(id)?;
    if feat.name.trim().is_empty() {
        return Err(RunDataError::EmptyText(id.to_string()));
    }
    if !feat.text.contains("{n}") {
        return Err(RunDataError::FeatText(id.to_string()));
    }
    // Each tier is harder than the one before: a smaller number where
    // fewer is better, a larger one otherwise.
    let harder = |pair: &[i32]| {
        if feat.measure.fewer_is_better() {
            pair[1] < pair[0]
        } else {
            pair[1] > pair[0]
        }
    };
    if !(1..=3).contains(&feat.tiers.len()) || !feat.tiers.windows(2).all(harder) {
        return Err(RunDataError::FeatTiers(id.to_string()));
    }
    if feat
        .by_turn
        .is_some_and(|turn| turn == 0 || !feat.measure.is_high_point())
    {
        return Err(RunDataError::FeatDeadline(id.to_string()));
    }
    unique(feat.planeswalkers.iter().map(PlaneswalkerId::as_str))?;
    match feat
        .planeswalkers
        .iter()
        .find(|planeswalker| run.planeswalker(planeswalker).is_none())
    {
        Some(planeswalker) => Err(RunDataError::UnknownPlaneswalker {
            id: id.to_string(),
            planeswalker: planeswalker.clone(),
        }),
        None => Ok(()),
    }
}
