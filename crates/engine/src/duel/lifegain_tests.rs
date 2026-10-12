//! Tests for the lifegain package of the green-white pool, and for Conclave
//! Mentor's "that many plus one" counters.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{
    attack, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};
use crate::cards::Keyword;

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

/// Passes priority until the stack is empty.
fn settle(state: &mut DuelState) -> Vec<GameEvent> {
    let mut events = pass_both(state);
    for _ in 0..40 {
        if state.stack.is_empty() {
            break;
        }
        events.extend(pass_both(state));
    }
    events
}

fn stats(state: &DuelState, creature: ObjectId) -> (i32, i32) {
    (
        state.power(creature).unwrap(),
        state.toughness(creature).unwrap(),
    )
}

fn counters(state: &DuelState, creature: ObjectId) -> u32 {
    state.objects[&creature].plus_counters
}

fn named(state: &DuelState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().name == name)
        .map(|permanent| permanent.id)
        .collect()
}

/// The player gains life, as an effect would give it.
fn gain(state: &mut DuelState, player: PlayerId, amount: i32) {
    let mut events = Vec::new();
    super::damage::change_life(
        state,
        &mut events,
        player,
        amount,
        super::damage::LifeCause::Other,
    )
    .unwrap();
    // What that triggers goes on the stack at once, as it would when a
    // player next got priority.
    super::stack::put_triggers_on_stack(state, &mut events).unwrap();
}

/// The activated abilities the player could use now.
fn activations(state: &DuelState, player: PlayerId) -> Vec<Action> {
    legal_actions(state, player)
        .into_iter()
        .filter(|action| matches!(action, Action::ActivateAbility { .. }))
        .collect()
}

// ---- Conclave Mentor ----

#[test]
fn conclave_mentor_adds_one_to_every_batch_of_counters_on_my_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    put_lands(&mut state, opponent, "forest", 1);
    let mentor = put_on_battlefield(&mut state, me, "conclave_mentor");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "grizzly_bears");

    // One counter comes as two.
    let veil = put_in_hand(&mut state, me, "snakeskin_veil");
    act(&mut state, me, cast_at(veil, at(bears)));
    settle(&mut state);
    assert_eq!(counters(&state, bears), 2);
    assert_eq!(state.player(me).counters_put_this_turn, 2);

    // Add one (two with the Mentor), then double: four more and one.
    let surge = put_in_hand(&mut state, me, "invigorating_surge");
    act(&mut state, me, cast_at(surge, at(bears)));
    settle(&mut state);
    assert_eq!(counters(&state, bears), 9);

    // "Enters with four" is five.
    let hydra = put_on_battlefield(&mut state, me, "kalonian_hydra");
    assert_eq!(counters(&state, hydra), 5);

    // The opponent's creatures get nothing extra, even from my spell.
    act(&mut state, me, Action::PassPriority);
    let theirs_veil = put_in_hand(&mut state, opponent, "snakeskin_veil");
    act(&mut state, opponent, cast_at(theirs_veil, at(theirs)));
    settle(&mut state);
    assert_eq!(counters(&state, theirs), 1);
    assert_eq!(counters(&state, mentor), 0);
}

#[test]
fn two_mentors_add_two() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_on_battlefield(&mut state, me, "conclave_mentor");
    put_on_battlefield(&mut state, me, "conclave_mentor");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let veil = put_in_hand(&mut state, me, "snakeskin_veil");
    act(&mut state, me, cast_at(veil, at(bears)));
    settle(&mut state);
    assert_eq!(counters(&state, bears), 3);
}

#[test]
fn conclave_mentor_gives_life_equal_to_its_power_when_it_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_lands(&mut state, opponent, "swamp", 3);
    let mentor = put_on_battlefield(&mut state, me, "conclave_mentor");
    // A counter on itself comes as two: a 4/4.
    let veil = put_in_hand(&mut state, me, "snakeskin_veil");
    act(&mut state, me, cast_at(veil, at(mentor)));
    settle(&mut state);
    assert_eq!(stats(&state, mentor), (4, 4));
    let life = state.player(me).life;

    let turn = state.turn;
    pass_until(&mut state, turn + 1, Step::PrecombatMain);
    let murder = put_in_hand(&mut state, opponent, "murder");
    act(&mut state, opponent, cast_at(murder, at(mentor)));
    settle(&mut state);
    assert!(named(&state, me, "Conclave Mentor").is_empty());
    // Its power as it last was on the battlefield.
    assert_eq!(state.player(me).life, life + 4);
    state.assert_consistent();
}

// ---- The lifegain package ----

#[test]
fn centaur_healer_gains_three_life_and_soulmender_taps_for_one() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "plains", 1);
    let soulmender = put_on_battlefield(&mut state, me, "soulmender");
    let healer = put_in_hand(&mut state, me, "centaur_healer");
    let life = state.player(me).life;
    act(&mut state, me, cast(healer));
    settle(&mut state);
    assert_eq!(state.player(me).life, life + 3);

    let tap = activations(&state, me);
    assert_eq!(tap.len(), 1);
    act(&mut state, me, tap[0].clone());
    settle(&mut state);
    assert_eq!(state.player(me).life, life + 4);
    assert!(state.objects[&soulmender].tapped);
    assert!(activations(&state, me).is_empty());
    assert_eq!(state.player(me).life_gained_this_turn, 4);
}

#[test]
fn celestial_unicorn_grows_each_time_i_gain_life() {
    let (mut state, me, opponent) = empty_board(1);
    let unicorn = put_on_battlefield(&mut state, me, "celestial_unicorn");
    gain(&mut state, me, 3);
    settle(&mut state);
    gain(&mut state, me, 1);
    settle(&mut state);
    assert_eq!(counters(&state, unicorn), 2);
    // Not when the opponent does.
    gain(&mut state, opponent, 2);
    settle(&mut state);
    assert_eq!(counters(&state, unicorn), 2);
}

#[test]
fn unflinching_courage_gives_two_and_two_trample_and_lifelink() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "plains", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let courage = put_in_hand(&mut state, me, "unflinching_courage");
    act(&mut state, me, cast_at(courage, at(bears)));
    settle(&mut state);
    assert_eq!(stats(&state, bears), (4, 4));
    assert!(state.has_keyword(bears, Keyword::Trample).unwrap());
    assert!(state.has_keyword(bears, Keyword::Lifelink).unwrap());

    let life = state.player(me).life;
    attack(&mut state, &[bears]);
    finish_combat(&mut state);
    assert_eq!(state.player(me).life, life + 4);
}

#[test]
fn griffin_aerie_makes_a_griffin_only_after_three_life_in_a_turn() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "griffin_aerie");
    gain(&mut state, me, 2);
    pass_until(&mut state, 1, Step::End);
    settle(&mut state);
    assert!(named(&state, me, "Griffin").is_empty());

    // My next turn: two and one make three.
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(state.player(me).life_gained_this_turn, 0);
    gain(&mut state, me, 2);
    gain(&mut state, me, 1);
    pass_until(&mut state, 3, Step::End);
    settle(&mut state);
    let griffins = named(&state, me, "Griffin");
    assert_eq!(griffins.len(), 1);
    assert_eq!(stats(&state, griffins[0]), (2, 2));
    assert!(state.has_keyword(griffins[0], Keyword::Flying).unwrap());
}

#[test]
fn griffin_aerie_only_looks_at_my_own_end_step() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "griffin_aerie");
    pass_until(&mut state, 2, Step::PrecombatMain);
    gain(&mut state, me, 5);
    pass_until(&mut state, 2, Step::End);
    settle(&mut state);
    assert!(named(&state, me, "Griffin").is_empty());
}

#[test]
fn twinblade_paladin_has_double_strike_at_five_life_above_the_start() {
    let (mut state, me, _) = empty_board(1);
    let paladin = put_on_battlefield(&mut state, me, "twinblade_paladin");
    let start = state.player(me).starting_life;
    assert_eq!(state.player(me).life, start);
    assert!(!state.has_keyword(paladin, Keyword::DoubleStrike).unwrap());
    gain(&mut state, me, 4);
    settle(&mut state);
    assert_eq!(counters(&state, paladin), 1);
    assert!(!state.has_keyword(paladin, Keyword::DoubleStrike).unwrap());
    gain(&mut state, me, 1);
    settle(&mut state);
    assert_eq!(counters(&state, paladin), 2);
    assert!(state.has_keyword(paladin, Keyword::DoubleStrike).unwrap());
    // It goes again if the life does.
    gain(&mut state, me, -1);
    assert!(!state.has_keyword(paladin, Keyword::DoubleStrike).unwrap());

    // The starting total, not a fixed 25: with a higher start it takes more.
    state.players[me.index()].starting_life = start + 3;
    gain(&mut state, me, 1);
    settle(&mut state);
    assert!(!state.has_keyword(paladin, Keyword::DoubleStrike).unwrap());
}

#[test]
fn nyx_fleece_ram_gains_a_life_each_upkeep_of_mine() {
    let (mut state, me, _) = empty_board(1);
    let ram = put_on_battlefield(&mut state, me, "nyx_fleece_ram");
    assert_eq!(stats(&state, ram), (0, 5));
    let life = state.player(me).life;
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert_eq!(state.player(me).life, life);
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(state.player(me).life, life + 1);
}

#[test]
fn angelic_chorus_gains_life_equal_to_the_toughness_of_each_creature_that_enters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    put_lands(&mut state, opponent, "mountain", 3);
    put_on_battlefield(&mut state, me, "angelic_chorus");
    let life = state.player(me).life;
    let spider = put_in_hand(&mut state, me, "giant_spider");
    act(&mut state, me, cast(spider));
    settle(&mut state);
    assert_eq!(state.player(me).life, life + 4);

    // If the creature is gone before the ability resolves, its toughness
    // is what it was when it entered.
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    act(&mut state, me, cast(bears));
    pass_both(&mut state);
    let bears = named(&state, me, "Grizzly Bears")[0];
    assert_eq!(state.stack.len(), 1);
    act(&mut state, me, Action::PassPriority);
    let strike = put_in_hand(&mut state, opponent, "lightning_strike");
    act(&mut state, opponent, cast_at(strike, at(bears)));
    settle(&mut state);
    assert!(named(&state, me, "Grizzly Bears").is_empty());
    assert_eq!(state.player(me).life, life + 4 + 2);
}

#[test]
fn speaker_of_the_heavens_makes_an_angel_only_well_above_starting_life_and_as_a_sorcery() {
    let (mut state, me, _) = empty_board(1);
    let speaker = put_on_battlefield(&mut state, me, "speaker_of_the_heavens");
    assert!(state.has_keyword(speaker, Keyword::Lifelink).unwrap());
    assert!(activations(&state, me).is_empty());
    gain(&mut state, me, 6);
    assert!(activations(&state, me).is_empty());
    gain(&mut state, me, 1);
    let make = activations(&state, me);
    assert_eq!(make.len(), 1);

    // Only in my own main phase with an empty stack.
    let mut later = state.clone();
    pass_until(&mut later, 1, Step::BeginCombat);
    assert!(activations(&later, me).is_empty());

    act(&mut state, me, make[0].clone());
    settle(&mut state);
    let angels = named(&state, me, "Angel");
    assert_eq!(angels.len(), 1);
    assert_eq!(stats(&state, angels[0]), (4, 4));
    assert!(state.has_keyword(angels[0], Keyword::Flying).unwrap());
    assert!(!state.has_keyword(angels[0], Keyword::Vigilance).unwrap());
}

#[test]
fn resplendent_angel_makes_an_angel_at_any_end_step_after_five_life() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 6);
    let angel = put_on_battlefield(&mut state, me, "resplendent_angel");
    gain(&mut state, me, 4);
    pass_until(&mut state, 1, Step::End);
    settle(&mut state);
    assert_eq!(named(&state, me, "Angel").len(), 0);

    // In the opponent's turn too.
    pass_until(&mut state, 2, Step::PrecombatMain);
    gain(&mut state, me, 5);
    pass_until(&mut state, 2, Step::End);
    settle(&mut state);
    let tokens = named(&state, me, "Angel");
    assert_eq!(tokens.len(), 1);
    assert!(state.has_keyword(tokens[0], Keyword::Vigilance).unwrap());

    // Six mana: +2/+2 and lifelink until end of turn.
    pass_until(&mut state, 3, Step::PrecombatMain);
    let pump = activations(&state, me);
    assert_eq!(pump.len(), 1);
    act(&mut state, me, pump[0].clone());
    settle(&mut state);
    assert_eq!(stats(&state, angel), (5, 5));
    assert!(state.has_keyword(angel, Keyword::Lifelink).unwrap());
    pass_until(&mut state, 4, Step::PrecombatMain);
    assert_eq!(stats(&state, angel), (3, 3));
    assert!(!state.has_keyword(angel, Keyword::Lifelink).unwrap());
}

#[test]
fn an_ability_whose_condition_stops_holding_does_nothing_when_it_resolves() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "griffin_aerie");
    gain(&mut state, me, 3);
    pass_until(&mut state, 1, Step::End);
    assert_eq!(state.stack.len(), 1);
    // As if the life gained had been undone before it resolved.
    state.players[me.index()].life_gained_this_turn = 0;
    settle(&mut state);
    assert!(named(&state, me, "Griffin").is_empty());
}
