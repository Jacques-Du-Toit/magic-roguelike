//! Tests for activated abilities, sacrificing as a cost, and the M4a cards
//! that use them.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    cast, cast_at, empty_board, pass_both, pass_until, put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, has_meaningful_action, legal_actions, player_view};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

/// Using the first ability of `source`, with no targets or sacrifice.
fn activate(source: ObjectId) -> Action {
    Action::ActivateAbility {
        source,
        ability: 0,
        targets: Vec::new(),
        sacrifice: None,
    }
}

fn activate_at(source: ObjectId, target: Target) -> Action {
    Action::ActivateAbility {
        source,
        ability: 0,
        targets: vec![target],
        sacrifice: None,
    }
}

fn sacrificing(source: ObjectId, creature: ObjectId) -> Action {
    Action::ActivateAbility {
        source,
        ability: 0,
        targets: Vec::new(),
        sacrifice: Some(creature),
    }
}

/// The activations among the player's legal actions.
fn activations(state: &DuelState, player: PlayerId) -> Vec<Action> {
    legal_actions(state, player)
        .into_iter()
        .filter(|action| matches!(action, Action::ActivateAbility { .. }))
        .collect()
}

fn stats(state: &DuelState, creature: ObjectId) -> (i32, i32) {
    (
        state.power(creature).unwrap(),
        state.toughness(creature).unwrap(),
    )
}

// ---- Tap abilities ----

#[test]
fn prodigal_pyromancer_taps_to_deal_1_damage_to_any_target() {
    let (mut state, me, opponent) = empty_board(1);
    let pyromancer = put_on_battlefield(&mut state, me, "prodigal_pyromancer");
    let goblin = put_on_battlefield(&mut state, opponent, "raging_goblin");
    assert_eq!(
        activations(&state, me),
        [
            activate_at(pyromancer, face(PlayerId::P1)),
            activate_at(pyromancer, face(PlayerId::P2)),
            activate_at(pyromancer, at(pyromancer)),
            activate_at(pyromancer, at(goblin)),
        ]
    );
    assert!(has_meaningful_action(&state, me));

    let events = act(&mut state, me, activate_at(pyromancer, at(goblin)));
    let ability = state.stack[0].id;
    assert_eq!(
        events,
        [
            GameEvent::AbilityActivated {
                player: me,
                source: pyromancer,
                ability,
                targets: vec![at(goblin)]
            },
            GameEvent::PermanentTapped {
                permanent: pyromancer
            }
        ]
    );
    // It uses the stack, its controller keeps priority, and it can't be
    // used again while tapped.
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert!(activations(&state, me).is_empty());
    assert_eq!(
        player_view(&state, me).unwrap().stack[0].targets,
        [at(goblin)]
    );

    let events = pass_both(&mut state);
    assert_eq!(
        events[0],
        GameEvent::DamageDealt {
            source: pyromancer,
            target: at(goblin),
            amount: 1
        }
    );
    assert_eq!(events[1], GameEvent::AbilityResolved { ability });
    assert!(!state.is_on_battlefield(goblin));
    state.assert_consistent();
}

#[test]
fn a_tap_ability_waits_out_summoning_sickness_and_works_in_the_opponents_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 3);
    let pyromancer = put_in_hand(&mut state, me, "prodigal_pyromancer");
    act(&mut state, me, cast(pyromancer));
    pass_both(&mut state);
    let pyromancer = *state.battlefield.last().unwrap();
    assert!(activations(&state, me).is_empty());

    // It stays summoning sick through the opponent's turn, until my next
    // untap step. After that it is ready in anyone's turn.
    pass_until(&mut state, 2, Step::PrecombatMain);
    act(&mut state, opponent, Action::PassPriority);
    assert!(activations(&state, me).is_empty());
    act(&mut state, me, Action::PassPriority);
    pass_until(&mut state, 4, Step::PrecombatMain);
    act(&mut state, opponent, Action::PassPriority);
    assert_eq!(activations(&state, me).len(), 3);
    act(&mut state, me, activate_at(pyromancer, face(opponent)));
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 19);
}

// ---- Mana abilities with a cost in mana ----

#[test]
fn spectral_sailor_draws_a_card_for_four_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 3);
    let sailor = put_on_battlefield(&mut state, me, "spectral_sailor");
    assert!(activations(&state, me).is_empty());
    put_lands(&mut state, me, "island", 1);
    assert_eq!(activations(&state, me), [activate(sailor)]);

    let events = act(&mut state, me, activate(sailor));
    // Four lands tap; the Sailor itself does not.
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GameEvent::PermanentTapped { .. }))
            .count(),
        4
    );
    assert!(!state.objects[&sailor].tapped);
    pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
    assert!(activations(&state, me).is_empty());
}

#[test]
fn shivan_dragon_pumps_once_for_each_red_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_lands(&mut state, me, "forest", 1);
    let dragon = put_on_battlefield(&mut state, me, "shivan_dragon");
    act(&mut state, me, activate(dragon));
    act(&mut state, me, activate(dragon));
    // The Forest can't pay {R}.
    assert!(activations(&state, me).is_empty());
    assert_eq!(state.stack.len(), 2);
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(stats(&state, dragon), (7, 5));
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, dragon), (5, 5));
}

#[test]
fn river_hoopoe_gains_2_life_and_draws_a_card() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    put_lands(&mut state, me, "island", 1);
    let hoopoe = put_on_battlefield(&mut state, me, "river_hoopoe");
    act(&mut state, me, activate(hoopoe));
    pass_both(&mut state);
    assert_eq!(state.player(me).life, 22);
    assert_eq!(state.player(me).hand.len(), 1);
}

// ---- Sacrifice as a cost ----

#[test]
fn bloodthrone_vampire_grows_by_sacrificing_a_creature() {
    let (mut state, me, opponent) = empty_board(1);
    let vampire = put_on_battlefield(&mut state, me, "bloodthrone_vampire");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    // Any creature of its controller's will do, itself included.
    assert_eq!(
        activations(&state, me),
        [sacrificing(vampire, vampire), sacrificing(vampire, bears)]
    );

    let events = act(&mut state, me, sacrificing(vampire, bears));
    assert!(matches!(events[0], GameEvent::AbilityActivated { .. }));
    assert_eq!(
        events[1],
        GameEvent::PermanentSacrificed {
            player: me,
            permanent: bears
        }
    );
    assert!(matches!(
        events[2],
        GameEvent::CreatureDied { permanent, .. } if permanent == bears
    ));
    // The cost is paid at once; the pump waits on the stack.
    assert!(!state.is_on_battlefield(bears));
    assert_eq!(stats(&state, vampire), (1, 1));
    pass_both(&mut state);
    assert_eq!(stats(&state, vampire), (3, 3));
    state.assert_consistent();
}

#[test]
fn an_ability_whose_own_permanent_was_the_sacrifice_does_nothing() {
    let (mut state, me, _) = empty_board(1);
    let vampire = put_on_battlefield(&mut state, me, "bloodthrone_vampire");
    act(&mut state, me, sacrificing(vampire, vampire));
    let events = pass_both(&mut state);
    assert!(matches!(events[..], [GameEvent::AbilityResolved { .. }]));
    assert!(state.battlefield.is_empty());
}

#[test]
fn viscera_seer_scries_by_sacrificing_and_a_dying_creatures_ability_still_triggers() {
    let (mut state, me, _) = empty_board(1);
    let seer = put_on_battlefield(&mut state, me, "viscera_seer");
    let dissenter = put_on_battlefield(&mut state, me, "doomed_dissenter");
    let events = act(&mut state, me, sacrificing(seer, dissenter));
    // The Dissenter's ability goes on the stack above the Seer's.
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameEvent::AbilityTriggered { .. }))
    );
    assert_eq!(state.stack.len(), 2);
    pass_both(&mut state);
    assert!(
        state
            .battlefield
            .iter()
            .any(|id| state.objects[id].def.as_str() == "zombie_token")
    );
    pass_both(&mut state);
    assert!(matches!(state.decision, Some(Decision::Scry { player, .. }) if player == me));
    act(&mut state, me, Action::ScryKeep);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
}

#[test]
fn mayhem_devil_pings_whenever_any_player_sacrifices_even_itself() {
    let (mut state, me, opponent) = empty_board(1);
    let devil = put_on_battlefield(&mut state, me, "mayhem_devil");
    let vampire = put_on_battlefield(&mut state, me, "bloodthrone_vampire");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");

    act(&mut state, me, sacrificing(vampire, bears));
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: me,
            source: devil
        })
    );
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![face(opponent)],
        },
    );
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 19);
    pass_both(&mut state);
    assert_eq!(stats(&state, vampire), (3, 3));

    // Sacrificed itself, it still sees the sacrifice.
    act(&mut state, me, sacrificing(vampire, devil));
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: me,
            source: devil
        })
    );
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![face(opponent)],
        },
    );
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 18);

    // The opponent sacrificing counts too.
    let (mut state, me, opponent) = empty_board(2);
    put_on_battlefield(&mut state, me, "mayhem_devil");
    let theirs = put_on_battlefield(&mut state, opponent, "viscera_seer");
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, sacrificing(theirs, theirs));
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { player, .. }) if player == me
    ));
}

#[test]
fn an_activated_ability_can_be_answered_before_it_resolves() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 1);
    let pyromancer = put_on_battlefield(&mut state, me, "prodigal_pyromancer");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, activate_at(pyromancer, face(opponent)));
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(pyromancer)));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(pyromancer));
    // The ability is already on the stack: it still resolves.
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 19);
}
