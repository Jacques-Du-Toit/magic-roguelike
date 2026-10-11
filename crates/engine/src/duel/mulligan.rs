//! The London mulligan.
//!
//! The starting player decides first. A player who mulligans shuffles their
//! hand away and is dealt a fresh, smoothed hand; players who haven't kept
//! take turns deciding until both have. A player who keeps then puts one
//! card on the bottom of their library per mulligan they took.

use super::action::{Action, GameEvent};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::setup::deal_opening_hand;
use super::state::{Decision, DuelState, Zone};

pub(crate) fn legal_mulligan_actions(state: &DuelState, player: PlayerId) -> Vec<Action> {
    let player_state = state.player(player);
    let mut actions = vec![Action::Keep];
    if player_state.mulligans_taken < player_state.opening_hand_size {
        actions.push(Action::Mulligan);
    }
    actions
}

pub(crate) fn legal_bottom_actions(state: &DuelState, player: PlayerId) -> Vec<Action> {
    state
        .player(player)
        .hand
        .iter()
        .map(|&card| Action::BottomCard { card })
        .collect()
}

pub(crate) fn take_mulligan(
    state: &mut DuelState,
    player: PlayerId,
) -> Result<Vec<GameEvent>, InternalError> {
    for card in state.player(player).hand.clone() {
        state.move_object(card, Zone::Library)?;
    }
    let player_state = state.player_mut(player);
    player_state.mulligans_taken += 1;
    let mut events = vec![GameEvent::MulliganTaken {
        player,
        mulligans_taken: player_state.mulligans_taken,
    }];
    events.extend(deal_opening_hand(state, player)?);
    state.decision = Some(next_mulligan_decision(state, player));
    Ok(events)
}

pub(crate) fn keep_hand(
    state: &mut DuelState,
    player: PlayerId,
) -> Result<Vec<GameEvent>, InternalError> {
    let player_state = state.player_mut(player);
    player_state.kept_hand = true;
    let hand_size = u8::try_from(player_state.hand.len()).unwrap_or(u8::MAX);
    let owed = player_state.mulligans_taken.min(hand_size);
    state.decision = if owed > 0 {
        Some(Decision::BottomCards {
            player,
            remaining: owed,
        })
    } else {
        after_keeping(state, player)
    };
    Ok(vec![GameEvent::HandKept { player }])
}

pub(crate) fn bottom_card(
    state: &mut DuelState,
    player: PlayerId,
    card: ObjectId,
    remaining: u8,
) -> Result<Vec<GameEvent>, InternalError> {
    let bottomed = state.move_object(card, Zone::Library)?;
    let event = GameEvent::CardPutOnBottom {
        player,
        card: state.event_card(bottomed)?,
    };
    state.decision = match remaining.saturating_sub(1) {
        0 => after_keeping(state, player),
        remaining => Some(Decision::BottomCards { player, remaining }),
    };
    Ok(vec![event])
}

/// Who decides next after `player` took a mulligan: the opponent if they
/// haven't kept, otherwise `player` again.
fn next_mulligan_decision(state: &DuelState, player: PlayerId) -> Decision {
    let opponent = player.opponent();
    if state.player(opponent).kept_hand {
        Decision::Mulligan { player }
    } else {
        Decision::Mulligan { player: opponent }
    }
}

/// What the game waits on once `player` has kept and finished bottoming.
fn after_keeping(state: &DuelState, player: PlayerId) -> Option<Decision> {
    let opponent = player.opponent();
    if state.player(opponent).kept_hand {
        // Both players have kept. The first turn starts here once turns
        // exist (M1 slice 3).
        None
    } else {
        Some(Decision::Mulligan { player: opponent })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duel::error::ActionError;
    use crate::duel::test_support::{config, hand_cards};
    use crate::duel::{apply_action, legal_actions, new_duel};

    fn duel(seed: u64) -> (DuelState, PlayerId, PlayerId) {
        let state = new_duel(config("deck_a", "deck_b"), seed).unwrap();
        let first = state.starting_player;
        (state, first, first.opponent())
    }

    #[test]
    fn only_the_deciding_player_has_actions() {
        let (state, first, second) = duel(1);
        assert_eq!(
            legal_actions(&state, first),
            [Action::Keep, Action::Mulligan]
        );
        assert_eq!(legal_actions(&state, second), []);
    }

    #[test]
    fn both_keeping_ends_the_mulligans_with_full_hands() {
        let (mut state, first, second) = duel(1);
        let events = apply_action(&mut state, first, Action::Keep).unwrap();
        assert_eq!(events, [GameEvent::HandKept { player: first }]);
        assert_eq!(state.decision, Some(Decision::Mulligan { player: second }));

        apply_action(&mut state, second, Action::Keep).unwrap();
        assert_eq!(state.decision, None);
        assert_eq!(state.player(first).hand.len(), 7);
        assert_eq!(state.player(second).hand.len(), 7);
        state.assert_consistent();
    }

    #[test]
    fn a_mulligan_deals_a_fresh_hand_of_the_same_size() {
        let (mut state, first, second) = duel(2);
        let before = hand_cards(&state, first);
        let events = apply_action(&mut state, first, Action::Mulligan).unwrap();

        assert_eq!(
            events[0],
            GameEvent::MulliganTaken {
                player: first,
                mulligans_taken: 1
            }
        );
        let drawn = events
            .iter()
            .filter(
                |event| matches!(event, GameEvent::CardDrawn { player, .. } if *player == first),
            )
            .count();
        assert_eq!((events.len(), drawn), (8, 7));

        assert_eq!(state.player(first).hand.len(), 7);
        assert_eq!(state.player(first).library.len(), 18);
        assert_eq!(state.player(first).mulligans_taken, 1);
        assert_ne!(hand_cards(&state, first), before);
        assert_eq!(state.decision, Some(Decision::Mulligan { player: second }));
        state.assert_consistent();
    }

    #[test]
    fn mulliganed_cards_come_back_as_new_objects() {
        let (mut state, first, _) = duel(2);
        let old_objects = state.player(first).hand.clone();
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        for old in old_objects {
            assert!(!state.objects.contains_key(&old));
        }
    }

    #[test]
    fn keeping_after_a_mulligan_means_bottoming_a_card() {
        let (mut state, first, second) = duel(3);
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        apply_action(&mut state, second, Action::Keep).unwrap();
        // The opponent has kept, so the decision returns to the first player.
        assert_eq!(state.decision, Some(Decision::Mulligan { player: first }));

        apply_action(&mut state, first, Action::Keep).unwrap();
        assert_eq!(
            state.decision,
            Some(Decision::BottomCards {
                player: first,
                remaining: 1
            })
        );
        let hand = state.player(first).hand.clone();
        let expected: Vec<Action> = hand
            .iter()
            .map(|&card| Action::BottomCard { card })
            .collect();
        assert_eq!(legal_actions(&state, first), expected);

        let chosen = hand[2];
        let chosen_card = state.objects[&chosen].card;
        let events = apply_action(&mut state, first, Action::BottomCard { card: chosen }).unwrap();

        let bottom = state.player(first).library[0];
        assert_eq!(state.objects[&bottom].card, chosen_card);
        assert!(matches!(
            &events[..],
            [GameEvent::CardPutOnBottom { player, card }]
                if *player == first && card.object == bottom
        ));
        assert_eq!(state.player(first).hand.len(), 6);
        assert_eq!(state.player(first).library.len(), 19);
        assert_eq!(state.decision, None);
        state.assert_consistent();
    }

    #[test]
    fn players_who_have_not_kept_take_turns_deciding() {
        let (mut state, first, second) = duel(4);
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        apply_action(&mut state, second, Action::Mulligan).unwrap();
        assert_eq!(state.decision, Some(Decision::Mulligan { player: first }));
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        assert_eq!(state.decision, Some(Decision::Mulligan { player: second }));

        // The second player keeps a six-card hand, the first a five-card one.
        apply_action(&mut state, second, Action::Keep).unwrap();
        let card = state.player(second).hand[0];
        apply_action(&mut state, second, Action::BottomCard { card }).unwrap();
        assert_eq!(state.decision, Some(Decision::Mulligan { player: first }));

        apply_action(&mut state, first, Action::Keep).unwrap();
        for remaining in [2, 1] {
            assert_eq!(
                state.decision,
                Some(Decision::BottomCards {
                    player: first,
                    remaining
                })
            );
            let card = state.player(first).hand[0];
            apply_action(&mut state, first, Action::BottomCard { card }).unwrap();
        }
        assert_eq!(state.decision, None);
        assert_eq!(state.player(first).hand.len(), 5);
        assert_eq!(state.player(second).hand.len(), 6);
        state.assert_consistent();
    }

    #[test]
    fn a_player_cannot_mulligan_more_times_than_their_hand_size() {
        let mut tiny = config("deck_a", "deck_b");
        for player in &mut tiny.players {
            player.opening_hand_size = 1;
        }
        let mut state = new_duel(tiny, 1).unwrap();
        let first = state.starting_player;
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        apply_action(&mut state, first.opponent(), Action::Keep).unwrap();

        assert_eq!(legal_actions(&state, first), [Action::Keep]);
        apply_action(&mut state, first, Action::Keep).unwrap();
        let card = state.player(first).hand[0];
        apply_action(&mut state, first, Action::BottomCard { card }).unwrap();
        assert!(state.player(first).hand.is_empty());
        assert_eq!(state.decision, None);
    }

    #[test]
    fn mulligan_hands_are_smoothed_too() {
        let total_distance = |candidates: u8| -> usize {
            (0..200)
                .map(|seed| {
                    let mut config = config("deck_a", "deck_b");
                    config.rules.smoothing_candidates = candidates;
                    let mut state = new_duel(config, seed).unwrap();
                    let first = state.starting_player;
                    apply_action(&mut state, first, Action::Mulligan).unwrap();
                    let lands = crate::duel::test_support::hand_lands(&state, first);
                    (lands * 25).abs_diff(70)
                })
                .sum()
        };
        let off = total_distance(1);
        let on = total_distance(3);
        assert!(on * 10 < off * 7, "3 candidates: {on}, 1 candidate: {off}");
    }

    #[test]
    fn illegal_actions_are_errors_and_change_nothing() {
        let (mut state, first, second) = duel(5);
        let before = state.clone();
        let card = state.player(first).hand[0];

        assert_eq!(
            apply_action(&mut state, second, Action::Keep),
            Err(ActionError::NotWaitingOn(second))
        );
        assert_eq!(
            apply_action(&mut state, first, Action::BottomCard { card }),
            Err(ActionError::IllegalAction {
                player: first,
                action: Action::BottomCard { card }
            })
        );
        assert_eq!(state, before);

        apply_action(&mut state, first, Action::Mulligan).unwrap();
        apply_action(&mut state, second, Action::Keep).unwrap();
        apply_action(&mut state, first, Action::Keep).unwrap();
        let before = state.clone();
        let opponents_card = state.player(second).hand[0];
        for action in [
            Action::Keep,
            Action::Mulligan,
            Action::BottomCard { card },
            Action::BottomCard {
                card: opponents_card,
            },
        ] {
            assert_eq!(
                apply_action(&mut state, first, action.clone()),
                Err(ActionError::IllegalAction {
                    player: first,
                    action
                })
            );
        }
        assert_eq!(state, before);
    }

    #[test]
    fn nobody_can_act_once_the_mulligans_are_over() {
        let (mut state, first, second) = duel(6);
        apply_action(&mut state, first, Action::Keep).unwrap();
        apply_action(&mut state, second, Action::Keep).unwrap();
        for player in PlayerId::ALL {
            assert_eq!(legal_actions(&state, player), []);
            assert_eq!(
                apply_action(&mut state, player, Action::Keep),
                Err(ActionError::NotWaitingOn(player))
            );
        }
    }

    /// Plays a fixed script of mulligan decisions and returns the event log.
    fn scripted_log(seed: u64) -> (Vec<GameEvent>, DuelState) {
        let (mut state, first, second) = duel(seed);
        let mut log = Vec::new();
        let script = [
            (first, Action::Mulligan),
            (second, Action::Mulligan),
            (first, Action::Keep),
        ];
        for (player, action) in script {
            log.extend(apply_action(&mut state, player, action).unwrap());
        }
        let card = state.player(first).hand[3];
        log.extend(apply_action(&mut state, first, Action::BottomCard { card }).unwrap());
        (log, state)
    }

    #[test]
    fn same_seed_and_actions_give_the_same_events_and_state() {
        assert_eq!(scripted_log(21), scripted_log(21));
        assert_ne!(scripted_log(21).0, scripted_log(22).0);
    }

    #[test]
    fn a_saved_and_restored_duel_continues_identically() {
        let (mut state, first, second) = duel(8);
        apply_action(&mut state, first, Action::Mulligan).unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let mut restored: DuelState = serde_json::from_str(&json).unwrap();

        let events = apply_action(&mut state, second, Action::Mulligan).unwrap();
        let restored_events = apply_action(&mut restored, second, Action::Mulligan).unwrap();
        assert_eq!(restored_events, events);
        assert_eq!(restored, state);
    }
}
