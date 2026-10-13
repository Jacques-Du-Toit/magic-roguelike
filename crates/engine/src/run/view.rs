//! What the UI shows of a run. A run has no hidden information except what
//! the seed will bring, so the view is the state without its RNG and its
//! duel, plus what the engine works out for the player: their basic lands
//! and their legal actions.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::balance::FeatReward;
use crate::cards::CardDefId;
use crate::data;
use crate::decks::DeckEntry;
use crate::duel::CardInstance;

use super::defs::{EnemyDef, EventDef, ModifierDef, ModifierId, PlaneswalkerDef, PlaneswalkerId};
use super::feats::{FeatDef, FeatScore};
use super::flow::{feat_scores, run_legal_actions};
use super::lands::basic_lands;
use super::state::{NodeId, Phase, RunAction, RunError, RunMap, RunState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunView {
    /// A `u64` in decimal.
    pub seed: String,
    pub planeswalker: PlaneswalkerId,
    /// The starting emblem the run was begun with, if any.
    pub emblem: Option<ModifierId>,
    /// The wounds carried, oldest first.
    pub wounds: Vec<ModifierId>,
    /// The wound that would end the run.
    pub max_wounds: u8,
    pub gold: u32,
    /// The spells and non-basic lands. `RunAction::RemoveCard` indexes it.
    pub deck: Vec<CardInstance>,
    /// The basic lands that go with the deck right now.
    pub basic_lands: Vec<DeckEntry>,
    pub relics: Vec<ModifierId>,
    /// Modifiers from events, for the next duel only.
    pub next_duel: Vec<ModifierId>,
    pub map: RunMap,
    /// The nodes visited, in order. The last is where the player is.
    pub path: Vec<NodeId>,
    pub phase: Phase,
    /// The feats picked for the duel in progress, and what each would
    /// score if it were won now.
    pub feats: Vec<FeatScore>,
    /// What the player may do now. Empty only when the run is over.
    pub actions: Vec<RunAction>,
}

/// Builds the view of the run.
pub fn run_view(state: &RunState) -> Result<RunView, RunError> {
    let data = data::embedded()?;
    let colors = data
        .run
        .planeswalker(&state.planeswalker)
        .map(|planeswalker| planeswalker.colors.as_slice())
        .unwrap_or_default();
    let basic_lands = basic_lands(&state.deck, colors, &data.cards, &data.balance.run.lands)?
        .into_iter()
        .map(|(card, count)| DeckEntry { card, count })
        .collect();
    Ok(RunView {
        seed: state.seed.to_string(),
        planeswalker: state.planeswalker.clone(),
        emblem: state.emblem.clone(),
        wounds: state.wounds.clone(),
        max_wounds: data.balance.run.max_wounds,
        gold: state.gold,
        deck: state.deck.clone(),
        basic_lands,
        relics: state.relics.clone(),
        next_duel: state.next_duel.clone(),
        map: state.map.clone(),
        path: state.path.clone(),
        phase: state.phase.clone(),
        feats: feat_scores(state)?,
        actions: run_legal_actions(state),
    })
}

/// Everything a run can contain, for the UI to show names and texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunContent {
    pub planeswalkers: Vec<PlaneswalkerDef>,
    /// The starting emblems a run can be begun with.
    pub emblems: Vec<ModifierDef>,
    pub relics: Vec<ModifierDef>,
    pub wounds: Vec<ModifierDef>,
    pub enemies: Vec<EnemyDef>,
    pub events: Vec<EventDef>,
    pub feats: Vec<FeatDef>,
    /// What a won duel pays for the points its feats scored, fewest first.
    pub feat_rewards: Vec<FeatReward>,
    /// The cards a card reward or a shop can offer.
    pub reward_cards: Vec<CardDefId>,
}

pub fn run_content() -> Result<RunContent, RunError> {
    let data = data::embedded()?;
    Ok(RunContent {
        planeswalkers: data.run.planeswalkers.clone(),
        emblems: data.run.emblems.clone(),
        relics: data.run.relics.clone(),
        wounds: data.run.wounds.clone(),
        enemies: data.run.enemies.clone(),
        events: data.run.events.clone(),
        feats: data.run.feats.clone(),
        feat_rewards: data.balance.run.feats.rewards.clone(),
        reward_cards: data
            .cards
            .iter()
            .filter(|card| card.supertypes.is_empty())
            .map(|card| card.id.clone())
            .collect(),
    })
}
