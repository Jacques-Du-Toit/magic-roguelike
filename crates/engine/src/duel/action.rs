//! Player actions and the events they produce.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::{CardId, ObjectId, PlayerId};
use super::state::{GameResult, Step};
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
    BottomCard {
        card: ObjectId,
    },
    PassPriority,
    PlayLand {
        card: ObjectId,
    },
    /// Discard a card at cleanup, down to the maximum hand size.
    Discard {
        card: ObjectId,
    },
    /// Cast a card from hand. The engine pays the mana cost by tapping
    /// sources itself; the action is only legal if it can.
    CastSpell {
        card: ObjectId,
        targets: Vec<Target>,
    },
    /// Add a creature to the attack being declared.
    Attack {
        creature: ObjectId,
    },
    /// Add a block to the blocks being declared.
    Block {
        blocker: ObjectId,
        attacker: ObjectId,
    },
    /// Take a creature back out of the attack being declared.
    RemoveAttacker {
        creature: ObjectId,
    },
    /// Take a creature back out of the blocks being declared.
    RemoveBlocker {
        blocker: ObjectId,
    },
    /// Start the attack or block declaration again.
    ResetDeclaration,
    /// Attack with the creatures chosen so far, which may be none.
    ConfirmAttackers,
    /// Block as chosen so far. Not legal while the blocks break a rule,
    /// e.g. a creature with menace has exactly one blocker.
    ConfirmBlockers,
    /// While dividing an attacker's combat damage: one more damage to this
    /// blocker, or to the defending player for a trampler.
    AssignDamage {
        target: Target,
    },
    /// One damage less to this recipient.
    UnassignDamage {
        target: Target,
    },
    /// Deal the attacker's damage as divided. Not legal until all of it is
    /// assigned, and none may go to the player while a blocker has less
    /// than lethal damage.
    ConfirmDamage,
}

/// What a spell or ability is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    Player {
        player: PlayerId,
    },
    /// A permanent, or a spell on the stack.
    Object {
        object: ObjectId,
    },
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
        card: CardRef,
    },
    CardPutOnBottom {
        player: PlayerId,
        card: CardRef,
    },
    TurnBegan {
        turn: u32,
        player: PlayerId,
    },
    StepBegan {
        step: Step,
    },
    /// The player's permanents untapped during their untap step.
    PermanentsUntapped {
        player: PlayerId,
    },
    LandPlayed {
        player: PlayerId,
        card: CardRef,
    },
    CardDiscarded {
        player: PlayerId,
        card: CardRef,
    },
    /// The player drew from an empty library.
    Fatigue {
        player: PlayerId,
        life_lost: i32,
    },
    /// `life` is the new total.
    LifeChanged {
        player: PlayerId,
        change: i32,
        life: i32,
    },
    GameOver {
        result: GameResult,
    },
    /// A permanent tapped, e.g. for mana.
    PermanentTapped {
        permanent: ObjectId,
    },
    /// A card was cast and is now on the stack.
    SpellCast {
        player: PlayerId,
        card: CardRef,
        targets: Vec<Target>,
    },
    /// A permanent spell resolved, or a permanent arrived some other way.
    PermanentEntered {
        player: PlayerId,
        card: CardRef,
    },
    /// An instant or sorcery resolved and went to the graveyard.
    SpellResolved {
        player: PlayerId,
        card: CardRef,
    },
    /// A spell left the stack without effect because all its targets had
    /// become illegal.
    SpellFizzled {
        player: PlayerId,
        card: CardRef,
    },
    /// A spell was countered. `player` is the spell's controller.
    SpellCountered {
        player: PlayerId,
        card: CardRef,
    },
    /// A triggered ability of `source` went on the stack as `ability`.
    AbilityTriggered {
        player: PlayerId,
        source: ObjectId,
        ability: ObjectId,
    },
    AbilityResolved {
        ability: ObjectId,
    },
    /// A creature got +`power`/+`toughness` until end of turn.
    PowerToughnessChanged {
        permanent: ObjectId,
        power: i32,
        toughness: i32,
    },
    /// An Aura entered the battlefield attached to `enchanted`.
    AuraAttached {
        aura: ObjectId,
        enchanted: ObjectId,
    },
    /// A permanent that isn't a creature went from the battlefield to its
    /// owner's graveyard, e.g. an Aura whose creature is gone.
    PermanentPutIntoGraveyard {
        player: PlayerId,
        permanent: ObjectId,
        card: CardRef,
    },
    AttackersDeclared {
        player: PlayerId,
        attackers: Vec<ObjectId>,
    },
    /// One entry per blocked attacker.
    BlockersDeclared {
        player: PlayerId,
        blocks: Vec<BlockDeclared>,
    },
    DamageDealt {
        source: ObjectId,
        target: Target,
        amount: i32,
    },
    /// A creature went from the battlefield to its owner's graveyard.
    /// `permanent` is what it was; `card` is the card in the graveyard.
    CreatureDied {
        player: PlayerId,
        permanent: ObjectId,
        card: CardRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BlockDeclared {
    pub attacker: ObjectId,
    pub blockers: Vec<ObjectId>,
}

/// A card as an event or a view shows it. `face` is `None` when the viewer
/// can't see which card it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardRef {
    pub object: ObjectId,
    pub face: Option<CardFace>,
}

/// Which card an object is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardFace {
    pub card: CardId,
    pub def: CardDefId,
}
