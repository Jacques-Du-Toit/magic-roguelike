//! Turn structure and priority.
//!
//! The functions here run the game forward until it needs a decision from a
//! player (or ends), recording what happened as events.

use super::action::{Action, GameEvent};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::setup::take_top_card;
use super::state::{Decision, DuelState, GameResult, Step, WinReason, Zone};

/// A rule of Magic rather than a balance number.
const LAND_DROPS_PER_TURN: u8 = 1;

/// Starts the first turn, once both players have kept their hands.
pub(crate) fn begin_game(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    let starting_player = state.starting_player;
    begin_turn(state, events, starting_player)
}

fn begin_turn(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
) -> Result<(), InternalError> {
    state.turn += 1;
    state.active_player = player;
    for player_state in &mut state.players {
        player_state.lands_played_this_turn = 0;
    }
    events.push(GameEvent::TurnBegan {
        turn: state.turn,
        player,
    });
    run_from_step(state, events, Step::Untap)
}

/// Begins `step`, and keeps going through later steps and turns until a
/// player has to decide something or the game ends.
fn run_from_step(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    step: Step,
) -> Result<(), InternalError> {
    let mut step = step;
    loop {
        state.step = step;
        events.push(GameEvent::StepBegan { step });
        let active = state.active_player;
        match step {
            Step::Untap => {
                let tapped: Vec<ObjectId> = state
                    .permanents_of(active)
                    .filter(|permanent| permanent.tapped)
                    .map(|permanent| permanent.id)
                    .collect();
                for id in tapped {
                    state.object_mut(id)?.tapped = false;
                }
                events.push(GameEvent::PermanentsUntapped { player: active });
            }
            Step::Draw => {
                // The starting player skips the draw of the first turn.
                if state.turn > 1 {
                    draw_card(state, events, active)?;
                }
                return open_priority(state, events);
            }
            Step::Upkeep | Step::PrecombatMain | Step::PostcombatMain | Step::End => {
                return open_priority(state, events);
            }
            Step::Cleanup => {
                let hand = state.player(active).hand.len();
                let excess = hand.saturating_sub(usize::from(state.rules.max_hand_size));
                if excess > 0 {
                    state.decision = Some(Decision::Discard {
                        player: active,
                        remaining: u8::try_from(excess).unwrap_or(u8::MAX),
                    });
                    return Ok(());
                }
            }
        }
        // No player gets priority in the untap and cleanup steps.
        match step.next() {
            Some(next) => step = next,
            None => return begin_turn(state, events, active.opponent()),
        }
    }
}

/// Checks state-based actions, then gives the active player priority unless
/// the game has ended.
fn open_priority(state: &mut DuelState, events: &mut Vec<GameEvent>) -> Result<(), InternalError> {
    check_state_based_actions(state, events);
    state.consecutive_passes = 0;
    state.decision = match state.result {
        Some(_) => None,
        None => Some(Decision::Priority {
            player: state.active_player,
        }),
    };
    Ok(())
}

/// A player at 0 life or less loses; if both are, the game is a draw.
fn check_state_based_actions(state: &mut DuelState, events: &mut Vec<GameEvent>) {
    if state.result.is_some() {
        return;
    }
    let lost = |player: PlayerId| state.player(player).life <= 0;
    let result = match (lost(PlayerId::P1), lost(PlayerId::P2)) {
        (false, false) => return,
        (true, true) => GameResult::Draw,
        (true, false) => win(state, PlayerId::P2),
        (false, true) => win(state, PlayerId::P1),
    };
    state.result = Some(result);
    events.push(GameEvent::GameOver { result });
}

fn win(state: &DuelState, winner: PlayerId) -> GameResult {
    let reason = if state.player(winner.opponent()).lost_to_fatigue {
        WinReason::Fatigue
    } else {
        WinReason::LifeZero
    };
    GameResult::Win {
        player: winner,
        reason,
    }
}

pub(crate) fn legal_priority_actions(
    state: &DuelState,
    player: PlayerId,
) -> Result<Vec<Action>, InternalError> {
    let mut actions = vec![Action::PassPriority];
    let player_state = state.player(player);
    let can_play_land = player == state.active_player
        && state.step.is_main()
        && player_state.lands_played_this_turn < LAND_DROPS_PER_TURN;
    if can_play_land {
        for &card in &player_state.hand {
            if state.def(card)?.is_land() {
                actions.push(Action::PlayLand { card });
            }
        }
    }
    Ok(actions)
}

pub(crate) fn legal_discard_actions(state: &DuelState, player: PlayerId) -> Vec<Action> {
    state
        .player(player)
        .hand
        .iter()
        .map(|&card| Action::Discard { card })
        .collect()
}

/// Both players passing in a row ends the step.
pub(crate) fn pass_priority(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
) -> Result<(), InternalError> {
    state.consecutive_passes += 1;
    if state.consecutive_passes < 2 {
        state.decision = Some(Decision::Priority {
            player: player.opponent(),
        });
        return Ok(());
    }
    state.consecutive_passes = 0;
    match state.step.next() {
        Some(next) => run_from_step(state, events, next),
        None => Err(InternalError::new(
            "priority was passed in the cleanup step",
        )),
    }
}

pub(crate) fn play_land(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    card: ObjectId,
) -> Result<(), InternalError> {
    let land = state.move_object(card, Zone::Battlefield)?;
    state.player_mut(player).lands_played_this_turn += 1;
    events.push(GameEvent::LandPlayed {
        player,
        card: state.card_ref(land)?,
    });
    // The player keeps priority.
    state.consecutive_passes = 0;
    Ok(())
}

pub(crate) fn discard(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    card: ObjectId,
    remaining: u8,
) -> Result<(), InternalError> {
    let discarded = state.move_object(card, Zone::Graveyard)?;
    events.push(GameEvent::CardDiscarded {
        player,
        card: state.card_ref(discarded)?,
    });
    let next_player = state.active_player.opponent();
    match remaining.saturating_sub(1) {
        0 => begin_turn(state, events, next_player),
        remaining => {
            state.decision = Some(Decision::Discard { player, remaining });
            Ok(())
        }
    }
}

/// The player draws a card, or takes fatigue if their library is empty: 1
/// life the first time, then more each time (numbers from the rules config).
pub(crate) fn draw_card(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
) -> Result<(), InternalError> {
    if let Some(drawn) = take_top_card(state, player)? {
        events.push(drawn);
        return Ok(());
    }
    let fatigue = &state.rules.fatigue;
    let (first, increase) = (fatigue.first_life_loss, fatigue.increase);
    let player_state = state.player_mut(player);
    let life_lost = first.saturating_add(increase.saturating_mul(player_state.fatigue_draws));
    let life_lost = i32::try_from(life_lost).unwrap_or(i32::MAX);
    player_state.fatigue_draws += 1;
    events.push(GameEvent::Fatigue { player, life_lost });
    change_life(
        state,
        events,
        player,
        life_lost.saturating_neg(),
        LifeCause::Fatigue,
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifeCause {
    Fatigue,
    #[cfg_attr(not(test), expect(dead_code, reason = "damage arrives with combat"))]
    Other,
}

pub(crate) fn change_life(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    change: i32,
    cause: LifeCause,
) {
    if change == 0 {
        return;
    }
    let player_state = state.player_mut(player);
    let was_alive = player_state.life > 0;
    player_state.life = player_state.life.saturating_add(change);
    if player_state.life > 0 {
        player_state.lost_to_fatigue = false;
    } else if was_alive {
        player_state.lost_to_fatigue = cause == LifeCause::Fatigue;
    }
    events.push(GameEvent::LifeChanged {
        player,
        change,
        life: player_state.life,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duel::error::ActionError;
    use crate::duel::test_support::{
        config, hand_land, pass_both, pass_until, play_out_passively, started,
    };
    use crate::duel::{apply_action, legal_actions, new_duel, result};

    fn steps(events: &[GameEvent]) -> Vec<Step> {
        events
            .iter()
            .filter_map(|event| match event {
                GameEvent::StepBegan { step } => Some(*step),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_turn_starts_when_both_players_have_kept() {
        let mut state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
        let first = state.starting_player;
        apply_action(&mut state, first, Action::Keep).unwrap();
        let events = apply_action(&mut state, first.opponent(), Action::Keep).unwrap();

        assert_eq!(
            events[1..],
            [
                GameEvent::TurnBegan {
                    turn: 1,
                    player: first
                },
                GameEvent::StepBegan { step: Step::Untap },
                GameEvent::PermanentsUntapped { player: first },
                GameEvent::StepBegan { step: Step::Upkeep },
            ]
        );
        assert_eq!((state.turn, state.active_player), (1, first));
        assert_eq!(state.step, Step::Upkeep);
        assert_eq!(state.decision, Some(Decision::Priority { player: first }));
        assert_eq!(result(&state), None);
    }

    #[test]
    fn a_step_ends_when_both_players_pass_in_a_row() {
        let (mut state, first, second) = started("deck_a", "deck_c", 1);
        assert_eq!(legal_actions(&state, first), [Action::PassPriority]);
        assert_eq!(legal_actions(&state, second), []);

        let events = apply_action(&mut state, first, Action::PassPriority).unwrap();
        assert_eq!(events, []);
        assert_eq!(state.step, Step::Upkeep);
        assert_eq!(state.decision, Some(Decision::Priority { player: second }));
        assert_eq!(
            apply_action(&mut state, first, Action::PassPriority),
            Err(ActionError::NotWaitingOn(first))
        );

        let events = apply_action(&mut state, second, Action::PassPriority).unwrap();
        assert_eq!(steps(&events), [Step::Draw]);
        // The active player gets priority first in each step.
        assert_eq!(state.decision, Some(Decision::Priority { player: first }));
    }

    #[test]
    fn a_turn_runs_through_every_step_in_order() {
        let (mut state, _, second) = started("deck_a", "deck_c", 1);
        let mut events = Vec::new();
        // The starting player didn't draw, so nobody discards on turn 1.
        while state.turn == 1 {
            events.extend(pass_both(&mut state));
        }
        assert_eq!(
            steps(&events),
            [
                Step::Draw,
                Step::PrecombatMain,
                Step::PostcombatMain,
                Step::End,
                Step::Cleanup,
                Step::Untap,
                Step::Upkeep,
            ]
        );
        assert!(events.contains(&GameEvent::TurnBegan {
            turn: 2,
            player: second
        }));
        assert_eq!((state.turn, state.active_player), (2, second));
        assert_eq!(state.decision, Some(Decision::Priority { player: second }));
    }

    #[test]
    fn the_starting_player_skips_their_first_draw_only() {
        let (mut state, first, second) = started("deck_a", "deck_c", 1);
        pass_until(&mut state, 1, Step::PrecombatMain);
        assert_eq!(state.player(first).hand.len(), 7);

        pass_until(&mut state, 2, Step::Draw);
        assert_eq!(state.player(second).hand.len(), 8);
        assert_eq!(state.player(second).library.len(), 17);
        assert_eq!(state.player(first).hand.len(), 7);

        // Discard down at the end of turn 2, then the first player draws.
        pass_until(&mut state, 2, Step::Cleanup);
        let card = state.player(second).hand[0];
        apply_action(&mut state, second, Action::Discard { card }).unwrap();
        pass_until(&mut state, 3, Step::Draw);
        assert_eq!(state.player(first).hand.len(), 8);
    }

    #[test]
    fn drawing_reports_the_card() {
        let (mut state, _, second) = started("deck_a", "deck_c", 1);
        pass_until(&mut state, 2, Step::Upkeep);
        let top = state.objects[state.player(second).library.last().unwrap()].card;
        let events = pass_both(&mut state);
        let drawn = events.iter().find_map(|event| match event {
            GameEvent::CardDrawn { player, card } if *player == second => card.face.clone(),
            _ => None,
        });
        assert_eq!(drawn.unwrap().card, top);
    }

    #[test]
    fn a_land_can_be_played_in_your_own_main_phase() {
        let (mut state, first, second) = started("deck_c", "deck_c", 3);
        let land = hand_land(&state, first);
        // Not in the upkeep.
        assert!(!legal_actions(&state, first).contains(&Action::PlayLand { card: land }));

        pass_until(&mut state, 1, Step::PrecombatMain);
        let lands_in_hand = state
            .player(first)
            .hand
            .iter()
            .filter(|id| state.def(**id).unwrap().is_land())
            .count();
        let actions = legal_actions(&state, first);
        assert_eq!(actions.len(), 1 + lands_in_hand);
        assert!(actions.contains(&Action::PlayLand { card: land }));

        let card = state.objects[&land].card;
        let events = apply_action(&mut state, first, Action::PlayLand { card: land }).unwrap();
        let played = *state.battlefield.last().unwrap();
        assert_eq!(state.objects[&played].card, card);
        assert_eq!(state.objects[&played].zone, Zone::Battlefield);
        assert!(matches!(
            &events[..],
            [GameEvent::LandPlayed { player, card }] if *player == first && card.object == played
        ));
        assert_eq!(state.player(first).hand.len(), 6);
        // The player keeps priority, and the opponent can't play lands.
        assert_eq!(state.decision, Some(Decision::Priority { player: first }));
        apply_action(&mut state, first, Action::PassPriority).unwrap();
        assert_eq!(legal_actions(&state, second), [Action::PassPriority]);
        state.assert_consistent();
    }

    #[test]
    fn only_one_land_per_turn() {
        let (mut state, first, _) = started("deck_c", "deck_c", 3);
        pass_until(&mut state, 1, Step::PrecombatMain);
        let land = hand_land(&state, first);
        apply_action(&mut state, first, Action::PlayLand { card: land }).unwrap();
        assert_eq!(legal_actions(&state, first), [Action::PassPriority]);

        let another = hand_land(&state, first);
        assert_eq!(
            apply_action(&mut state, first, Action::PlayLand { card: another }),
            Err(ActionError::IllegalAction {
                player: first,
                action: Action::PlayLand { card: another }
            })
        );
        // Not in the second main phase of the same turn either.
        pass_until(&mut state, 1, Step::PostcombatMain);
        assert_eq!(legal_actions(&state, first), [Action::PassPriority]);

        // The land drop comes back on the player's next turn.
        pass_until(&mut state, 2, Step::Cleanup);
        let second = first.opponent();
        let card = state.player(second).hand[0];
        apply_action(&mut state, second, Action::Discard { card }).unwrap();
        pass_until(&mut state, 3, Step::PostcombatMain);
        assert!(legal_actions(&state, first).contains(&Action::PlayLand { card: another }));
    }

    #[test]
    fn a_spell_card_cannot_be_played_as_a_land() {
        let (mut state, first, _) = started("deck_c", "deck_c", 3);
        pass_until(&mut state, 1, Step::PrecombatMain);
        let spell = *state
            .player(first)
            .hand
            .iter()
            .find(|id| !state.def(**id).unwrap().is_land())
            .unwrap();
        assert!(apply_action(&mut state, first, Action::PlayLand { card: spell }).is_err());
    }

    #[test]
    fn the_active_player_discards_down_to_the_maximum_hand_size() {
        let (mut state, _, second) = started("deck_a", "deck_c", 1);
        pass_until(&mut state, 2, Step::Cleanup);
        assert_eq!(
            state.decision,
            Some(Decision::Discard {
                player: second,
                remaining: 1
            })
        );
        let hand = state.player(second).hand.clone();
        let expected: Vec<Action> = hand.iter().map(|&card| Action::Discard { card }).collect();
        assert_eq!(legal_actions(&state, second), expected);

        let card = state.objects[&hand[4]].card;
        let events = apply_action(&mut state, second, Action::Discard { card: hand[4] }).unwrap();
        let in_graveyard = state.player(second).graveyard[0];
        assert_eq!(state.objects[&in_graveyard].card, card);
        assert!(matches!(
            &events[0],
            GameEvent::CardDiscarded { player, card } if *player == second && card.object == in_graveyard
        ));
        assert_eq!(state.player(second).hand.len(), 7);
        assert_eq!(state.turn, 3);
        state.assert_consistent();
    }

    #[test]
    fn maximum_hand_size_comes_from_the_rules_config() {
        let mut small_hands = config("deck_a", "deck_c");
        small_hands.rules.max_hand_size = 5;
        let mut state = new_duel(small_hands, 1).unwrap();
        let first = state.starting_player;
        apply_action(&mut state, first, Action::Keep).unwrap();
        apply_action(&mut state, first.opponent(), Action::Keep).unwrap();
        pass_until(&mut state, 1, Step::Cleanup);
        for remaining in [2, 1] {
            assert_eq!(
                state.decision,
                Some(Decision::Discard {
                    player: first,
                    remaining
                })
            );
            let card = state.player(first).hand[0];
            apply_action(&mut state, first, Action::Discard { card }).unwrap();
        }
        assert_eq!(state.player(first).hand.len(), 5);
        assert_eq!(state.turn, 2);
    }

    #[test]
    fn drawing_from_an_empty_library_costs_more_life_each_time() {
        let (mut state, _, second) = started("deck_a", "deck_c", 1);
        let (events, _) = play_out_passively(&mut state);
        let losses: Vec<i32> = events
            .iter()
            .filter_map(|event| match event {
                GameEvent::Fatigue { player, life_lost } if *player == second => Some(*life_lost),
                _ => None,
            })
            .collect();
        // 1 + 2 + 3 + 4 + 5 leaves 5 life; the sixth draw is lethal.
        assert_eq!(losses, [1, 2, 3, 4, 5, 6]);
        assert_eq!(state.player(second).fatigue_draws, 6);
        assert_eq!(state.player(second).life, -1);
        assert!(events.contains(&GameEvent::LifeChanged {
            player: second,
            change: -3,
            life: 14
        }));
    }

    #[test]
    fn a_passive_game_ends_in_a_fatigue_win() {
        let (mut state, first, second) = started("deck_a", "deck_c", 1);
        let (events, actions) = play_out_passively(&mut state);
        // The player on the draw runs out of cards first.
        let expected = GameResult::Win {
            player: first,
            reason: WinReason::Fatigue,
        };
        assert_eq!(result(&state), Some(expected));
        assert_eq!(
            events.last(),
            Some(&GameEvent::GameOver { result: expected })
        );
        assert!(actions > 100);
        assert_eq!(state.decision, None);
        for player in [first, second] {
            assert_eq!(legal_actions(&state, player), []);
            assert_eq!(
                apply_action(&mut state, player, Action::PassPriority),
                Err(ActionError::NotWaitingOn(player))
            );
        }
        state.assert_consistent();
    }

    #[test]
    fn fatigue_numbers_come_from_the_rules_config() {
        let mut harsh = config("deck_a", "deck_c");
        harsh.rules.fatigue.first_life_loss = 2;
        harsh.rules.fatigue.increase = 3;
        let mut state = new_duel(harsh, 1).unwrap();
        let first = state.starting_player;
        apply_action(&mut state, first, Action::Keep).unwrap();
        apply_action(&mut state, first.opponent(), Action::Keep).unwrap();
        let (events, _) = play_out_passively(&mut state);
        let losses: Vec<i32> = events
            .iter()
            .filter_map(|event| match event {
                GameEvent::Fatigue { life_lost, .. } => Some(*life_lost),
                _ => None,
            })
            .collect();
        // 2 + 5 + 8 = 15, then 11 is lethal; the other player took 2 + 5 + 8.
        assert_eq!(losses, [2, 2, 5, 5, 8, 8, 11]);
    }

    #[test]
    fn a_player_at_zero_life_loses_when_priority_would_be_given() {
        let (mut state, first, second) = started("deck_a", "deck_c", 1);
        let mut events = Vec::new();
        change_life(&mut state, &mut events, second, -20, LifeCause::Other);
        let events = pass_both(&mut state);
        let expected = GameResult::Win {
            player: first,
            reason: WinReason::LifeZero,
        };
        assert_eq!(result(&state), Some(expected));
        assert_eq!(
            events.last(),
            Some(&GameEvent::GameOver { result: expected })
        );
    }

    #[test]
    fn both_players_at_zero_life_is_a_draw() {
        let (mut state, first, second) = started("deck_a", "deck_c", 1);
        let mut events = Vec::new();
        change_life(&mut state, &mut events, first, -25, LifeCause::Other);
        change_life(&mut state, &mut events, second, -20, LifeCause::Other);
        pass_both(&mut state);
        assert_eq!(result(&state), Some(GameResult::Draw));
        assert_eq!(state.decision, None);
    }

    #[test]
    fn regaining_life_before_the_check_clears_a_fatigue_loss() {
        let (mut state, first, _) = started("deck_a", "deck_c", 1);
        let mut events = Vec::new();
        change_life(&mut state, &mut events, first, -20, LifeCause::Fatigue);
        assert!(state.player(first).lost_to_fatigue);
        change_life(&mut state, &mut events, first, 5, LifeCause::Other);
        assert!(!state.player(first).lost_to_fatigue);
        change_life(&mut state, &mut events, first, -5, LifeCause::Other);
        assert!(!state.player(first).lost_to_fatigue);
    }

    #[test]
    fn same_seed_and_actions_give_the_same_game() {
        let play = |seed: u64| {
            let (mut state, _, _) = started("deck_b", "deck_c", seed);
            let (events, _) = play_out_passively(&mut state);
            (events, state)
        };
        assert_eq!(play(7), play(7));
        assert_ne!(play(7).0, play(8).0);
    }
}
