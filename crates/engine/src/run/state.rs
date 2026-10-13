//! The full state of a run, and the actions and events of the run layer.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::cards::CardDefId;
use crate::data::DataError;
use crate::duel::{
    ActionError, CardInstance, ConfigError, DuelRng, DuelState, InternalError, PlayerId,
};

use super::defs::{EnemyId, EventId, FeatId, ModifierId, PlaneswalkerId};
use super::feats::{DuelStats, FeatScore};

/// The human's seat in every duel of a run. The player takes the first turn
/// of each (`duel_config`).
pub const PLAYER: PlayerId = PlayerId::P1;
/// The enemy's seat.
pub const ENEMY: PlayerId = PlayerId::P2;

/// What a run is started from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunConfig {
    pub planeswalker: PlaneswalkerId,
    /// The starting emblem to take; `None` for the planeswalker's usual one,
    /// if it has one.
    pub emblem: Option<ModifierId>,
}

/// Identifies a node of the map: its position in `RunMap::nodes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Duel,
    Elite,
    Rest,
    Shop,
    Draft,
    Treasure,
    Event,
    Boss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MapNode {
    pub id: NodeId,
    /// 0 is the first row; the boss is alone in the last.
    pub row: u8,
    pub column: u8,
    pub kind: NodeKind,
    /// The nodes of the next row this one leads to.
    pub next: Vec<NodeId>,
}

/// One act's map: a layered graph. Every edge goes one row forward, and
/// every node is on some path from the first row to the boss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunMap {
    pub columns: u8,
    /// Rows including the boss's.
    pub rows: u8,
    /// In row order, then column order; a node's id is its index.
    pub nodes: Vec<MapNode>,
}

impl RunMap {
    pub fn node(&self, id: NodeId) -> Option<&MapNode> {
        self.nodes.get(id.0 as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DuelKind {
    Normal,
    Elite,
    Boss,
}

/// What the run is waiting on. `run_legal_actions` lists the ways to answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Phase {
    /// Choose the next node.
    Map,
    /// A duel is about to start: `enemy` is who it is against, and some of
    /// the `offered` feats are to be picked for it.
    Feats {
        enemy: EnemyId,
        kind: DuelKind,
        offered: Vec<FeatId>,
    },
    /// A duel is being played. `RunState::duel` holds it.
    Duel {
        enemy: EnemyId,
        kind: DuelKind,
    },
    /// Pick one of the cards, or none.
    Reward {
        choices: Vec<CardDefId>,
    },
    /// At a rest site: one thing may be done, heal every wound or upgrade a
    /// card.
    Rest,
    Shop {
        items: Vec<ShopItem>,
        removal_price: u32,
        removals_left: u8,
    },
    /// A pack is open: take up to `picks_left` more of the cards still in
    /// it. A card taken leaves `choices`.
    Draft {
        choices: Vec<CardDefId>,
        picks_left: u8,
    },
    /// A treasure is open: take one of `relics`, or `gold` instead.
    /// `relics` is empty when the player owns every relic.
    Treasure {
        relics: Vec<ModifierId>,
        gold: u32,
    },
    /// An event has been met: one of its options may be taken, or none.
    /// `relics` and `cards` are what it shows, for the options that give
    /// them.
    Event {
        event: EventId,
        relics: Vec<ModifierId>,
        cards: Vec<CardDefId>,
    },
    Over {
        won: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShopItem {
    pub ware: Ware,
    pub price: u32,
    pub sold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ware {
    /// A spell or a non-basic land.
    Card {
        card: CardDefId,
    },
    Relic {
        relic: ModifierId,
    },
}

/// Everything about a run in progress. Like `DuelState` it is plain data:
/// it serialises, it refers to definitions by id, and all its randomness
/// comes from `rng`, so the same seed and actions always give the same run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub seed: u64,
    pub planeswalker: PlaneswalkerId,
    /// The starting emblem the run was begun with, if any.
    pub emblem: Option<ModifierId>,
    /// The wounds carried, oldest first: one for each duel lost and not
    /// healed since. Each is a modifier the player takes into every duel.
    /// The run ends when there are `run.max_wounds` of them.
    pub wounds: Vec<ModifierId>,
    pub gold: u32,
    /// Spells and non-basic lands. The basic lands are worked out from it.
    pub deck: Vec<CardInstance>,
    pub relics: Vec<ModifierId>,
    pub map: RunMap,
    /// The nodes visited, in order. The last is where the player is.
    pub path: Vec<NodeId>,
    pub phase: Phase,
    /// The duel in progress, in `Phase::Duel`.
    pub duel: Option<DuelState>,
    /// What that duel has been like for the player so far, which is what
    /// feats are scored from.
    pub duel_stats: Option<DuelStats>,
    /// The feats picked for that duel. None for the boss.
    pub feats: Vec<FeatId>,
    /// Enemies already met, so the next one is someone new if possible.
    pub fought: Vec<EnemyId>,
    /// Events already met, likewise.
    pub events_met: Vec<EventId>,
    /// Modifiers from events that the player takes into their next duel
    /// and no further.
    pub next_duel: Vec<ModifierId>,
    pub rng: DuelRng,
}

impl RunState {
    /// The node the player is at, or `None` before the first move.
    pub fn position(&self) -> Option<NodeId> {
        self.path.last().copied()
    }
}

/// Something the player does between duels, or to end one. Every action
/// returned by `run_legal_actions` can be passed straight to
/// `apply_run_action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunAction {
    /// Move to a node and do what is there.
    Travel {
        node: NodeId,
    },
    /// Before a duel: pick these of the feats offered, by their places in
    /// `offered`, in rising order, and start the duel.
    ChooseFeats {
        feats: Vec<usize>,
    },
    /// Give up the duel in progress. It counts as a loss.
    Concede,
    /// Leave a duel that has ended and take its consequences.
    FinishDuel,
    /// Add `choices[choice]` of the reward to the deck.
    TakeReward {
        choice: usize,
    },
    SkipReward,
    /// At a rest site: heal every wound carried.
    HealWounds,
    /// At a rest site: upgrade `deck[card]`, which is from then on played
    /// as its card's upgraded side.
    UpgradeCard {
        card: usize,
    },
    /// At a shop: remove `deck[card]` from the deck.
    RemoveCard {
        card: usize,
    },
    /// Buy `items[item]` of the shop.
    Buy {
        item: usize,
    },
    /// At a draft: add `choices[choice]` of the pack to the deck.
    TakeDraft {
        choice: usize,
    },
    /// At a treasure: take `relics[choice]`.
    TakeTreasure {
        choice: usize,
    },
    /// At a treasure: take the gold instead of a relic.
    TakeGold,
    /// At an event: take `options[option]` of it. `pick` is the relic
    /// shown or the card of the deck it is about, for an option that needs
    /// one chosen.
    TakeEvent {
        option: usize,
        pick: Option<usize>,
    },
    /// Leave a rest site, a shop, a draft or an event.
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DuelOutcome {
    Won,
    Lost,
    Drawn,
}

/// Why gold changed hands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GoldReason {
    DuelWon,
    DuelLost,
    DuelDrawn,
    Relic,
    Purchase,
    Removal,
    Treasure,
    Event,
    Feats,
}

/// Something that happened in the run, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    Travelled {
        node: NodeId,
        kind: NodeKind,
    },
    /// A duel is ahead, with feats to pick for it.
    FeatsOffered {
        enemy: EnemyId,
        kind: DuelKind,
    },
    DuelStarted {
        enemy: EnemyId,
        kind: DuelKind,
    },
    /// A duel was won: how its feats scored, and their points in all.
    FeatsScored {
        scores: Vec<FeatScore>,
        points: u32,
    },
    DuelEnded {
        enemy: EnemyId,
        kind: DuelKind,
        outcome: DuelOutcome,
    },
    /// The player now carries this wound as well.
    WoundTaken {
        wound: ModifierId,
    },
    WoundHealed {
        wound: ModifierId,
    },
    /// `gold` is the new total.
    GoldChanged {
        #[ts(type = "number")]
        change: i64,
        gold: u32,
        reason: GoldReason,
    },
    RelicGained {
        relic: ModifierId,
    },
    CardAdded {
        card: CardDefId,
    },
    CardRemoved {
        card: CardDefId,
    },
    /// A copy of `card` in the deck was upgraded.
    CardUpgraded {
        card: CardDefId,
    },
    /// An event has left the player a modifier for their next duel.
    NextDuelModified {
        modifier: ModifierId,
    },
    RunEnded {
        won: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunError {
    #[error("there is no planeswalker `{0}`")]
    UnknownPlaneswalker(PlaneswalkerId),
    #[error("there is no starting emblem `{0}`")]
    UnknownEmblem(ModifierId),
    #[error("{0:?} is not a legal action in the run right now")]
    IllegalAction(RunAction),
    #[error("no duel is being played")]
    NoDuel,
    #[error(transparent)]
    Duel(#[from] ActionError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Data(#[from] DataError),
    #[error(transparent)]
    Internal(#[from] InternalError),
}
