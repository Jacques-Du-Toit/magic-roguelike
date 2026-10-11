//! Tests for instants, sorceries, targets, triggered abilities and
//! counterspells.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{
    AbilityRef, Decision, DuelState, GameResult, PendingTrigger, StackItemKind, Step,
    TriggeredAbility, WinReason, Zone,
};
use super::test_support::{
    attack, block, cast, cast_at, empty_board, finish_combat, pass_both, pass_until,
    pass_until_other_decision, put_in_hand, put_lands, put_on_battlefield,
};
use super::turn::open_priority;
use super::{apply_action, legal_actions, result};
use crate::cards::CardDefId;

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

fn life(state: &DuelState, player: PlayerId) -> i32 {
    state.player(player).life
}

fn gone(state: &DuelState, permanent: ObjectId) -> bool {
    !state.is_on_battlefield(permanent)
}

/// The targets the player could cast `card` at right now.
fn targets_of(state: &DuelState, player: PlayerId, card: ObjectId) -> Vec<Vec<Target>> {
    legal_actions(state, player)
        .into_iter()
        .filter_map(|action| match action {
            Action::CastSpell {
                card: cast,
                targets,
            } if cast == card => Some(targets),
            _ => None,
        })
        .collect()
}

fn can_cast(state: &DuelState, player: PlayerId, card: ObjectId) -> bool {
    !targets_of(state, player, card).is_empty()
}

/// The definitions of the cards in a player's graveyard, oldest first.
fn graveyard(state: &DuelState, player: PlayerId) -> Vec<&str> {
    state
        .player(player)
        .graveyard
        .iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

fn pass(state: &mut DuelState, player: PlayerId) -> Vec<GameEvent> {
    act(state, player, Action::PassPriority)
}

// ---- Shock and Lightning Strike ----

#[test]
fn shock_can_target_any_player_or_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let shock = put_in_hand(&mut state, me, "shock");
    assert_eq!(
        targets_of(&state, me, shock),
        [
            vec![face(PlayerId::P1)],
            vec![face(PlayerId::P2)],
            vec![at(mine)],
            vec![at(theirs)]
        ]
    );
    // A spell needs its target: no target, no spell.
    assert!(apply_action(&mut state, me, cast(shock)).is_err());
    assert!(apply_action(&mut state, me, cast_at(shock, at(ObjectId(9999)))).is_err());
}

#[test]
fn shock_deals_2_damage_to_a_player() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");

    let events = act(&mut state, me, cast_at(shock, face(opponent)));
    let spell = state.stack[0].id;
    assert_eq!(
        events[0],
        GameEvent::SpellCast {
            player: me,
            card: state.card_ref(spell).unwrap(),
            targets: vec![face(opponent)]
        }
    );
    assert_eq!(life(&state, opponent), 20);

    let events = pass_both(&mut state);
    let in_graveyard = state.player(me).graveyard[0];
    assert_eq!(
        events,
        [
            GameEvent::DamageDealt {
                source: spell,
                target: face(opponent),
                amount: 2
            },
            GameEvent::LifeChanged {
                player: opponent,
                change: -2,
                life: 18
            },
            GameEvent::SpellResolved {
                player: me,
                card: state.card_ref(in_graveyard).unwrap()
            },
        ]
    );
    assert_eq!(graveyard(&state, me), ["shock"]);
    assert!(state.stack.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    state.assert_consistent();
}

#[test]
fn shock_kills_a_creature_with_toughness_2() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let first = put_in_hand(&mut state, me, "shock");
    let second = put_in_hand(&mut state, me, "shock");

    act(&mut state, me, cast_at(first, at(bears)));
    let events = pass_both(&mut state);
    assert!(gone(&state, bears));
    assert!(events.iter().any(
        |event| matches!(event, GameEvent::CreatureDied { permanent, .. } if *permanent == bears)
    ));
    assert_eq!(graveyard(&state, opponent), ["grizzly_bears"]);

    // 2 damage on a 3/3 stays until the end of the turn.
    act(&mut state, me, cast_at(second, at(giant)));
    pass_both(&mut state);
    assert_eq!(state.objects[&giant].damage, 2);
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(state.objects[&giant].damage, 0);
}

#[test]
fn a_player_can_shock_themselves() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    act(&mut state, me, cast_at(shock, face(me)));
    pass_both(&mut state);
    assert_eq!(life(&state, me), 18);
}

#[test]
fn lethal_burn_ends_the_game() {
    let (mut state, me, opponent) = empty_board(1);
    state.player_mut(opponent).life = 2;
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    act(&mut state, me, cast_at(shock, face(opponent)));
    pass_both(&mut state);
    assert_eq!(
        result(&state),
        Some(GameResult::Win {
            player: me,
            reason: WinReason::LifeZero
        })
    );
    assert_eq!(state.decision, None);
}

#[test]
fn lightning_strike_deals_3_damage_to_any_target() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 4);
    let courser = put_on_battlefield(&mut state, opponent, "centaur_courser");
    let first = put_in_hand(&mut state, me, "lightning_strike");
    let second = put_in_hand(&mut state, me, "lightning_strike");
    assert_eq!(targets_of(&state, me, first).len(), 3);

    act(&mut state, me, cast_at(first, at(courser)));
    pass_both(&mut state);
    assert!(gone(&state, courser));

    act(&mut state, me, cast_at(second, face(opponent)));
    pass_both(&mut state);
    assert_eq!(life(&state, opponent), 17);
    assert_eq!(
        graveyard(&state, me),
        ["lightning_strike", "lightning_strike"]
    );
}

#[test]
fn instants_can_be_cast_whenever_the_player_has_priority() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, opponent, "mountain", 1);
    let mine = put_in_hand(&mut state, me, "shock");
    let theirs = put_in_hand(&mut state, opponent, "shock");

    // The opponent, on my turn, once I pass.
    assert!(!can_cast(&state, opponent, theirs));
    pass(&mut state, me);
    assert!(can_cast(&state, opponent, theirs));
    act(&mut state, opponent, cast_at(theirs, face(me)));
    // In response to a spell.
    pass(&mut state, opponent);
    assert!(can_cast(&state, me, mine));
    pass_both(&mut state);
    assert_eq!(life(&state, me), 18);

    // In the opponent's upkeep.
    pass_until(&mut state, 2, Step::Upkeep);
    pass(&mut state, opponent);
    assert!(can_cast(&state, me, mine));
}

// ---- Murder ----

#[test]
fn murder_destroys_target_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let murder = put_in_hand(&mut state, me, "murder");
    assert!(!can_cast(&state, me, murder));

    let dreadmaw = put_on_battlefield(&mut state, opponent, "colossal_dreadmaw");
    let mine = put_on_battlefield(&mut state, me, "typhoid_rats");
    // Creatures only, and your own are fair game.
    assert_eq!(
        targets_of(&state, me, murder),
        [vec![at(dreadmaw)], vec![at(mine)]]
    );

    act(&mut state, me, cast_at(murder, at(dreadmaw)));
    let events = pass_both(&mut state);
    assert!(gone(&state, dreadmaw));
    assert!(events.iter().any(
        |event| matches!(event, GameEvent::CreatureDied { permanent, .. } if *permanent == dreadmaw)
    ));
    assert_eq!(graveyard(&state, opponent), ["colossal_dreadmaw"]);
    assert_eq!(graveyard(&state, me), ["murder"]);
}

// ---- Divination ----

#[test]
fn divination_draws_two_cards() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 3);
    let divination = put_in_hand(&mut state, me, "divination");
    let library = state.player(me).library.len();

    assert_eq!(targets_of(&state, me, divination), [Vec::<Target>::new()]);
    act(&mut state, me, cast(divination));
    let events = pass_both(&mut state);
    let drawn = events
        .iter()
        .filter(|event| matches!(event, GameEvent::CardDrawn { player, .. } if *player == me))
        .count();
    assert_eq!(drawn, 2);
    assert_eq!(state.player(me).hand.len(), 2);
    assert_eq!(state.player(me).library.len(), library - 2);
    assert_eq!(graveyard(&state, me), ["divination"]);
}

#[test]
fn divination_is_a_sorcery() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "island", 7);
    put_lands(&mut state, opponent, "island", 3);
    let mine = put_in_hand(&mut state, me, "divination");
    let another = put_in_hand(&mut state, me, "divination");
    let theirs = put_in_hand(&mut state, opponent, "divination");

    // Not with a spell on the stack.
    let drake = put_in_hand(&mut state, me, "wind_drake");
    act(&mut state, me, cast(drake));
    assert!(!can_cast(&state, me, mine));
    // Not on someone else's turn.
    pass(&mut state, me);
    assert!(!can_cast(&state, opponent, theirs));
    pass(&mut state, opponent);

    // Not outside a main phase.
    pass_until(&mut state, 1, Step::BeginCombat);
    assert!(!can_cast(&state, me, another));
    pass_until(&mut state, 1, Step::PostcombatMain);
    assert!(can_cast(&state, me, another));
}

#[test]
fn drawing_past_the_end_of_the_library_is_fatigue() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 3);
    let divination = put_in_hand(&mut state, me, "divination");
    let library = state.player(me).library.clone();
    for &card in &library[1..] {
        state.move_object(card, Zone::Graveyard).unwrap();
    }

    act(&mut state, me, cast(divination));
    let events = pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
    assert!(events.contains(&GameEvent::Fatigue {
        player: me,
        life_lost: 1
    }));
    assert_eq!(life(&state, me), 19);
}

// ---- Giant Growth ----

#[test]
fn giant_growth_gives_plus_3_plus_3_until_end_of_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let growth = put_in_hand(&mut state, me, "giant_growth");
    assert!(!can_cast(&state, me, growth));
    // Any creature will do, including the opponent's.
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");

    act(&mut state, me, cast_at(growth, at(bears)));
    let events = pass_both(&mut state);
    assert_eq!(
        events[0],
        GameEvent::PowerToughnessChanged {
            permanent: bears,
            power: 3,
            toughness: 3
        }
    );
    assert_eq!(
        (state.power(bears).unwrap(), state.toughness(bears).unwrap()),
        (5, 5)
    );

    pass_until(&mut state, 1, Step::End);
    assert_eq!(state.power(bears).unwrap(), 5);
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(
        (state.power(bears).unwrap(), state.toughness(bears).unwrap()),
        (2, 2)
    );
}

#[test]
fn giant_growth_wins_a_combat() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let growth = put_in_hand(&mut state, me, "giant_growth");
    let attacker = put_on_battlefield(&mut state, me, "grizzly_bears");
    let blocker = put_on_battlefield(&mut state, opponent, "hill_giant");
    attack(&mut state, &[attacker]);
    block(&mut state, &[(blocker, attacker)]);

    act(&mut state, me, cast_at(growth, at(attacker)));
    pass_both(&mut state);
    finish_combat(&mut state);
    assert!(gone(&state, blocker));
    assert_eq!(state.objects[&attacker].damage, 3);

    // The damage and the bonus go away together: the Bears survive.
    pass_until(&mut state, 2, Step::Upkeep);
    assert!(!gone(&state, attacker));
}

#[test]
fn the_stack_resolves_last_in_first_out() {
    // The opponent Shocks my Bears; I respond with Giant Growth, which
    // resolves first and saves them.
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let growth = put_in_hand(&mut state, me, "giant_growth");
    let shock = put_in_hand(&mut state, opponent, "shock");

    pass(&mut state, me);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    assert_eq!(
        state.decision,
        Some(Decision::Priority { player: opponent })
    );
    pass(&mut state, opponent);
    act(&mut state, me, cast_at(growth, at(bears)));
    assert_eq!(state.stack.len(), 2);

    let events = pass_both(&mut state);
    assert!(matches!(
        events.last(),
        Some(GameEvent::SpellResolved { player, .. }) if *player == me
    ));
    assert_eq!(state.stack.len(), 1);
    // The step doesn't end while something is on the stack.
    assert_eq!(state.step, Step::PrecombatMain);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));

    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert!(!gone(&state, bears));
    assert_eq!(state.objects[&bears].damage, 2);
}

// ---- Fizzling ----

#[test]
fn a_spell_whose_target_is_gone_does_nothing() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, me, "swamp", 3);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let shock = put_in_hand(&mut state, me, "shock");
    let murder = put_in_hand(&mut state, me, "murder");

    // Shock the Bears, then Murder them in response.
    act(&mut state, me, cast_at(shock, at(bears)));
    act(&mut state, me, cast_at(murder, at(bears)));
    pass_both(&mut state);
    assert!(gone(&state, bears));

    let events = pass_both(&mut state);
    let in_graveyard = *state.player(me).graveyard.last().unwrap();
    assert_eq!(
        events,
        [GameEvent::SpellFizzled {
            player: me,
            card: state.card_ref(in_graveyard).unwrap()
        }]
    );
    assert_eq!(graveyard(&state, me), ["murder", "shock"]);
    assert_eq!(life(&state, opponent), 20);
}

#[test]
fn giant_growth_fizzles_if_its_creature_dies_in_response() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let growth = put_in_hand(&mut state, me, "giant_growth");
    let shock = put_in_hand(&mut state, opponent, "shock");

    act(&mut state, me, cast_at(growth, at(bears)));
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    pass_both(&mut state);
    assert!(gone(&state, bears));
    let events = pass_both(&mut state);
    assert!(matches!(&events[..], [GameEvent::SpellFizzled { .. }]));
}

// ---- Cancel ----

#[test]
fn cancel_counters_a_creature_spell() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "island", 3);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let cancel = put_in_hand(&mut state, opponent, "cancel");

    // Nothing to counter yet.
    pass(&mut state, me);
    assert!(!can_cast(&state, opponent, cancel));
    pass(&mut state, opponent);
    pass_until(&mut state, 1, Step::PostcombatMain);

    act(&mut state, me, cast(bears));
    let spell = state.stack[0].id;
    pass(&mut state, me);
    assert_eq!(targets_of(&state, opponent, cancel), [vec![at(spell)]]);
    act(&mut state, opponent, cast_at(cancel, at(spell)));

    let events = pass_both(&mut state);
    let countered = state.player(me).graveyard[0];
    assert_eq!(
        events[0],
        GameEvent::SpellCountered {
            player: me,
            card: state.card_ref(countered).unwrap()
        }
    );
    assert!(matches!(
        events[1],
        GameEvent::SpellResolved { player, .. } if player == opponent
    ));
    assert!(state.stack.is_empty());
    assert!(state.creatures_of(me).unwrap().is_empty());
    assert_eq!(graveyard(&state, me), ["grizzly_bears"]);
    assert_eq!(graveyard(&state, opponent), ["cancel"]);
    state.assert_consistent();
}

#[test]
fn cancel_counters_an_instant() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, opponent, "island", 3);
    let shock = put_in_hand(&mut state, me, "shock");
    let cancel = put_in_hand(&mut state, opponent, "cancel");
    act(&mut state, me, cast_at(shock, face(opponent)));
    let spell = state.stack[0].id;
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(cancel, at(spell)));
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(life(&state, opponent), 20);
}

#[test]
fn a_counterspell_can_be_countered() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "island", 3);
    put_lands(&mut state, opponent, "island", 3);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let mine = put_in_hand(&mut state, me, "cancel");
    let theirs = put_in_hand(&mut state, opponent, "cancel");

    act(&mut state, me, cast(bears));
    let bears_spell = state.stack[0].id;
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(theirs, at(bears_spell)));
    let their_spell = state.stack[1].id;
    pass(&mut state, opponent);
    // Either spell on the stack can be countered.
    assert_eq!(
        targets_of(&state, me, mine),
        [vec![at(bears_spell)], vec![at(their_spell)]]
    );
    act(&mut state, me, cast_at(mine, at(their_spell)));

    pass_both(&mut state);
    assert_eq!(graveyard(&state, opponent), ["cancel"]);
    pass_both(&mut state);
    assert_eq!(state.creatures_of(me).unwrap().len(), 1);
}

#[test]
fn a_second_cancel_on_the_same_spell_fizzles() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "island", 6);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let first = put_in_hand(&mut state, opponent, "cancel");
    let second = put_in_hand(&mut state, opponent, "cancel");

    act(&mut state, me, cast(bears));
    let spell = state.stack[0].id;
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(first, at(spell)));
    act(&mut state, opponent, cast_at(second, at(spell)));
    pass_both(&mut state);
    let events = pass_both(&mut state);
    assert!(matches!(
        &events[..],
        [GameEvent::SpellFizzled { player, .. }] if *player == opponent
    ));
    assert_eq!(graveyard(&state, opponent), ["cancel", "cancel"]);
}

// ---- Enters-the-battlefield triggers ----

#[test]
fn elvish_visionary_draws_a_card_when_it_enters() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let visionary = put_in_hand(&mut state, me, "elvish_visionary");
    act(&mut state, me, cast(visionary));

    // The creature resolves and its ability goes on the stack.
    let events = pass_both(&mut state);
    let permanent = *state.battlefield.last().unwrap();
    let ability = state.stack[0].id;
    assert!(matches!(events[0], GameEvent::PermanentEntered { .. }));
    assert_eq!(
        events[1],
        GameEvent::AbilityTriggered {
            player: me,
            source: permanent,
            ability
        }
    );
    assert!(matches!(state.stack[0].kind, StackItemKind::Trigger { .. }));
    assert_eq!(state.stack[0].controller, me);
    assert!(state.player(me).hand.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    state.assert_consistent();

    let events = pass_both(&mut state);
    assert!(matches!(
        &events[..],
        [GameEvent::CardDrawn { player, .. }, GameEvent::AbilityResolved { ability: resolved }]
            if *player == me && *resolved == ability
    ));
    assert_eq!(state.player(me).hand.len(), 1);
    assert!(state.stack.is_empty());
}

#[test]
fn inspiring_overseer_gains_a_life_and_draws_a_card() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    let overseer = put_in_hand(&mut state, me, "inspiring_overseer");
    act(&mut state, me, cast(overseer));
    pass_both(&mut state);
    let events = pass_both(&mut state);
    assert!(matches!(
        &events[..],
        [
            GameEvent::LifeChanged {
                change: 1,
                life: 21,
                ..
            },
            GameEvent::CardDrawn { .. },
            GameEvent::AbilityResolved { .. }
        ]
    ));
    assert_eq!(life(&state, me), 21);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn an_enters_trigger_still_resolves_if_the_creature_dies_first() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "mountain", 1);
    let visionary = put_in_hand(&mut state, me, "elvish_visionary");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, cast(visionary));
    pass_both(&mut state);
    let permanent = *state.battlefield.last().unwrap();

    pass(&mut state, me);
    act(&mut state, opponent, cast_at(shock, at(permanent)));
    pass_both(&mut state);
    assert!(gone(&state, permanent));
    pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn a_countered_creature_never_triggers() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "island", 3);
    let visionary = put_in_hand(&mut state, me, "elvish_visionary");
    let cancel = put_in_hand(&mut state, opponent, "cancel");
    act(&mut state, me, cast(visionary));
    let spell = state.stack[0].id;
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(cancel, at(spell)));
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert!(state.pending_triggers.is_empty());
    assert!(state.player(me).hand.is_empty());
}

#[test]
fn simultaneous_triggers_go_on_the_stack_active_player_first_then_by_source() {
    let (mut state, me, opponent) = empty_board(1);
    let trigger = |controller: PlayerId, source: u32| PendingTrigger {
        ability: TriggeredAbility::Card {
            ability: AbilityRef {
                source: ObjectId(source),
                def: CardDefId("elvish_visionary".to_string()),
                index: 0,
            },
        },
        controller,
    };
    state.pending_triggers = vec![trigger(opponent, 905), trigger(me, 909), trigger(me, 903)];
    let mut events = Vec::new();
    open_priority(&mut state, &mut events).unwrap();

    let order: Vec<(PlayerId, u32)> = state
        .stack
        .iter()
        .map(|item| match &item.kind {
            StackItemKind::Trigger { ability } => (item.controller, ability.source().0),
            StackItemKind::Spell => panic!("unexpected spell"),
        })
        .collect();
    // The top of the stack is last: the opponent's ability resolves first.
    assert_eq!(order, [(me, 903), (me, 909), (opponent, 905)]);
    assert_eq!(events.len(), 3);

    pass_both(&mut state);
    assert_eq!(state.player(opponent).hand.len(), 1);
    assert!(state.player(me).hand.is_empty());
}

// ---- Spells during combat ----

#[test]
fn a_blocked_attacker_whose_blocker_is_removed_deals_no_damage() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let murder = put_in_hand(&mut state, me, "murder");
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[giant]);
    block(&mut state, &[(bears, giant)]);
    act(&mut state, me, cast_at(murder, at(bears)));
    pass_both(&mut state);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 20);
    assert_eq!(state.objects[&giant].damage, 0);
}

#[test]
fn a_trampler_whose_blocker_is_removed_hits_the_player_in_full() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let murder = put_in_hand(&mut state, me, "murder");
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[dreadmaw]);
    block(&mut state, &[(bears, dreadmaw)]);
    act(&mut state, me, cast_at(murder, at(bears)));
    pass_both(&mut state);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 14);
}

#[test]
fn an_attacker_removed_before_damage_deals_none() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 1);
    let shock = put_in_hand(&mut state, opponent, "shock");
    let lions = put_on_battlefield(&mut state, me, "savannah_lions");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[lions, bears]);
    pass(&mut state, me);
    act(&mut state, opponent, cast_at(shock, at(lions)));
    pass_both(&mut state);
    assert!(gone(&state, lions));
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 18);
}

#[test]
fn a_creature_killed_in_the_first_strike_step_deals_no_damage() {
    // Covered for blockers in the combat tests; here a burn spell finishes
    // off a creature between the two damage steps.
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    let knight = put_on_battlefield(&mut state, me, "youthful_knight");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    attack(&mut state, &[knight]);
    block(&mut state, &[(spider, knight)]);
    pass_until(&mut state, 1, Step::FirstStrikeDamage);
    assert_eq!(state.objects[&spider].damage, 2);
    act(&mut state, me, cast_at(shock, at(spider)));
    pass_both(&mut state);
    assert!(gone(&state, spider));
    finish_combat(&mut state);
    assert!(!gone(&state, knight));
}

#[test]
fn spells_are_not_cast_while_declaring_attackers() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_in_hand(&mut state, me, "shock");
    put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until_other_decision(&mut state);
    assert!(
        legal_actions(&state, me)
            .iter()
            .all(|action| !matches!(action, Action::CastSpell { .. }))
    );
}

// ---- Cards added for the run ----

#[test]
fn volcanic_hammer_is_a_sorcery_that_deals_three_damage() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_lands(&mut state, opponent, "mountain", 2);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let hammer = put_in_hand(&mut state, me, "volcanic_hammer");
    let theirs = put_in_hand(&mut state, opponent, "volcanic_hammer");
    assert_eq!(targets_of(&state, me, hammer).len(), 3);
    act(&mut state, me, cast_at(hammer, at(giant)));
    // Not at instant speed: the opponent can't answer with theirs.
    pass(&mut state, me);
    assert!(!can_cast(&state, opponent, theirs));
    pass(&mut state, opponent);
    assert!(gone(&state, giant));

    let another = put_in_hand(&mut state, me, "volcanic_hammer");
    put_lands(&mut state, me, "mountain", 2);
    act(&mut state, me, cast_at(another, face(opponent)));
    pass_both(&mut state);
    assert_eq!(life(&state, opponent), 17);
    // Not in combat either.
    let third = put_in_hand(&mut state, me, "volcanic_hammer");
    put_lands(&mut state, me, "mountain", 2);
    pass_until(&mut state, 1, Step::BeginCombat);
    assert!(!can_cast(&state, me, third));
}

#[test]
fn revitalize_gains_three_life_and_draws_a_card() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let revitalize = put_in_hand(&mut state, me, "revitalize");
    let library = state.player(me).library.len();
    act(&mut state, me, cast(revitalize));
    pass_both(&mut state);
    assert_eq!(life(&state, me), 23);
    assert_eq!(life(&state, opponent), 20);
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(state.player(me).library.len(), library - 1);
    assert_eq!(graveyard(&state, me), ["revitalize"]);
}

#[test]
fn angel_of_mercy_gains_three_life_when_it_enters() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 5);
    let angel = put_in_hand(&mut state, me, "angel_of_mercy");
    act(&mut state, me, cast(angel));
    pass_both(&mut state);
    assert_eq!(life(&state, me), 20);
    assert!(matches!(state.stack[0].kind, StackItemKind::Trigger { .. }));
    pass_both(&mut state);
    assert_eq!(life(&state, me), 23);
}

#[test]
fn vampire_spawn_drains_the_opponent_when_it_enters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let spawn = put_in_hand(&mut state, me, "vampire_spawn");
    act(&mut state, me, cast(spawn));
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(life(&state, opponent), 18);
    assert_eq!(life(&state, me), 22);
}

#[test]
fn vampire_spawn_can_finish_the_opponent() {
    let (mut state, me, opponent) = empty_board(1);
    state.player_mut(opponent).life = 2;
    put_lands(&mut state, me, "swamp", 3);
    let spawn = put_in_hand(&mut state, me, "vampire_spawn");
    act(&mut state, me, cast(spawn));
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(
        result(&state),
        Some(GameResult::Win {
            player: me,
            reason: WinReason::LifeZero
        })
    );
}

// ---- Guildgates and paying with them ----

#[test]
fn a_guildgate_enters_tapped_and_untaps_next_turn() {
    let (mut state, me, _) = empty_board(1);
    let gate = put_in_hand(&mut state, me, "selesnya_guildgate");
    let bears = put_in_hand(&mut state, me, "savannah_lions");
    act(&mut state, me, Action::PlayLand { card: gate });
    let gate = *state.battlefield.last().unwrap();
    assert!(state.objects[&gate].tapped);
    assert!(!can_cast(&state, me, bears));
    // A basic land does not.
    let forest = put_in_hand(&mut state, me, "forest");
    assert!(!legal_actions(&state, me).contains(&Action::PlayLand { card: forest }));

    pass_until(&mut state, 3, Step::PrecombatMain);
    assert!(!state.objects[&gate].tapped);
    assert!(can_cast(&state, me, bears));
}

#[test]
fn a_guildgate_pays_for_either_of_its_colours_but_only_once() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "selesnya_guildgate");
    let lions = put_in_hand(&mut state, me, "savannah_lions");
    let elves = put_in_hand(&mut state, me, "llanowar_elves");
    let goblin = put_in_hand(&mut state, me, "raging_goblin");
    assert!(can_cast(&state, me, lions));
    assert!(can_cast(&state, me, elves));
    assert!(!can_cast(&state, me, goblin));
    act(&mut state, me, cast(lions));
    pass_both(&mut state);
    assert!(!can_cast(&state, me, elves));
}

#[test]
fn the_payer_gives_a_guildgate_the_colour_nothing_else_can_make() {
    // Youthful Knight costs {1}{W}. With a Forest and a Guildgate, the Gate
    // has to make the white mana.
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "selesnya_guildgate");
    put_on_battlefield(&mut state, me, "forest");
    let knight = put_in_hand(&mut state, me, "youthful_knight");
    assert!(can_cast(&state, me, knight));
    act(&mut state, me, cast(knight));
    assert!(state.player(me).mana_pool.is_empty());

    // Aerial Responder costs {1}{W}{W}: a Plains, a Gate and a Forest pay
    // for it, whichever order they entered in.
    for lands in [
        ["selesnya_guildgate", "plains", "forest"],
        ["forest", "selesnya_guildgate", "plains"],
        ["plains", "forest", "selesnya_guildgate"],
    ] {
        let (mut state, me, _) = empty_board(1);
        for land in lands {
            put_on_battlefield(&mut state, me, land);
        }
        let responder = put_in_hand(&mut state, me, "aerial_responder");
        assert!(can_cast(&state, me, responder), "{lands:?}");
        act(&mut state, me, cast(responder));
    }

    // Two Gates and a Mountain can't make {G}{G}{G}{G}{G}, and one Gate
    // can't make both colours of two cards at once.
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "selesnya_guildgate");
    put_on_battlefield(&mut state, me, "mountain");
    let responder = put_in_hand(&mut state, me, "aerial_responder");
    assert!(!can_cast(&state, me, responder));
}

#[test]
fn generic_mana_is_paid_with_basics_before_a_guildgate() {
    // Grizzly Bears ({1}{G}) with two Forests and a Gate: the Gate stays
    // untapped, so the Savannah Lions in hand can still be cast.
    let (mut state, me, _) = empty_board(1);
    let gate = put_on_battlefield(&mut state, me, "selesnya_guildgate");
    put_on_battlefield(&mut state, me, "forest");
    put_on_battlefield(&mut state, me, "forest");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let lions = put_in_hand(&mut state, me, "savannah_lions");
    act(&mut state, me, cast(bears));
    assert!(!state.objects[&gate].tapped);
    pass_both(&mut state);
    assert!(can_cast(&state, me, lions));
}
