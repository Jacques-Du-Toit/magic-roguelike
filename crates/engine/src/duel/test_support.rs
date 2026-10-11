//! Helpers shared by the duel tests.

use super::action::{Action, GameEvent};
use super::config::DuelConfig;
use super::ids::{CardId, ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::{apply_action, new_duel};
use crate::data;
use crate::decks::DeckId;

/// A default duel between two of the decks in `data/decks/`.
pub(crate) fn config(p1_deck: &str, p2_deck: &str) -> DuelConfig {
    let data = data::embedded().unwrap();
    let deck = |id: &str| &data.decks[&DeckId(id.to_string())];
    DuelConfig::from_decks(deck(p1_deck), deck(p2_deck), &data.balance.duel)
}

/// A duel in which both players have kept their opening hands, with the
/// starting player and their opponent.
pub(crate) fn started(p1_deck: &str, p2_deck: &str, seed: u64) -> (DuelState, PlayerId, PlayerId) {
    let mut state = new_duel(config(p1_deck, p2_deck), seed).unwrap();
    let first = state.starting_player;
    let second = first.opponent();
    apply_action(&mut state, first, Action::Keep).unwrap();
    apply_action(&mut state, second, Action::Keep).unwrap();
    (state, first, second)
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

/// The first land in the player's hand.
pub(crate) fn hand_land(state: &DuelState, player: PlayerId) -> ObjectId {
    *state
        .player(player)
        .hand
        .iter()
        .find(|id| state.def(**id).unwrap().is_land())
        .expect("no land in hand")
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

/// The player with priority passes, up to twice. Stops early if the game
/// stops waiting on a priority decision.
pub(crate) fn pass_both(state: &mut DuelState) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..2 {
        let Some(Decision::Priority { player }) = state.decision else {
            break;
        };
        events.extend(apply_action(state, player, Action::PassPriority).unwrap());
    }
    events
}

/// Passes priority until the given step of the given turn begins.
pub(crate) fn pass_until(state: &mut DuelState, turn: u32, step: Step) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..10_000 {
        if (state.turn, state.step) == (turn, step) {
            return events;
        }
        match state.decision {
            Some(Decision::Priority { player }) => {
                events.extend(apply_action(state, player, Action::PassPriority).unwrap());
            }
            other => panic!(
                "can't pass at turn {} {:?}: waiting on {other:?}",
                state.turn, state.step
            ),
        }
    }
    panic!("never reached turn {turn} {step:?}");
}

/// Plays the game to its end with both players doing nothing: they pass
/// priority and discard their first card when they must. Returns the events
/// and the number of actions taken.
pub(crate) fn play_out_passively(state: &mut DuelState) -> (Vec<GameEvent>, usize) {
    let mut events = Vec::new();
    for actions in 0..100_000 {
        let (player, action) = match state.decision {
            None => return (events, actions),
            Some(Decision::Priority { player }) => (player, Action::PassPriority),
            Some(Decision::Discard { player, .. }) => {
                let card = state.player(player).hand[0];
                (player, Action::Discard { card })
            }
            Some(other) => panic!("unexpected decision {other:?}"),
        };
        events.extend(apply_action(state, player, action).unwrap());
    }
    panic!("the game did not end");
}
