//! Tests for player views, event redaction, auto-pass, determinism, and
//! random play.

use super::action::{Action, CardRef, GameEvent, Target};
use super::ids::PlayerId;
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, cast, cast_at, config, empty_board, pass_both, pass_until, pass_until_other_decision,
    play_randomly, put_in_hand, put_lands, put_on_battlefield, started,
};
use super::view::{PlayerView, StackItemViewKind, player_view, redact_events, should_auto_pass};
use super::{apply_action, has_meaningful_action, legal_actions, new_duel};

const PAIRINGS: [(&str, &str); 6] = [
    ("deck_a", "deck_a"),
    ("deck_a", "deck_b"),
    ("deck_a", "deck_c"),
    ("deck_b", "deck_b"),
    ("deck_b", "deck_c"),
    ("deck_c", "deck_c"),
];

// ---- Views ----

#[test]
fn a_view_shows_your_hand_and_hides_the_opponents() {
    let (state, first, second) = started("deck_a", "deck_c", 1);
    let view = player_view(&state, first).unwrap();
    assert_eq!(view.viewer, first);

    let mine = &view.players[first.index()];
    let theirs = &view.players[second.index()];
    assert_eq!((mine.player, theirs.player), (first, second));
    assert_eq!(mine.hand.len(), 7);
    for (card, id) in mine.hand.iter().zip(&state.player(first).hand) {
        assert_eq!(*card, state.card_ref(*id).unwrap());
    }
    assert_eq!(theirs.hand.len(), 7);
    for (card, id) in theirs.hand.iter().zip(&state.player(second).hand) {
        assert_eq!(
            *card,
            CardRef {
                object: *id,
                face: None
            }
        );
    }
    assert_eq!((mine.library_size, theirs.library_size), (18, 18));
    assert_eq!((mine.life, theirs.life), (20, 20));
}

#[test]
fn a_view_never_names_a_hidden_card() {
    // Give the opponent a card nobody else has: it must not appear anywhere
    // in the viewer's view, in hand or in the library.
    let (mut state, me, opponent) = empty_board(1);
    put_in_hand(&mut state, opponent, "serra_angel");
    let hidden_card = state.objects[state.player(opponent).hand.last().unwrap()].card;
    let top_of_library = state.objects[state.player(me).library.last().unwrap()].card;

    let json = serde_json::to_string(&player_view(&state, me).unwrap()).unwrap();
    assert!(!json.contains("serra_angel"));
    let view: PlayerView = serde_json::from_str(&json).unwrap();
    let visible = |card| {
        view.players
            .iter()
            .flat_map(|player| player.hand.iter().chain(&player.graveyard))
            .any(|shown| shown.face.as_ref().is_some_and(|face| face.card == card))
    };
    assert!(!visible(hidden_card));
    assert!(!visible(top_of_library));

    // The owner sees it.
    let json = serde_json::to_string(&player_view(&state, opponent).unwrap()).unwrap();
    assert!(json.contains("serra_angel"));
}

#[test]
fn a_view_reports_turn_step_and_decision() {
    let (mut state, first, second) = started("deck_a", "deck_c", 1);
    pass_until(&mut state, 1, Step::PrecombatMain);
    for viewer in [first, second] {
        let view = player_view(&state, viewer).unwrap();
        assert_eq!((view.turn, view.step), (1, Step::PrecombatMain));
        assert_eq!(view.active_player, first);
        assert_eq!(view.starting_player, first);
        assert_eq!(view.decision, Some(Decision::Priority { player: first }));
        assert_eq!(view.result, None);
    }
}

#[test]
fn a_view_shows_permanents_with_their_current_stats() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_lands(&mut state, me, "plains", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let growth = put_in_hand(&mut state, me, "giant_growth");
    let pacifism = put_in_hand(&mut state, me, "pacifism");
    apply_action(
        &mut state,
        me,
        cast_at(growth, Target::Object { object: bears }),
    )
    .unwrap();
    pass_both(&mut state);
    apply_action(
        &mut state,
        me,
        cast_at(pacifism, Target::Object { object: giant }),
    )
    .unwrap();
    pass_both(&mut state);
    state.objects.get_mut(&giant).unwrap().damage = 1;

    let view = player_view(&state, opponent).unwrap();
    let find = |def: &str| {
        view.battlefield
            .iter()
            .find(|permanent| permanent.def.as_str() == def)
            .unwrap()
    };
    let bears_view = find("grizzly_bears");
    assert_eq!(bears_view.object, bears);
    assert_eq!(bears_view.controller, me);
    assert_eq!((bears_view.power, bears_view.toughness), (Some(5), Some(5)));
    assert!(!bears_view.tapped && !bears_view.summoning_sick);

    let giant_view = find("hill_giant");
    assert_eq!((giant_view.power, giant_view.damage), (Some(3), 1));
    let aura = find("pacifism");
    assert_eq!(aura.attached_to, Some(giant));
    assert_eq!((aura.power, aura.toughness), (None, None));
    let forest = find("forest");
    assert!(forest.tapped);
    assert_eq!(forest.power, None);
    assert_eq!(view.players[me.index()].graveyard.len(), 1);
    assert_eq!(view.battlefield.len(), state.battlefield.len());
}

#[test]
fn a_view_shows_the_stack_and_combat() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let visionary = put_in_hand(&mut state, me, "elvish_visionary");
    apply_action(&mut state, me, cast(visionary)).unwrap();
    let view = player_view(&state, opponent).unwrap();
    assert_eq!(view.stack.len(), 1);
    assert_eq!(view.stack[0].controller, me);
    assert!(matches!(
        &view.stack[0].kind,
        StackItemViewKind::Spell { card } if card.def.as_str() == "elvish_visionary"
    ));

    pass_both(&mut state);
    let view = player_view(&state, opponent).unwrap();
    assert!(matches!(
        &view.stack[0].kind,
        StackItemViewKind::Ability { ability }
            if ability.def.as_str() == "elvish_visionary" && ability.index == 0
    ));
    pass_both(&mut state);

    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    let view = player_view(&state, opponent).unwrap();
    assert!(view.combat.unwrap().is_attacking(bears));
}

#[test]
fn a_view_shows_the_result() {
    let (mut state, _, opponent) = empty_board(1);
    state.player_mut(opponent).life = 0;
    pass_both(&mut state);
    let view = player_view(&state, opponent).unwrap();
    assert_eq!(view.result, state.result);
    assert!(view.result.is_some());
    assert_eq!(view.decision, None);
}

// ---- Redaction ----

#[test]
fn redaction_hides_what_the_opponent_drew() {
    let (mut state, first, second) = started("deck_a", "deck_c", 1);
    pass_until(&mut state, 2, Step::Upkeep);
    let events = pass_both(&mut state);
    let drawn = events
        .iter()
        .find(|event| matches!(event, GameEvent::CardDrawn { .. }))
        .unwrap();
    let GameEvent::CardDrawn { player, card } = drawn else {
        unreachable!()
    };
    assert_eq!(*player, second);
    assert!(card.face.is_some());

    // The player who drew sees everything.
    assert_eq!(redact_events(&events, second), events);
    // The other player sees that a card was drawn, not which.
    let redacted = redact_events(&events, first);
    assert_eq!(redacted.len(), events.len());
    assert!(redacted.contains(&GameEvent::CardDrawn {
        player: second,
        card: CardRef {
            object: card.object,
            face: None
        }
    }));
    let json = serde_json::to_string(&redacted).unwrap();
    let face = card.face.as_ref().unwrap();
    assert!(!json.contains(&format!("\"card\":{}", face.card.0)));
}

#[test]
fn redaction_hides_mulligan_draws_and_bottomed_cards() {
    let mut state = new_duel(config("deck_a", "deck_c"), 4).unwrap();
    let first = state.starting_player;
    let second = first.opponent();
    let mut events = apply_action(&mut state, first, Action::Mulligan).unwrap();
    events.extend(apply_action(&mut state, second, Action::Keep).unwrap());
    events.extend(apply_action(&mut state, first, Action::Keep).unwrap());
    let card = state.player(first).hand[0];
    events.extend(apply_action(&mut state, first, Action::BottomCard { card }).unwrap());

    let hidden_faces = |events: &[GameEvent]| {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    GameEvent::CardDrawn { card, .. } | GameEvent::CardPutOnBottom { card, .. }
                        if card.face.is_none()
                )
            })
            .count()
    };
    assert_eq!(hidden_faces(&events), 0);
    assert_eq!(hidden_faces(&redact_events(&events, first)), 0);
    // Seven draws and one bottomed card.
    assert_eq!(hidden_faces(&redact_events(&events, second)), 8);
    // That a mulligan happened is public.
    assert!(
        redact_events(&events, second).contains(&GameEvent::MulliganTaken {
            player: first,
            mulligans_taken: 1
        })
    );
}

#[test]
fn public_events_are_not_redacted() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    let land = put_in_hand(&mut state, me, "mountain");
    let mut events = apply_action(&mut state, me, Action::PlayLand { card: land }).unwrap();
    let target = Target::Player { player: opponent };
    events.extend(apply_action(&mut state, me, cast_at(shock, target)).unwrap());
    events.extend(pass_both(&mut state));
    assert_eq!(redact_events(&events, opponent), events);
}

// ---- Auto-pass ----

#[test]
fn has_meaningful_action_is_false_when_passing_is_the_only_choice() {
    let (mut state, me, opponent) = empty_board(1);
    assert!(!has_meaningful_action(&state, me));
    // The player without priority has no action at all.
    assert!(!has_meaningful_action(&state, opponent));

    let land = put_in_hand(&mut state, me, "forest");
    assert!(has_meaningful_action(&state, me));
    apply_action(&mut state, me, Action::PlayLand { card: land }).unwrap();
    assert!(!has_meaningful_action(&state, me));

    // A castable spell counts; one the player can't pay for doesn't.
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    assert!(!has_meaningful_action(&state, me));
    put_lands(&mut state, me, "forest", 1);
    assert!(has_meaningful_action(&state, me));
    apply_action(&mut state, me, cast(bears)).unwrap();
    assert!(!has_meaningful_action(&state, me));
}

#[test]
fn has_meaningful_action_is_true_for_every_other_decision() {
    let state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
    let first = state.starting_player;
    assert!(has_meaningful_action(&state, first));
    assert!(!has_meaningful_action(&state, first.opponent()));

    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until_other_decision(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareAttackers { player: me })
    );
    assert!(has_meaningful_action(&state, me));
}

#[test]
fn an_instant_with_a_target_is_a_meaningful_action_on_the_opponents_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 1);
    put_in_hand(&mut state, opponent, "shock");
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(has_meaningful_action(&state, opponent));
}

#[test]
fn a_player_with_nothing_to_do_is_passed_for_unless_holding_priority() {
    let (mut state, me, opponent) = empty_board(1);
    assert!(should_auto_pass(&state, me, false));
    assert!(!should_auto_pass(&state, me, true));
    // Nobody is passed for when it isn't their priority.
    assert!(!should_auto_pass(&state, opponent, false));

    // A land to play in your own main phase is worth stopping for.
    put_in_hand(&mut state, me, "forest");
    assert!(!should_auto_pass(&state, me, false));
}

#[test]
fn other_decisions_are_never_passed_for() {
    let state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
    for player in PlayerId::ALL {
        assert!(!should_auto_pass(&state, player, false));
    }

    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until_other_decision(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareAttackers { player: me })
    );
    assert!(!should_auto_pass(&state, me, false));
}

/// Plays on until `turn` begins: everyone passes, every creature attacks and
/// nothing blocks. Returns each (active player, step) at which `player` got
/// priority and `stops(state)` was true.
fn stops_until_turn(
    state: &mut DuelState,
    player: PlayerId,
    turn: u32,
    stops: impl Fn(&DuelState) -> bool,
) -> Vec<(PlayerId, Step)> {
    let mut stopped = Vec::new();
    while state.turn < turn {
        let action = match state.decision.unwrap() {
            Decision::Priority { player: asked } => {
                if asked == player && stops(state) {
                    stopped.push((state.active_player, state.step));
                }
                Action::PassPriority
            }
            Decision::DeclareAttackers { player: asked } => legal_actions(state, asked)
                .into_iter()
                .find(|action| matches!(action, Action::Attack { .. }))
                .unwrap_or(Action::ConfirmAttackers),
            Decision::DeclareBlockers { .. } => Action::ConfirmBlockers,
            other => panic!("unexpected decision {other:?}"),
        };
        let asked = state.waiting_on().unwrap();
        apply_action(state, asked, action).unwrap();
    }
    stopped
}

#[test]
fn with_an_instant_ready_you_are_stopped_only_where_it_matters() {
    let setup = || {
        let (mut state, me, opponent) = empty_board(1);
        put_lands(&mut state, me, "swamp", 3);
        put_in_hand(&mut state, me, "murder");
        put_on_battlefield(&mut state, me, "grizzly_bears");
        put_on_battlefield(&mut state, opponent, "hill_giant");
        (state, me, opponent)
    };

    let (mut state, me, opponent) = setup();
    let stops = stops_until_turn(&mut state, me, 3, |state| {
        !should_auto_pass(state, me, false)
    });
    assert_eq!(
        stops,
        [
            (me, Step::PrecombatMain),
            (me, Step::DeclareAttackers),
            (me, Step::DeclareBlockers),
            (me, Step::PostcombatMain),
            (opponent, Step::DeclareAttackers),
            (opponent, Step::DeclareBlockers),
            (opponent, Step::End),
        ]
    );

    // Murder was castable every time: holding priority stops at them all.
    let (mut state, me, _) = setup();
    let all = stops_until_turn(&mut state, me, 3, |state| {
        !should_auto_pass(state, me, true)
    });
    let (mut state, me, _) = setup();
    let castable = stops_until_turn(&mut state, me, 3, |state| has_meaningful_action(state, me));
    assert_eq!(all, castable);
    assert!(all.len() > 15, "{all:?}");
}

#[test]
fn you_are_stopped_to_respond_to_the_opponents_spell_but_not_your_own() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let shock = put_in_hand(&mut state, me, "shock");
    put_in_hand(&mut state, me, "shock");
    put_lands(&mut state, opponent, "mountain", 1);
    put_in_hand(&mut state, opponent, "shock");

    let face = Target::Player { player: opponent };
    apply_action(&mut state, me, cast_at(shock, face)).unwrap();
    // I could cast the second Shock, but my own spell is left to resolve.
    assert!(has_meaningful_action(&state, me));
    assert!(should_auto_pass(&state, me, false));
    assert!(!should_auto_pass(&state, me, true));

    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(!should_auto_pass(&state, opponent, false));

    // With nothing to respond with, the opponent would be passed for.
    crate::duel::test_support::empty_hand(&mut state, opponent);
    assert!(should_auto_pass(&state, opponent, false));
}

// ---- Determinism and random play ----

#[test]
fn same_seed_and_actions_give_an_identical_event_log() {
    for (index, (p1, p2)) in PAIRINGS.into_iter().enumerate() {
        let seed = 100 + index as u64;
        let play =
            |choice_seed: u64| play_randomly(config(p1, p2), seed, choice_seed, |_, _, _| {});
        let (state, log) = play(7);
        let (again_state, again_log) = play(7);
        assert_eq!(log, again_log, "{p1} v {p2}");
        assert_eq!(state, again_state, "{p1} v {p2}");
        assert!(state.result.is_some());
        // The log is what makes two games the same or different.
        assert_ne!(log, play(8).1, "{p1} v {p2}");
    }
}

#[test]
fn a_different_seed_gives_a_different_game() {
    let play = |seed: u64| play_randomly(config("deck_a", "deck_c"), seed, 1, |_, _, _| {}).1;
    assert_ne!(play(1), play(2));
}

#[test]
fn random_games_never_break_the_engine() {
    for game in 0..60u64 {
        let (p1, p2) = PAIRINGS[(game % 6) as usize];
        let mut checks = 0u32;
        let (state, log) = play_randomly(config(p1, p2), game, game * 31 + 1, |state, _, _| {
            // Checking everything on every action is slow; sample.
            checks += 1;
            if checks.is_multiple_of(25) {
                state.assert_consistent();
                for viewer in PlayerId::ALL {
                    player_view(state, viewer).unwrap();
                }
            }
        });
        state.assert_consistent();
        assert!(state.result.is_some(), "game {game} ({p1} v {p2})");
        assert!(matches!(log.last(), Some(GameEvent::GameOver { .. })));
        for viewer in PlayerId::ALL {
            assert_eq!(redact_events(&log, viewer).len(), log.len());
        }
    }
}

#[test]
fn every_legal_action_can_be_applied() {
    // At every point of a few random games, apply each legal action to a
    // copy of the state: none may be rejected.
    for (game, (p1, p2)) in PAIRINGS.into_iter().enumerate() {
        play_randomly(
            config(p1, p2),
            500 + game as u64,
            game as u64,
            |state: &DuelState, player, actions| {
                for action in actions {
                    let mut copy = state.clone();
                    let outcome = apply_action(&mut copy, player, action.clone());
                    assert!(outcome.is_ok(), "{action:?} failed: {outcome:?}");
                }
            },
        );
    }
}

#[test]
fn a_saved_game_restored_mid_way_plays_on_identically() {
    let mut saved = None;
    let mut count = 0;
    let (state, _) = play_randomly(config("deck_b", "deck_c"), 9, 9, |state, _, _| {
        count += 1;
        if count == 150 {
            saved = Some(serde_json::to_string(state).unwrap());
        }
    });
    let restored: DuelState = serde_json::from_str(&saved.unwrap()).unwrap();
    assert!(restored.result.is_none());
    assert_ne!(restored, state);
    // The restored state equals the live one at the same point, so every
    // later action does the same thing.
    let mut again = None;
    let mut count = 0;
    play_randomly(config("deck_b", "deck_c"), 9, 9, |state, _, _| {
        count += 1;
        if count == 150 {
            again = Some(state.clone());
        }
    });
    assert_eq!(Some(restored), again);
}
