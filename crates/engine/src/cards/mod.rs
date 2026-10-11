//! Card definitions: the data format of `data/cards/*.json` (see
//! `docs/ARCHITECTURE.md` §4) and its validation.
//!
//! Every enum here lists only what a card in the pool uses. Unknown fields,
//! keywords, effects and targets fail to deserialise, so a typo in a card
//! file is an error rather than a silently ignored ability.

mod emblem;
mod mana;

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub use emblem::{EmblemDef, EmblemError, validate_emblem};
pub use mana::{Color, ManaCost, ManaCostError, ManaSymbol};

/// Identifies a card definition, e.g. `lightning_strike`. Game state refers
/// to definitions by this id and never embeds them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct CardDefId(pub String);

impl CardDefId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CardDefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct CardDef {
    pub id: CardDefId,
    pub name: String,
    #[ts(type = "string")]
    pub mana_cost: ManaCost,
    #[serde(default)]
    pub colors: Vec<Color>,
    #[serde(default)]
    pub supertypes: Vec<Supertype>,
    pub types: Vec<CardType>,
    #[serde(default)]
    pub subtypes: Vec<String>,
    #[serde(default)]
    pub power: Option<i32>,
    #[serde(default)]
    pub toughness: Option<i32>,
    pub rarity: Rarity,
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    #[serde(default)]
    pub oracle_text: String,
    /// What an Aura can enchant.
    #[serde(default)]
    pub enchant: Option<TargetSpec>,
    #[serde(default)]
    pub abilities: Vec<AbilityDef>,
}

impl CardDef {
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }

    pub fn is_land(&self) -> bool {
        self.has_type(CardType::Land)
    }

    pub fn is_creature(&self) -> bool {
        self.has_type(CardType::Creature)
    }

    pub fn is_aura(&self) -> bool {
        self.subtypes.iter().any(|subtype| subtype == "Aura")
    }

    pub fn is_instant_or_sorcery(&self) -> bool {
        self.has_type(CardType::Instant) || self.has_type(CardType::Sorcery)
    }

    pub fn has_keyword(&self, keyword: Keyword) -> bool {
        self.keywords.contains(&keyword)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum Supertype {
    Basic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum CardType {
    Creature,
    Enchantment,
    Instant,
    Land,
    Sorcery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Mythic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Keyword {
    Deathtouch,
    FirstStrike,
    Flying,
    Haste,
    Lifelink,
    Menace,
    Reach,
    Trample,
    Vigilance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AbilityDef {
    /// What an instant or sorcery does when it resolves.
    Spell {
        effects: Vec<Effect>,
    },
    Triggered {
        trigger: Trigger,
        effects: Vec<Effect>,
    },
    Static(StaticAbility),
    /// A mana ability; it doesn't use the stack.
    Mana {
        cost: AbilityCost,
        #[ts(type = "string")]
        produces: ManaCost,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AbilityCost {
    #[serde(default)]
    pub tap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    EntersBattlefield {
        subject: TriggerSubject,
    },
    /// At the beginning of its controller's upkeep. Emblems only, so far.
    BeginningOfUpkeep {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSubject {
    /// The permanent the ability is on.
    #[serde(rename = "self")]
    This,
    /// Any creature entering under the controller's control. Emblems only,
    /// so far.
    CreatureYouControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StaticAbility {
    Restriction {
        affects: Affects,
        cant: Vec<Restricted>,
    },
    /// The affected creatures get +power/+toughness.
    Anthem {
        affects: Affects,
        power: i32,
        toughness: i32,
    },
    GrantKeyword {
        affects: Affects,
        keyword: Keyword,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Affects {
    /// The permanent this Aura is attached to.
    Enchanted,
    /// Every creature the controller controls. Emblems only, so far.
    CreaturesYouControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Restricted {
    Attack,
    Block,
}

/// An effect primitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    CounterSpell {
        target: TargetSpec,
    },
    DealDamage {
        amount: u32,
        target: TargetSpec,
    },
    Destroy {
        target: TargetSpec,
    },
    DrawCards {
        player: PlayerRef,
        amount: u32,
    },
    GainLife {
        player: PlayerRef,
        amount: u32,
    },
    LoseLife {
        player: PlayerRef,
        amount: u32,
    },
    ModifyPtUntilEot {
        target: TargetSpec,
        power: i32,
        toughness: i32,
    },
}

impl Effect {
    /// The effect's name as written in card data.
    pub fn name(&self) -> &'static str {
        match self {
            Effect::CounterSpell { .. } => "counter_spell",
            Effect::DealDamage { .. } => "deal_damage",
            Effect::Destroy { .. } => "destroy",
            Effect::DrawCards { .. } => "draw_cards",
            Effect::GainLife { .. } => "gain_life",
            Effect::LoseLife { .. } => "lose_life",
            Effect::ModifyPtUntilEot { .. } => "modify_pt_until_eot",
        }
    }

    /// The target this effect needs chosen when it goes on the stack.
    pub fn target(&self) -> Option<&TargetSpec> {
        match self {
            Effect::CounterSpell { target }
            | Effect::DealDamage { target, .. }
            | Effect::Destroy { target }
            | Effect::ModifyPtUntilEot { target, .. } => Some(target),
            Effect::DrawCards { .. } | Effect::GainLife { .. } | Effect::LoseLife { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRef {
    /// The controller of the spell or ability.
    Controller,
    /// The controller's opponent.
    Opponent,
}

/// What a target must be. Written as `{ "creature": {} }`; the braces are
/// where predicates (`power_lte`, `has_keyword`...) will go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetSpec {
    /// A creature or a player.
    AnyTarget {},
    Creature {},
    /// A spell on the stack.
    Spell {},
}

impl TargetSpec {
    /// The spec's name as written in card data.
    pub fn name(&self) -> &'static str {
        match self {
            TargetSpec::AnyTarget {} => "any_target",
            TargetSpec::Creature {} => "creature",
            TargetSpec::Spell {} => "spell",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardError {
    #[error("id `{0}` must be lower snake_case")]
    BadId(String),
    #[error("name is empty")]
    EmptyName,
    #[error("card has no card type")]
    NoTypes,
    #[error("`{0}` has a repeated entry")]
    Duplicate(&'static str),
    #[error("creatures need power and toughness, and only creatures may have them")]
    PowerToughness,
    #[error("colors do not match the colours in the mana cost `{0}`")]
    ColorsMismatch(String),
    #[error("a land cannot have a mana cost")]
    LandWithCost,
    #[error("an Aura must be an Enchantment")]
    AuraNotEnchantment,
    #[error("Auras need `enchant`, and only Auras may have it")]
    Enchant,
    #[error("instants and sorceries need exactly one ability, of kind `spell`")]
    SpellAbility,
    #[error("only instants and sorceries may have a `spell` ability")]
    SpellAbilityOnPermanent,
    #[error("an ability needs at least one effect")]
    NoEffects,
    #[error("triggered abilities with targets are not supported yet")]
    TargetedTrigger,
    #[error("`{effect}` cannot target `{target}`")]
    BadTarget {
        effect: &'static str,
        target: &'static str,
    },
    #[error("`{0}` needs an amount above zero")]
    ZeroAmount(&'static str),
    #[error("a mana ability must cost `tap`")]
    ManaCost,
    #[error("a mana ability must produce exactly one coloured mana")]
    ManaProduces,
    #[error("a restriction needs at least one entry in `cant`")]
    EmptyRestriction,
    #[error("`affects: enchanted` is only valid on an Aura")]
    EnchantedOnNonAura,
    #[error("`{0}` is only supported on emblems so far")]
    EmblemOnly(&'static str),
}

/// Checks everything about a card that its JSON shape alone doesn't
/// guarantee.
pub fn validate(card: &CardDef) -> Result<(), CardError> {
    let id = card.id.as_str();
    let id_ok = !id.is_empty()
        && !id.starts_with('_')
        && !id.ends_with('_')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if !id_ok {
        return Err(CardError::BadId(id.to_string()));
    }
    if card.name.trim().is_empty() {
        return Err(CardError::EmptyName);
    }
    if card.types.is_empty() {
        return Err(CardError::NoTypes);
    }
    no_duplicates("colors", &card.colors)?;
    no_duplicates("supertypes", &card.supertypes)?;
    no_duplicates("types", &card.types)?;
    no_duplicates("subtypes", &card.subtypes)?;
    no_duplicates("keywords", &card.keywords)?;

    let has_pt = card.power.is_some() && card.toughness.is_some();
    let has_no_pt = card.power.is_none() && card.toughness.is_none();
    if !(card.is_creature() && has_pt || !card.is_creature() && has_no_pt) {
        return Err(CardError::PowerToughness);
    }
    if card.colors.iter().copied().collect::<BTreeSet<_>>() != card.mana_cost.colors() {
        return Err(CardError::ColorsMismatch(card.mana_cost.to_string()));
    }
    if card.is_land() && !card.mana_cost.is_empty() {
        return Err(CardError::LandWithCost);
    }

    if card.is_aura() && !card.has_type(CardType::Enchantment) {
        return Err(CardError::AuraNotEnchantment);
    }
    match (&card.enchant, card.is_aura()) {
        (Some(TargetSpec::Creature {}), true) | (None, false) => {}
        (Some(target), true) => {
            return Err(CardError::BadTarget {
                effect: "enchant",
                target: target.name(),
            });
        }
        (None, true) | (Some(_), false) => return Err(CardError::Enchant),
    }

    let is_spell_ability = |ability: &AbilityDef| matches!(ability, AbilityDef::Spell { .. });
    if card.is_instant_or_sorcery() {
        if card.abilities.len() != 1 || !card.abilities.iter().all(is_spell_ability) {
            return Err(CardError::SpellAbility);
        }
    } else if card.abilities.iter().any(is_spell_ability) {
        return Err(CardError::SpellAbilityOnPermanent);
    }
    card.abilities
        .iter()
        .try_for_each(|ability| validate_ability(card, ability))
}

fn validate_ability(card: &CardDef, ability: &AbilityDef) -> Result<(), CardError> {
    match ability {
        AbilityDef::Triggered {
            trigger: Trigger::BeginningOfUpkeep {},
            ..
        } => Err(CardError::EmblemOnly("beginning_of_upkeep")),
        AbilityDef::Triggered {
            trigger:
                Trigger::EntersBattlefield {
                    subject: TriggerSubject::CreatureYouControl,
                },
            ..
        } => Err(CardError::EmblemOnly("creature_you_control")),
        AbilityDef::Spell { effects } | AbilityDef::Triggered { effects, .. } => {
            if effects.is_empty() {
                return Err(CardError::NoEffects);
            }
            let is_trigger = matches!(ability, AbilityDef::Triggered { .. });
            if is_trigger && effects.iter().any(|effect| effect.target().is_some()) {
                return Err(CardError::TargetedTrigger);
            }
            effects.iter().try_for_each(validate_effect)
        }
        AbilityDef::Static(StaticAbility::Restriction { affects, cant }) => {
            if cant.is_empty() {
                return Err(CardError::EmptyRestriction);
            }
            no_duplicates("cant", cant)?;
            match affects {
                Affects::Enchanted if card.is_aura() => Ok(()),
                Affects::Enchanted => Err(CardError::EnchantedOnNonAura),
                Affects::CreaturesYouControl => Err(CardError::EmblemOnly("creatures_you_control")),
            }
        }
        AbilityDef::Static(StaticAbility::Anthem { .. }) => Err(CardError::EmblemOnly("anthem")),
        AbilityDef::Static(StaticAbility::GrantKeyword { .. }) => {
            Err(CardError::EmblemOnly("grant_keyword"))
        }
        AbilityDef::Mana { cost, produces } => {
            if !cost.tap {
                return Err(CardError::ManaCost);
            }
            // One mana per source keeps the auto-payer a simple greedy pick.
            if produces.symbols().len() != 1 || produces.colored().count() != 1 {
                return Err(CardError::ManaProduces);
            }
            Ok(())
        }
    }
}

pub(crate) fn validate_effect(effect: &Effect) -> Result<(), CardError> {
    let target_ok = match effect {
        Effect::CounterSpell { target } => matches!(target, TargetSpec::Spell {}),
        Effect::DealDamage { target, .. } => {
            matches!(target, TargetSpec::AnyTarget {} | TargetSpec::Creature {})
        }
        Effect::Destroy { target } | Effect::ModifyPtUntilEot { target, .. } => {
            matches!(target, TargetSpec::Creature {})
        }
        Effect::DrawCards { .. } | Effect::GainLife { .. } | Effect::LoseLife { .. } => true,
    };
    if let (false, Some(target)) = (target_ok, effect.target()) {
        return Err(CardError::BadTarget {
            effect: effect.name(),
            target: target.name(),
        });
    }
    match effect {
        Effect::DealDamage { amount: 0, .. }
        | Effect::DrawCards { amount: 0, .. }
        | Effect::GainLife { amount: 0, .. }
        | Effect::LoseLife { amount: 0, .. } => Err(CardError::ZeroAmount(effect.name())),
        _ => Ok(()),
    }
}

fn no_duplicates<T: Ord>(field: &'static str, items: &[T]) -> Result<(), CardError> {
    let unique: BTreeSet<&T> = items.iter().collect();
    if unique.len() == items.len() {
        Ok(())
    } else {
        Err(CardError::Duplicate(field))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn bears() -> Value {
        json!({
            "id": "grizzly_bears",
            "name": "Grizzly Bears",
            "mana_cost": "{1}{G}",
            "colors": ["G"],
            "types": ["Creature"],
            "subtypes": ["Bear"],
            "power": 2, "toughness": 2,
            "rarity": "common"
        })
    }

    fn shock() -> Value {
        json!({
            "id": "shock",
            "name": "Shock",
            "mana_cost": "{R}",
            "colors": ["R"],
            "types": ["Instant"],
            "rarity": "common",
            "abilities": [{
                "kind": "spell",
                "effects": [{ "type": "deal_damage", "amount": 2, "target": { "any_target": {} } }]
            }]
        })
    }

    fn pacifism() -> Value {
        json!({
            "id": "pacifism",
            "name": "Pacifism",
            "mana_cost": "{1}{W}",
            "colors": ["W"],
            "types": ["Enchantment"],
            "subtypes": ["Aura"],
            "rarity": "common",
            "enchant": { "creature": {} },
            "abilities": [
                { "kind": "static", "type": "restriction", "affects": "enchanted", "cant": ["attack", "block"] }
            ]
        })
    }

    fn forest() -> Value {
        json!({
            "id": "forest",
            "name": "Forest",
            "mana_cost": "",
            "supertypes": ["Basic"],
            "types": ["Land"],
            "subtypes": ["Forest"],
            "rarity": "common",
            "abilities": [{ "kind": "mana", "cost": { "tap": true }, "produces": "{G}" }]
        })
    }

    /// Returns `base` with `field` set to `value` (or removed, for null).
    fn with(mut base: Value, field: &str, value: Value) -> Value {
        let object = base.as_object_mut().unwrap();
        if value.is_null() {
            object.remove(field);
        } else {
            object.insert(field.to_string(), value);
        }
        base
    }

    fn check(card: Value) -> Result<(), CardError> {
        validate(&serde_json::from_value::<CardDef>(card).unwrap())
    }

    fn parses(card: Value) -> bool {
        serde_json::from_value::<CardDef>(card).is_ok()
    }

    #[test]
    fn valid_cards_of_each_shape_pass() {
        for card in [bears(), shock(), pacifism(), forest()] {
            assert_eq!(check(card.clone()), Ok(()), "{card}");
        }
    }

    #[test]
    fn omitted_optional_fields_default_to_empty() {
        let card: CardDef = serde_json::from_value(forest()).unwrap();
        assert!(card.colors.is_empty());
        assert!(card.keywords.is_empty());
        assert_eq!(card.oracle_text, "");
        assert_eq!(card.power, None);
        assert_eq!(card.enchant, None);
    }

    #[test]
    fn card_round_trips_through_json() {
        for card in [bears(), shock(), pacifism(), forest()] {
            let def: CardDef = serde_json::from_value(card).unwrap();
            let json = serde_json::to_string(&def).unwrap();
            assert_eq!(serde_json::from_str::<CardDef>(&json).unwrap(), def);
        }
    }

    #[test]
    fn unknown_names_fail_to_parse() {
        assert!(!parses(with(bears(), "keywords", json!(["flyign"]))));
        assert!(!parses(with(bears(), "types", json!(["Planeswalker"]))));
        assert!(!parses(with(bears(), "mana_cost", json!("{X}{G}"))));
        assert!(!parses(with(bears(), "colors", json!(["green"]))));
        assert!(!parses(with(bears(), "rarity", json!("bonus"))));
        assert!(!parses(with(bears(), "powr", json!(2))));
        assert!(!parses(with(bears(), "rarity", Value::Null)));
        assert!(!parses(with(
            shock(),
            "abilities",
            json!([{ "kind": "spell", "effects": [{ "type": "explode" }] }])
        )));
        assert!(!parses(with(
            shock(),
            "abilities",
            json!([{ "kind": "spell", "effects": [
                { "type": "deal_damage", "amount": 2, "target": { "planeswalker": {} } }
            ] }])
        )));
        assert!(!parses(with(
            shock(),
            "abilities",
            json!([{ "kind": "spell", "effects": [
                { "type": "deal_damage", "amount": 2, "target": { "any_target": {} }, "extra": 1 }
            ] }])
        )));
        assert!(!parses(with(
            pacifism(),
            "abilities",
            json!([{ "kind": "static", "type": "restriction", "affects": "enchanted", "cant": ["untap"] }])
        )));
    }

    #[test]
    fn rejects_bad_ids_and_names() {
        for id in [
            "",
            "Grizzly_Bears",
            "grizzly bears",
            "_bears",
            "bears_",
            "grizzly-bears",
        ] {
            assert_eq!(
                check(with(bears(), "id", json!(id))),
                Err(CardError::BadId(id.to_string()))
            );
        }
        assert_eq!(
            check(with(bears(), "name", json!(" "))),
            Err(CardError::EmptyName)
        );
    }

    #[test]
    fn rejects_missing_types_and_repeats() {
        assert_eq!(
            check(with(shock(), "types", json!([]))),
            Err(CardError::NoTypes)
        );
        assert_eq!(
            check(with(bears(), "keywords", json!(["flying", "flying"]))),
            Err(CardError::Duplicate("keywords"))
        );
    }

    #[test]
    fn power_and_toughness_belong_to_creatures_only() {
        assert_eq!(
            check(with(bears(), "power", Value::Null)),
            Err(CardError::PowerToughness)
        );
        assert_eq!(
            check(with(shock(), "toughness", json!(1))),
            Err(CardError::PowerToughness)
        );
    }

    #[test]
    fn colours_must_match_the_mana_cost() {
        let expected = Err(CardError::ColorsMismatch("{1}{G}".to_string()));
        assert_eq!(check(with(bears(), "colors", json!(["R"]))), expected);
        assert_eq!(check(with(bears(), "colors", json!([]))), expected);
        assert_eq!(check(with(bears(), "colors", json!(["G", "W"]))), expected);
    }

    #[test]
    fn lands_have_no_mana_cost() {
        assert_eq!(
            check(with(forest(), "mana_cost", json!("{0}"))),
            Err(CardError::LandWithCost)
        );
    }

    #[test]
    fn auras_need_enchant_and_only_auras_have_it() {
        assert_eq!(
            check(with(pacifism(), "enchant", Value::Null)),
            Err(CardError::Enchant)
        );
        assert_eq!(
            check(with(bears(), "enchant", json!({ "creature": {} }))),
            Err(CardError::Enchant)
        );
        assert_eq!(
            check(with(pacifism(), "enchant", json!({ "spell": {} }))),
            Err(CardError::BadTarget {
                effect: "enchant",
                target: "spell"
            })
        );
        assert_eq!(
            check(with(pacifism(), "types", json!(["Sorcery"]))),
            Err(CardError::AuraNotEnchantment)
        );
    }

    #[test]
    fn spell_abilities_belong_to_instants_and_sorceries() {
        let spell = shock()["abilities"].clone();
        assert_eq!(
            check(with(shock(), "abilities", json!([]))),
            Err(CardError::SpellAbility)
        );
        assert_eq!(
            check(with(shock(), "abilities", json!([spell[0], spell[0]]))),
            Err(CardError::SpellAbility)
        );
        assert_eq!(
            check(with(bears(), "abilities", spell)),
            Err(CardError::SpellAbilityOnPermanent)
        );
        assert_eq!(
            check(with(
                shock(),
                "abilities",
                json!([{ "kind": "spell", "effects": [] }])
            )),
            Err(CardError::NoEffects)
        );
    }

    #[test]
    fn effects_only_take_targets_that_make_sense() {
        let spell = |effect: Value| {
            with(
                shock(),
                "abilities",
                json!([{ "kind": "spell", "effects": [effect] }]),
            )
        };
        assert_eq!(
            check(spell(
                json!({ "type": "counter_spell", "target": { "creature": {} } })
            )),
            Err(CardError::BadTarget {
                effect: "counter_spell",
                target: "creature"
            })
        );
        assert_eq!(
            check(spell(
                json!({ "type": "destroy", "target": { "any_target": {} } })
            )),
            Err(CardError::BadTarget {
                effect: "destroy",
                target: "any_target"
            })
        );
        assert_eq!(
            check(spell(
                json!({ "type": "deal_damage", "amount": 2, "target": { "spell": {} } })
            )),
            Err(CardError::BadTarget {
                effect: "deal_damage",
                target: "spell"
            })
        );
        assert_eq!(
            check(spell(json!({
                "type": "modify_pt_until_eot", "power": 3, "toughness": 3, "target": { "spell": {} }
            }))),
            Err(CardError::BadTarget {
                effect: "modify_pt_until_eot",
                target: "spell"
            })
        );
    }

    #[test]
    fn triggered_abilities_cannot_target_yet() {
        let trigger = |effect: Value| {
            json!([{
                "kind": "triggered",
                "trigger": { "type": "enters_battlefield", "subject": "self" },
                "effects": [effect]
            }])
        };
        assert_eq!(
            check(with(
                bears(),
                "abilities",
                trigger(json!({ "type": "draw_cards", "player": "controller", "amount": 1 }))
            )),
            Ok(())
        );
        assert_eq!(
            check(with(
                bears(),
                "abilities",
                trigger(json!({ "type": "destroy", "target": { "creature": {} } }))
            )),
            Err(CardError::TargetedTrigger)
        );
    }

    #[test]
    fn amounts_must_be_above_zero() {
        let spell = |effect: Value| {
            with(
                shock(),
                "abilities",
                json!([{ "kind": "spell", "effects": [effect] }]),
            )
        };
        assert_eq!(
            check(spell(
                json!({ "type": "draw_cards", "player": "controller", "amount": 0 })
            )),
            Err(CardError::ZeroAmount("draw_cards"))
        );
        assert_eq!(
            check(spell(
                json!({ "type": "deal_damage", "amount": 0, "target": { "creature": {} } })
            )),
            Err(CardError::ZeroAmount("deal_damage"))
        );
    }

    #[test]
    fn mana_abilities_tap_for_coloured_mana() {
        let land = |ability: Value| with(forest(), "abilities", json!([ability]));
        assert_eq!(
            check(land(
                json!({ "kind": "mana", "cost": {}, "produces": "{G}" })
            )),
            Err(CardError::ManaCost)
        );
        for produces in ["", "{1}", "{1}{G}", "{G}{G}"] {
            assert_eq!(
                check(land(
                    json!({ "kind": "mana", "cost": { "tap": true }, "produces": produces })
                )),
                Err(CardError::ManaProduces)
            );
        }
    }

    #[test]
    fn restrictions_need_entries_and_an_aura() {
        let restriction = |cant: Value| json!([{ "kind": "static", "type": "restriction", "affects": "enchanted", "cant": cant }]);
        assert_eq!(
            check(with(pacifism(), "abilities", restriction(json!([])))),
            Err(CardError::EmptyRestriction)
        );
        assert_eq!(
            check(with(bears(), "abilities", restriction(json!(["attack"])))),
            Err(CardError::EnchantedOnNonAura)
        );
    }
}
