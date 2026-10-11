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
    /// "This land enters tapped." A property, not a replacement effect.
    #[serde(default)]
    pub enters_tapped: bool,
    /// A token, made by an effect and never in a deck. The loader decides
    /// it by folder: true for `data/tokens/`, false for `data/cards/`,
    /// whatever the file says.
    #[serde(default)]
    pub token: bool,
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

    /// The colours of mana this card's tap abilities can make, in the
    /// order of the abilities.
    pub fn mana_colors(&self) -> Vec<Color> {
        let mut colors = Vec::new();
        for ability in &self.abilities {
            if let AbilityDef::Mana { cost, produces } = ability
                && cost.tap
            {
                for color in produces.colored() {
                    if !colors.contains(&color) {
                        colors.push(color);
                    }
                }
            }
        }
        colors
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
    Defender,
    DoubleStrike,
    FirstStrike,
    Flash,
    Flying,
    Haste,
    Hexproof,
    Indestructible,
    Lifelink,
    Menace,
    Prowess,
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
        /// "The first time each turn": it triggers once a turn at most.
        #[serde(default)]
        once_each_turn: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    EntersBattlefield {
        subject: TriggerSubject,
    },
    /// The subject is put into a graveyard from the battlefield.
    Dies {
        subject: TriggerSubject,
    },
    /// The permanent the ability is on attacks.
    Attacks {},
    /// The permanent the ability is on deals damage to an opponent.
    DealsDamageToOpponent {},
    /// The controller gains life.
    GainLife {},
    /// The controller casts a spell of this kind.
    CastSpell {
        kind: CastKind,
    },
    /// At the beginning of its controller's upkeep.
    BeginningOfUpkeep {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CastKind {
    InstantOrSorcery,
    Noncreature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSubject {
    /// The permanent the ability is on.
    #[serde(rename = "self")]
    This,
    /// Any creature the controller controls, the one the ability is on
    /// included.
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
        /// "With flying": only those that have this keyword.
        #[serde(default)]
        with_keyword: Option<Keyword>,
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
    /// Every creature the controller controls.
    CreaturesYouControl,
    /// Every creature the controller controls except the one the ability
    /// is on.
    OtherCreaturesYouControl,
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
    /// The controller creates `count` of the token `token`, a file in
    /// `data/tokens/`.
    CreateToken {
        token: CardDefId,
        #[serde(default = "one")]
        count: u32,
    },
    /// `source` deals damage equal to its power to `target`.
    DealPowerDamage {
        source: TargetSpec,
        target: TargetSpec,
    },
    Destroy {
        target: TargetSpec,
    },
    /// The player discards cards: of their choice, or at random.
    Discard {
        player: PlayerRef,
        amount: u32,
        #[serde(default)]
        random: bool,
    },
    DrawCards {
        player: PlayerRef,
        amount: u32,
    },
    GainLife {
        player: PlayerRef,
        amount: u32,
    },
    GrantKeywordUntilEot {
        target: TargetSpec,
        keyword: Keyword,
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
    ReturnToHand {
        target: TargetSpec,
    },
    /// The controller looks at the top cards of their library, one at a
    /// time, and puts each back or on the bottom.
    Scry {
        amount: u32,
    },
    /// Taps the target. With `skip_next_untap` it also stays tapped through
    /// its controller's next untap step.
    Tap {
        target: TargetSpec,
        #[serde(default)]
        skip_next_untap: bool,
    },
}

impl Effect {
    /// The effect's name as written in card data.
    pub fn name(&self) -> &'static str {
        match self {
            Effect::CounterSpell { .. } => "counter_spell",
            Effect::CreateToken { .. } => "create_token",
            Effect::DealDamage { .. } => "deal_damage",
            Effect::DealPowerDamage { .. } => "deal_power_damage",
            Effect::Destroy { .. } => "destroy",
            Effect::Discard { .. } => "discard",
            Effect::DrawCards { .. } => "draw_cards",
            Effect::GainLife { .. } => "gain_life",
            Effect::GrantKeywordUntilEot { .. } => "grant_keyword_until_eot",
            Effect::LoseLife { .. } => "lose_life",
            Effect::ModifyPtUntilEot { .. } => "modify_pt_until_eot",
            Effect::ReturnToHand { .. } => "return_to_hand",
            Effect::Scry { .. } => "scry",
            Effect::Tap { .. } => "tap",
        }
    }

    /// What this effect is aimed at, in the order the effect uses them. A
    /// `same` entry stands for the target chosen just before it; the others
    /// are chosen when the spell or ability goes on the stack.
    pub fn targets(&self) -> Vec<TargetSpec> {
        match self {
            Effect::CounterSpell { target }
            | Effect::DealDamage { target, .. }
            | Effect::Destroy { target }
            | Effect::GrantKeywordUntilEot { target, .. }
            | Effect::ModifyPtUntilEot { target, .. }
            | Effect::ReturnToHand { target }
            | Effect::Tap { target, .. } => vec![*target],
            Effect::DealPowerDamage { source, target } => vec![*source, *target],
            Effect::Discard { player, .. }
            | Effect::DrawCards { player, .. }
            | Effect::GainLife { player, .. }
            | Effect::LoseLife { player, .. } => player.target().into_iter().collect(),
            Effect::CreateToken { .. } | Effect::Scry { .. } => Vec::new(),
        }
    }
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRef {
    /// The controller of the spell or ability.
    Controller,
    /// The controller's opponent.
    Opponent,
    /// "Target player".
    TargetPlayer,
    /// "Target opponent".
    TargetOpponent,
}

impl PlayerRef {
    /// The target this needs chosen, if it names a targeted player.
    pub fn target(self) -> Option<TargetSpec> {
        match self {
            PlayerRef::Controller | PlayerRef::Opponent => None,
            PlayerRef::TargetPlayer => Some(TargetSpec::Player {}),
            PlayerRef::TargetOpponent => Some(TargetSpec::Opponent {}),
        }
    }
}

/// What a target must be. Written as `{ "creature": {} }`; the braces hold
/// any further conditions, e.g. `{ "creature": { "min_power": 4 } }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetSpec {
    /// A creature or a player.
    AnyTarget {},
    Creature(CreatureFilter),
    /// An opponent of the spell or ability's controller.
    Opponent {},
    Player {},
    /// Not a new target: the one chosen just before, as in "target creature
    /// gets +3/+0 and gains first strike".
    Same {},
    /// Not a target: the permanent the ability is on ("this creature gets
    /// +2/+0").
    This {},
    /// Not a target: what made the ability trigger ("that creature gains
    /// haste").
    Triggering {},
    /// A spell on the stack.
    Spell(SpellFilter),
}

/// Conditions a creature must meet to be a target. Empty means any creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreatureFilter {
    /// "Nonblack": the creature must not be this colour.
    #[serde(default)]
    pub not_color: Option<Color>,
    /// "With power N or greater".
    #[serde(default)]
    pub min_power: Option<i32>,
    /// "With flying": the creature must have this keyword.
    #[serde(default)]
    pub keyword: Option<Keyword>,
    /// "You control" or "an opponent controls", from the side of whoever
    /// controls the spell or ability.
    #[serde(default)]
    pub controller: Option<ControlledBy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ControlledBy {
    You,
    Opponent,
}

/// Conditions a spell must meet to be a target. Empty means any spell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SpellFilter {
    #[serde(default)]
    pub kind: Option<SpellKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SpellKind {
    Creature,
    Noncreature,
}

impl TargetSpec {
    /// Any creature.
    pub fn creature() -> TargetSpec {
        TargetSpec::Creature(CreatureFilter::default())
    }

    /// Any spell.
    pub fn spell() -> TargetSpec {
        TargetSpec::Spell(SpellFilter::default())
    }
}

impl TargetSpec {
    /// Whether this is chosen when the spell or ability goes on the stack.
    /// The others stand for something already known.
    pub fn is_chosen(&self) -> bool {
        !matches!(
            self,
            TargetSpec::Same {} | TargetSpec::This {} | TargetSpec::Triggering {}
        )
    }

    /// The spec's name as written in card data.
    pub fn name(&self) -> &'static str {
        match self {
            TargetSpec::AnyTarget {} => "any_target",
            TargetSpec::Creature(_) => "creature",
            TargetSpec::Opponent {} => "opponent",
            TargetSpec::Player {} => "player",
            TargetSpec::Same {} => "same",
            TargetSpec::This {} => "this",
            TargetSpec::Triggering {} => "triggering",
            TargetSpec::Spell(_) => "spell",
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
    #[error("a token must be a creature with no mana cost")]
    Token,
    #[error("`create_token` needs a count above zero")]
    NoTokens,
    #[error("only lands may have `enters_tapped`")]
    EntersTapped,
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
    #[error("an emblem's triggered abilities can't have targets yet")]
    TargetedTrigger,
    #[error("`same` needs a target chosen before it")]
    SameWithoutTarget,
    #[error("`{0}` is only for triggered abilities")]
    NotOnASpell(&'static str),
    #[error("`triggering` needs a trigger that something else sets off")]
    NothingTriggering,
    #[error("only `enters_battlefield` and `dies` take the subject `creature_you_control`")]
    BadSubject,
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
    // A token has a colour and no cost.
    let colors_from_cost = card.colors.iter().copied().collect::<BTreeSet<_>>();
    if card.token {
        if !card.mana_cost.is_empty() || !card.is_creature() {
            return Err(CardError::Token);
        }
    } else if colors_from_cost != card.mana_cost.colors() {
        return Err(CardError::ColorsMismatch(card.mana_cost.to_string()));
    }
    if card.is_land() && !card.mana_cost.is_empty() {
        return Err(CardError::LandWithCost);
    }
    if card.enters_tapped && !card.is_land() {
        return Err(CardError::EntersTapped);
    }

    if card.is_aura() && !card.has_type(CardType::Enchantment) {
        return Err(CardError::AuraNotEnchantment);
    }
    match (&card.enchant, card.is_aura()) {
        (Some(TargetSpec::Creature(_)), true) | (None, false) => {}
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
        AbilityDef::Spell { effects } => {
            if effects.is_empty() {
                return Err(CardError::NoEffects);
            }
            for spec in effects.iter().flat_map(Effect::targets) {
                if matches!(spec, TargetSpec::This {} | TargetSpec::Triggering {}) {
                    return Err(CardError::NotOnASpell(spec.name()));
                }
            }
            validate_effects(effects)
        }
        AbilityDef::Triggered {
            trigger, effects, ..
        } => validate_triggered(trigger, effects),
        AbilityDef::Static(StaticAbility::Restriction { affects, cant }) => {
            if cant.is_empty() {
                return Err(CardError::EmptyRestriction);
            }
            no_duplicates("cant", cant)?;
            match affects {
                Affects::Enchanted if card.is_aura() => Ok(()),
                Affects::Enchanted => Err(CardError::EnchantedOnNonAura),
                Affects::CreaturesYouControl | Affects::OtherCreaturesYouControl => {
                    Err(CardError::EmblemOnly("a restriction on your creatures"))
                }
            }
        }
        AbilityDef::Static(
            StaticAbility::Anthem { affects, .. } | StaticAbility::GrantKeyword { affects, .. },
        ) => {
            if *affects == Affects::Enchanted && !card.is_aura() {
                return Err(CardError::EnchantedOnNonAura);
            }
            Ok(())
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

/// Checks a triggered ability: its trigger, and that its effects only refer
/// to what the trigger provides.
pub(crate) fn validate_triggered(trigger: &Trigger, effects: &[Effect]) -> Result<(), CardError> {
    if effects.is_empty() {
        return Err(CardError::NoEffects);
    }
    // Something other than the ability's own permanent sets these off.
    let has_subject = matches!(
        trigger,
        Trigger::EntersBattlefield {
            subject: TriggerSubject::CreatureYouControl
        } | Trigger::Dies {
            subject: TriggerSubject::CreatureYouControl
        }
    );
    let uses_triggering = effects
        .iter()
        .flat_map(Effect::targets)
        .any(|spec| spec == TargetSpec::Triggering {});
    if uses_triggering && !has_subject {
        return Err(CardError::NothingTriggering);
    }
    validate_effects(effects)
}

/// Checks the effects of one spell or ability, in order.
pub(crate) fn validate_effects(effects: &[Effect]) -> Result<(), CardError> {
    let mut has_target = false;
    for effect in effects {
        for spec in effect.targets() {
            match spec {
                TargetSpec::Same {} if !has_target => return Err(CardError::SameWithoutTarget),
                TargetSpec::Same {} => {}
                _ => has_target = true,
            }
        }
        validate_effect(effect)?;
    }
    Ok(())
}

pub(crate) fn validate_effect(effect: &Effect) -> Result<(), CardError> {
    let creature = |target: &TargetSpec| matches!(target, TargetSpec::Creature(_));
    // A creature to choose, or one already known.
    let creature_or_same =
        |target: &TargetSpec| matches!(target, TargetSpec::Creature(_)) || !target.is_chosen();
    let bad_target = match effect {
        Effect::CounterSpell { target } => {
            (!matches!(target, TargetSpec::Spell(_))).then_some(target)
        }
        Effect::DealDamage { target, .. } => (!matches!(
            target,
            TargetSpec::AnyTarget {}
                | TargetSpec::Creature(_)
                | TargetSpec::Opponent {}
                | TargetSpec::Player {}
        ))
        .then_some(target),
        Effect::Destroy { target }
        | Effect::GrantKeywordUntilEot { target, .. }
        | Effect::ModifyPtUntilEot { target, .. }
        | Effect::ReturnToHand { target }
        | Effect::Tap { target, .. } => (!creature_or_same(target)).then_some(target),
        Effect::DealPowerDamage { source, target } => {
            if creature(source) {
                (!creature(target)).then_some(target)
            } else {
                Some(source)
            }
        }
        Effect::CreateToken { .. }
        | Effect::Discard { .. }
        | Effect::DrawCards { .. }
        | Effect::GainLife { .. }
        | Effect::LoseLife { .. }
        | Effect::Scry { .. } => None,
    };
    if let Some(target) = bad_target {
        return Err(CardError::BadTarget {
            effect: effect.name(),
            target: target.name(),
        });
    }
    match effect {
        Effect::DealDamage { amount: 0, .. }
        | Effect::Discard { amount: 0, .. }
        | Effect::DrawCards { amount: 0, .. }
        | Effect::GainLife { amount: 0, .. }
        | Effect::LoseLife { amount: 0, .. }
        | Effect::Scry { amount: 0 } => Err(CardError::ZeroAmount(effect.name())),
        Effect::CreateToken { count: 0, .. } => Err(CardError::NoTokens),
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
    fn triggered_abilities_may_have_targets() {
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
            Ok(())
        );
    }

    #[test]
    fn same_reuses_the_target_before_it_and_so_needs_one() {
        let spell = |effects: Value| {
            with(
                shock(),
                "abilities",
                json!([{ "kind": "spell", "effects": effects }]),
            )
        };
        let grant = json!({
            "type": "grant_keyword_until_eot", "target": { "same": {} }, "keyword": "first_strike"
        });
        let pump = json!({
            "type": "modify_pt_until_eot", "target": { "creature": {} }, "power": 3, "toughness": 0
        });
        assert_eq!(check(spell(json!([pump, grant]))), Ok(()));
        assert_eq!(
            check(spell(json!([grant, pump]))),
            Err(CardError::SameWithoutTarget)
        );
        // Only effects on a creature can reuse a target.
        assert_eq!(
            check(spell(json!([
                pump,
                { "type": "deal_damage", "amount": 1, "target": { "same": {} } }
            ]))),
            Err(CardError::BadTarget {
                effect: "deal_damage",
                target: "same"
            })
        );
    }

    #[test]
    fn this_and_triggering_are_for_triggers_that_provide_them() {
        let trigger = |trigger: Value, target: &str| {
            with(
                bears(),
                "abilities",
                json!([{
                    "kind": "triggered",
                    "trigger": trigger,
                    "effects": [{
                        "type": "modify_pt_until_eot", "target": { target: {} },
                        "power": 1, "toughness": 1
                    }]
                }]),
            )
        };
        let cast = json!({ "type": "cast_spell", "kind": "noncreature" });
        let dies = json!({ "type": "dies", "subject": "creature_you_control" });
        assert_eq!(check(trigger(cast.clone(), "this")), Ok(()));
        assert_eq!(check(trigger(dies, "triggering")), Ok(()));
        // Nothing but the permanent itself sets a "you cast" ability off.
        assert_eq!(
            check(trigger(cast, "triggering")),
            Err(CardError::NothingTriggering)
        );
        // A spell is not a permanent and nothing triggers it.
        let spell = with(
            shock(),
            "abilities",
            json!([{ "kind": "spell", "effects": [{
                "type": "modify_pt_until_eot", "target": { "this": {} }, "power": 1, "toughness": 1
            }] }]),
        );
        assert_eq!(check(spell), Err(CardError::NotOnASpell("this")));
    }

    #[test]
    fn anthems_on_cards_affect_your_creatures_or_an_auras_creature() {
        let anthem = |affects: &str| {
            with(
                bears(),
                "abilities",
                json!([{ "kind": "static", "type": "anthem", "affects": affects,
                    "power": 1, "toughness": 1 }]),
            )
        };
        assert_eq!(check(anthem("creatures_you_control")), Ok(()));
        assert_eq!(check(anthem("other_creatures_you_control")), Ok(()));
        assert_eq!(
            check(anthem("enchanted")),
            Err(CardError::EnchantedOnNonAura)
        );
    }

    #[test]
    fn effects_name_the_targets_they_need_in_order() {
        let effect = |value: Value| serde_json::from_value::<Effect>(value).unwrap();
        let bite = effect(json!({
            "type": "deal_power_damage",
            "source": { "creature": { "controller": "you" } },
            "target": { "creature": { "controller": "opponent" } }
        }));
        assert_eq!(bite.targets().len(), 2);
        let rot = effect(json!({ "type": "discard", "player": "target_player", "amount": 2 }));
        assert_eq!(rot.targets(), [TargetSpec::Player {}]);
        let drain =
            effect(json!({ "type": "lose_life", "player": "target_opponent", "amount": 1 }));
        assert_eq!(drain.targets(), [TargetSpec::Opponent {}]);
        let rat = effect(json!({ "type": "discard", "player": "opponent", "amount": 1 }));
        assert_eq!(rat.targets(), []);
        assert_eq!(effect(json!({ "type": "scry", "amount": 1 })).targets(), []);
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
    fn only_lands_enter_tapped_and_a_land_may_tap_for_two_colours() {
        let gate = with(
            with(forest(), "enters_tapped", json!(true)),
            "abilities",
            json!([
                { "kind": "mana", "cost": { "tap": true }, "produces": "{G}" },
                { "kind": "mana", "cost": { "tap": true }, "produces": "{W}" }
            ]),
        );
        assert_eq!(check(gate.clone()), Ok(()));
        let def: CardDef = serde_json::from_value(gate).unwrap();
        assert_eq!(def.mana_colors(), [Color::G, Color::W]);
        assert_eq!(
            check(with(bears(), "enters_tapped", json!(true))),
            Err(CardError::EntersTapped)
        );
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
