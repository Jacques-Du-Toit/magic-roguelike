//! Tests for targets with conditions ("nonblack creature", "creature
//! spell", "target player") and the M4a cards that use them.

use super::action::{Action, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::DuelState;
use super::test_support::{
    cast, cast_at, empty_board, pass_both, put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

/// The single targets the player could cast `card` at right now.
fn targets_of(state: &DuelState, player: PlayerId, card: ObjectId) -> Vec<Target> {
    legal_actions(state, player)
        .into_iter()
        .filter_map(|action| match action {
            Action::CastSpell {
                card: cast,
                targets,
            } if cast == card => targets.first().copied(),
            _ => None,
        })
        .collect()
}

fn graveyard(state: &DuelState, player: PlayerId) -> Vec<&str> {
    state
        .player(player)
        .graveyard
        .iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

#[test]
fn essence_scatter_counters_creature_spells_and_negate_the_others() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, opponent, "island", 4);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let shock = put_in_hand(&mut state, me, "shock");
    let scatter = put_in_hand(&mut state, opponent, "essence_scatter");
    let negate = put_in_hand(&mut state, opponent, "negate");

    apply_action(&mut state, me, cast(bears)).unwrap();
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    let bears_spell = state.stack[0].id;
    assert_eq!(targets_of(&state, opponent, scatter), [at(bears_spell)]);
    assert_eq!(targets_of(&state, opponent, negate), []);
    apply_action(&mut state, opponent, cast_at(scatter, at(bears_spell))).unwrap();
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(graveyard(&state, me), ["grizzly_bears"]);

    apply_action(&mut state, me, cast_at(shock, face(opponent))).unwrap();
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    let shock_spell = state.stack[0].id;
    assert_eq!(targets_of(&state, opponent, negate), [at(shock_spell)]);
    apply_action(&mut state, opponent, cast_at(negate, at(shock_spell))).unwrap();
    // Negate is a noncreature spell too, but a spell can't target itself.
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.player(opponent).life, 20);
    assert_eq!(graveyard(&state, me), ["grizzly_bears", "shock"]);
    state.assert_consistent();
}

#[test]
fn lava_axe_deals_5_damage_to_a_player_and_cannot_target_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 5);
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let axe = put_in_hand(&mut state, me, "lava_axe");
    assert_eq!(
        targets_of(&state, me, axe),
        [face(PlayerId::P1), face(PlayerId::P2)]
    );
    apply_action(&mut state, me, cast_at(axe, face(opponent))).unwrap();
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 15);
}

#[test]
fn doom_blade_destroys_a_creature_that_is_not_black() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    put_on_battlefield(&mut state, opponent, "typhoid_rats");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let blade = put_in_hand(&mut state, me, "doom_blade");
    assert_eq!(targets_of(&state, me, blade), [at(bears)]);
    apply_action(&mut state, me, cast_at(blade, at(bears))).unwrap();
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));
}

#[test]
fn smite_the_monstrous_needs_power_4_or_greater() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    put_on_battlefield(&mut state, opponent, "hill_giant");
    let baloth = put_on_battlefield(&mut state, opponent, "rumbling_baloth");
    let smite = put_in_hand(&mut state, me, "smite_the_monstrous");
    assert_eq!(targets_of(&state, me, smite), [at(baloth)]);
    apply_action(&mut state, me, cast_at(smite, at(baloth))).unwrap();
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(baloth));
}

#[test]
fn smite_the_monstrous_counts_power_as_it_is_now_and_checks_again_on_resolution() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    put_lands(&mut state, opponent, "forest", 1);
    put_lands(&mut state, opponent, "swamp", 1);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let smite = put_in_hand(&mut state, me, "smite_the_monstrous");
    let growth = put_in_hand(&mut state, opponent, "giant_growth");
    let disfigure = put_in_hand(&mut state, opponent, "disfigure");
    assert_eq!(targets_of(&state, me, smite), []);

    // Giant Growth makes the Bears a 5/5: now they can be smitten.
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    apply_action(&mut state, opponent, cast_at(growth, at(bears))).unwrap();
    pass_both(&mut state);
    assert_eq!(targets_of(&state, me, smite), [at(bears)]);

    // In response the opponent shrinks them to 3/3: the Smite does nothing.
    apply_action(&mut state, me, cast_at(smite, at(bears))).unwrap();
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    apply_action(&mut state, opponent, cast_at(disfigure, at(bears))).unwrap();
    pass_both(&mut state);
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert!(state.is_on_battlefield(bears) && state.is_on_battlefield(giant));
    assert_eq!(graveyard(&state, me), ["smite_the_monstrous"]);
}

#[test]
fn plummet_destroys_a_creature_with_flying() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let drake = put_on_battlefield(&mut state, opponent, "wind_drake");
    let plummet = put_in_hand(&mut state, me, "plummet");
    assert_eq!(targets_of(&state, me, plummet), [at(drake)]);
    apply_action(&mut state, me, cast_at(plummet, at(drake))).unwrap();
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(drake));
}

#[test]
fn titanic_growth_gives_plus_4_plus_4_until_end_of_turn() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let growth = put_in_hand(&mut state, me, "titanic_growth");
    apply_action(&mut state, me, cast_at(growth, at(bears))).unwrap();
    pass_both(&mut state);
    assert_eq!(
        (state.power(bears).unwrap(), state.toughness(bears).unwrap()),
        (6, 6)
    );
}

#[test]
fn disfigure_kills_a_creature_with_toughness_2_and_shrinks_a_bigger_one() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let first = put_in_hand(&mut state, me, "disfigure");
    let second = put_in_hand(&mut state, me, "disfigure");
    apply_action(&mut state, me, cast_at(first, at(bears))).unwrap();
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));
    assert_eq!(graveyard(&state, opponent), ["grizzly_bears"]);

    apply_action(&mut state, me, cast_at(second, at(giant))).unwrap();
    pass_both(&mut state);
    assert_eq!(
        (state.power(giant).unwrap(), state.toughness(giant).unwrap()),
        (1, 1)
    );
}
