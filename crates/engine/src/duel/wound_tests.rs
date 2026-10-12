//! Tests for the Festering Wound, the artifact a wound of the run puts on
//! its owner's battlefield, and for "sacrifice this".

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{empty_board, pass_both, pass_until, put_lands, put_on_battlefield};
use super::{apply_action, legal_actions};
use crate::ai_choose_action;
use crate::cards::CardType;

/// Paying a wound off: its second ability.
fn pay_off(wound: ObjectId) -> Action {
    Action::ActivateAbility {
        source: wound,
        ability: 1,
        targets: Vec::new(),
        sacrifice: None,
    }
}

fn wounds(state: &DuelState, player: PlayerId) -> usize {
    state
        .permanents_of(player)
        .filter(|permanent| permanent.def.as_str() == "festering_wound")
        .count()
}

#[test]
fn a_festering_wound_deals_1_damage_to_its_controller_in_each_of_their_end_steps() {
    let (mut state, me, opponent) = empty_board(1);
    let wound = put_on_battlefield(&mut state, me, "festering_wound");
    assert_eq!(state.def(wound).unwrap().types, [CardType::Artifact]);
    // It is not a creature: it can't attack, block or be damaged.
    assert!(!state.is_creature(wound).unwrap());

    let events = pass_until(&mut state, 2, Step::Upkeep);
    assert!(events.contains(&GameEvent::DamageDealt {
        source: wound,
        target: Target::Player { player: me },
        amount: 1
    }));
    assert_eq!(state.player(me).life, 19);
    assert_eq!(state.player(opponent).life, 20);

    // Nothing in the opponent's turn, and again in mine.
    pass_until(&mut state, 3, Step::Upkeep);
    assert_eq!(state.player(me).life, 19);
    pass_until(&mut state, 4, Step::Upkeep);
    assert_eq!(state.player(me).life, 18);
    assert_eq!(wounds(&state, me), 1);
}

#[test]
fn two_mana_sacrifices_a_festering_wound_for_the_rest_of_the_duel() {
    let (mut state, me, _) = empty_board(1);
    let wound = put_on_battlefield(&mut state, me, "festering_wound");
    put_lands(&mut state, me, "forest", 1);
    // One land is not enough.
    assert!(!legal_actions(&state, me).contains(&pay_off(wound)));
    put_lands(&mut state, me, "forest", 1);
    assert!(legal_actions(&state, me).contains(&pay_off(wound)));

    apply_action(&mut state, me, pay_off(wound)).unwrap();
    let events = pass_both(&mut state);
    assert!(events.contains(&GameEvent::PermanentSacrificed {
        player: me,
        permanent: wound
    }));
    assert_eq!(wounds(&state, me), 0);
    // A token: it is gone, not in the graveyard.
    assert!(state.player(me).graveyard.is_empty());

    pass_until(&mut state, 4, Step::Upkeep);
    assert_eq!(state.player(me).life, 20);
    state.assert_consistent();
}

#[test]
fn each_wound_is_paid_off_on_its_own() {
    let (mut state, me, _) = empty_board(1);
    let first = put_on_battlefield(&mut state, me, "festering_wound");
    put_on_battlefield(&mut state, me, "festering_wound");
    put_lands(&mut state, me, "forest", 2);
    apply_action(&mut state, me, pay_off(first)).unwrap();
    pass_both(&mut state);
    assert_eq!(wounds(&state, me), 1);
    // The mana is spent: the other stays and deals its damage.
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(state.player(me).life, 19);
}

#[test]
fn the_ai_pays_a_wound_off_with_mana_it_has_no_other_use_for() {
    let (mut state, me, _) = empty_board(1);
    let wound = put_on_battlefield(&mut state, me, "festering_wound");
    // Without the mana it has nothing to do.
    assert_eq!(ai_choose_action(&state, me), Ok(Action::PassPriority));
    put_lands(&mut state, me, "forest", 2);
    assert_eq!(ai_choose_action(&state, me), Ok(pay_off(wound)));
}
