//! Helpers shared by the duel tests.

use super::action::{Action, GameEvent, Target};
use super::config::DuelConfig;
use super::ids::{CardId, ObjectId, PlayerId};
use super::rng::DuelRng;
use super::state::{Decision, DuelState, Step, Zone};
use super::{apply_action, legal_actions, new_duel};
use crate::cards::CardDefId;
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

/// A started duel at the first main phase of turn 1, with the active player
/// and their opponent.
pub(crate) fn in_main_phase(
    p1_deck: &str,
    p2_deck: &str,
    seed: u64,
) -> (DuelState, PlayerId, PlayerId) {
    let (mut state, first, second) = started(p1_deck, p2_deck, seed);
    pass_until(&mut state, 1, Step::PrecombatMain);
    (state, first, second)
}

/// Adds a new copy of a card to the player's hand.
pub(crate) fn put_in_hand(state: &mut DuelState, player: PlayerId, card: &str) -> ObjectId {
    state.create_card(player, CardDefId(card.to_string()), Zone::Hand)
}

/// Adds a new copy of a card to the battlefield, as if it had been there
/// since before the turn began (so it isn't summoning sick).
pub(crate) fn put_on_battlefield(state: &mut DuelState, player: PlayerId, card: &str) -> ObjectId {
    let id = state.create_card(player, CardDefId(card.to_string()), Zone::Battlefield);
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

/// Puts the player's whole hand on the bottom of their library, so a test
/// controls exactly what they hold.
pub(crate) fn empty_hand(state: &mut DuelState, player: PlayerId) {
    for card in state.player(player).hand.clone() {
        state.move_object(card, Zone::Library).unwrap();
    }
}

/// Adds `count` untapped copies of a land to the battlefield.
pub(crate) fn put_lands(state: &mut DuelState, player: PlayerId, land: &str, count: usize) {
    for _ in 0..count {
        put_on_battlefield(state, player, land);
    }
}

/// The action that casts a spell with one target.
pub(crate) fn cast_at(card: ObjectId, target: Target) -> Action {
    Action::CastSpell {
        card,
        targets: vec![target],
    }
}

/// The action that casts a spell with no targets.
pub(crate) fn cast(card: ObjectId) -> Action {
    Action::CastSpell {
        card,
        targets: Vec::new(),
    }
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
            // Combat damage is divided the automatic way.
            Some(Decision::AssignCombatDamage { player, .. }) => {
                events.extend(apply_action(state, player, Action::ConfirmDamage).unwrap());
            }
            other => panic!(
                "can't pass at turn {} {:?}: waiting on {other:?}",
                state.turn, state.step
            ),
        }
    }
    panic!("never reached turn {turn} {step:?}");
}

/// Passes priority until the game waits on some other kind of decision, or
/// ends.
pub(crate) fn pass_until_other_decision(state: &mut DuelState) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..10_000 {
        let Some(Decision::Priority { player }) = state.decision else {
            return events;
        };
        events.extend(apply_action(state, player, Action::PassPriority).unwrap());
    }
    panic!("the game only ever asked for priority");
}

/// A duel at the first main phase of turn 1 with both hands empty, ready
/// for a test to put the cards it needs in place.
pub(crate) fn empty_board(seed: u64) -> (DuelState, PlayerId, PlayerId) {
    let (mut state, me, opponent) = in_main_phase("deck_a", "deck_c", seed);
    empty_hand(&mut state, me);
    empty_hand(&mut state, opponent);
    (state, me, opponent)
}

/// Passes to the declare-attackers decision and attacks with `attackers`.
pub(crate) fn attack(state: &mut DuelState, attackers: &[ObjectId]) -> Vec<GameEvent> {
    let mut events = pass_until_other_decision(state);
    let Some(Decision::DeclareAttackers { player }) = state.decision else {
        panic!("expected to declare attackers, got {:?}", state.decision);
    };
    for &creature in attackers {
        events.extend(apply_action(state, player, Action::Attack { creature }).unwrap());
    }
    events.extend(apply_action(state, player, Action::ConfirmAttackers).unwrap());
    events
}

/// Passes to the declare-blockers decision and makes the `(blocker,
/// attacker)` blocks.
pub(crate) fn block(state: &mut DuelState, blocks: &[(ObjectId, ObjectId)]) -> Vec<GameEvent> {
    let mut events = pass_until_other_decision(state);
    let Some(Decision::DeclareBlockers { player }) = state.decision else {
        panic!("expected to declare blockers, got {:?}", state.decision);
    };
    for &(blocker, attacker) in blocks {
        events.extend(apply_action(state, player, Action::Block { blocker, attacker }).unwrap());
    }
    events.extend(apply_action(state, player, Action::ConfirmBlockers).unwrap());
    events
}

/// Passes priority until the second main phase of the current turn.
pub(crate) fn finish_combat(state: &mut DuelState) -> Vec<GameEvent> {
    let turn = state.turn;
    pass_until(state, turn, Step::PostcombatMain)
}

/// Plays a duel to its end with both players picking uniformly at random
/// among their legal actions, using `choice_seed`. `check` sees the state
/// before every action. Returns the final state and the full event log.
pub(crate) fn play_randomly(
    config: DuelConfig,
    seed: u64,
    choice_seed: u64,
    mut check: impl FnMut(&DuelState, PlayerId, &[Action]),
) -> (DuelState, Vec<GameEvent>) {
    let mut state = new_duel(config, seed).unwrap();
    let mut choices = DuelRng::from_seed(choice_seed);
    let mut log = Vec::new();
    for _ in 0..200_000 {
        let Some(player) = state.waiting_on() else {
            return (state, log);
        };
        let actions = legal_actions(&state, player);
        assert!(
            !actions.is_empty(),
            "no legal action for {:?}",
            state.decision
        );
        check(&state, player, &actions);
        let pick = choices.below(u32::try_from(actions.len()).unwrap()) as usize;
        log.extend(apply_action(&mut state, player, actions[pick].clone()).unwrap());
    }
    panic!("the random game did not end");
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
