//! Tests for the Auras and enchantments package of the green-white pool.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Zone};
use super::test_support::{
    cast, cast_at, empty_board, pass_both, put_in_hand, put_lands, put_on_battlefield,
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

fn named(state: &DuelState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().name == name)
        .map(|permanent| permanent.id)
        .collect()
}

/// A board with plenty of both colours of mana for `me`.
fn board() -> (DuelState, PlayerId, PlayerId) {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 8);
    put_lands(&mut state, me, "plains", 8);
    (state, me, opponent)
}

/// Casts an Aura from hand onto a creature and lets everything resolve.
fn enchant(state: &mut DuelState, player: PlayerId, aura: &str, creature: ObjectId) {
    let aura = put_in_hand(state, player, aura);
    act(state, player, cast_at(aura, at(creature)));
    settle(state);
}

/// Sends a permanent to its owner's graveyard and puts what that triggers
/// on the stack.
fn bury(state: &mut DuelState, permanent: ObjectId) {
    let mut events = Vec::new();
    state.move_object(permanent, Zone::Graveyard).unwrap();
    super::stack::put_triggers_on_stack(state, &mut events).unwrap();
}

#[test]
fn setessan_training_draws_a_card_and_only_goes_on_my_own_creature() {
    let (mut state, me, opponent) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let training = put_in_hand(&mut state, me, "setessan_training");
    let options = legal_actions(&state, me);
    assert!(options.contains(&cast_at(training, at(bears))));
    assert!(!options.contains(&cast_at(training, at(theirs))));
    act(&mut state, me, cast_at(training, at(bears)));
    settle(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(stats(&state, bears), (3, 2));
    assert!(state.has_keyword(bears, Keyword::Trample).unwrap());
}

#[test]
fn cartouche_of_solidarity_and_knightly_valor_bring_a_token_each() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    enchant(&mut state, me, "cartouche_of_solidarity", bears);
    let warriors = named(&state, me, "Warrior");
    assert_eq!(warriors.len(), 1);
    assert_eq!(stats(&state, warriors[0]), (1, 1));
    assert!(state.has_keyword(warriors[0], Keyword::Vigilance).unwrap());
    assert_eq!(stats(&state, bears), (3, 3));
    assert!(state.has_keyword(bears, Keyword::FirstStrike).unwrap());

    enchant(&mut state, me, "knightly_valor", bears);
    let knights = named(&state, me, "Knight");
    assert_eq!(knights.len(), 1);
    assert_eq!(stats(&state, knights[0]), (2, 2));
    assert_eq!(stats(&state, bears), (5, 5));
    assert!(state.has_keyword(bears, Keyword::Vigilance).unwrap());
}

#[test]
fn divine_favor_gains_three_life_and_gives_one_and_three() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let life = state.player(me).life;
    enchant(&mut state, me, "divine_favor", bears);
    assert_eq!(state.player(me).life, life + 3);
    assert_eq!(stats(&state, bears), (3, 5));
}

#[test]
fn oakheart_dryads_pumps_a_creature_when_it_or_another_enchantment_enters() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    // The Dryads are an enchantment themselves: entering sets them off.
    let dryads = put_in_hand(&mut state, me, "oakheart_dryads");
    act(&mut state, me, cast(dryads));
    pass_both(&mut state);
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { player, .. }) if player == me
    ));
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![at(bears)],
        },
    );
    settle(&mut state);
    assert_eq!(stats(&state, bears), (3, 3));

    // An Aura entering sets them off again; a creature does not.
    let favor = put_in_hand(&mut state, me, "divine_favor");
    act(&mut state, me, cast_at(favor, at(bears)));
    pass_both(&mut state);
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { .. })
    ));
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![at(bears)],
        },
    );
    settle(&mut state);
    assert_eq!(stats(&state, bears), (5, 7));
    let lions = put_in_hand(&mut state, me, "savannah_lions");
    act(&mut state, me, cast(lions));
    settle(&mut state);
    assert_eq!(stats(&state, bears), (5, 7));
}

#[test]
fn ethereal_armor_counts_my_enchantments_and_ancestral_mask_all_the_others() {
    let (mut state, me, opponent) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let spider = put_on_battlefield(&mut state, me, "giant_spider");
    enchant(&mut state, me, "ethereal_armor", bears);
    // Itself: one enchantment of mine.
    assert_eq!(stats(&state, bears), (3, 3));
    assert!(state.has_keyword(bears, Keyword::FirstStrike).unwrap());
    put_on_battlefield(&mut state, me, "glorious_anthem");
    // Two enchantments, and the Anthem's own +1/+1.
    assert_eq!(stats(&state, bears), (5, 5));
    // The opponent's enchantments are not mine.
    put_on_battlefield(&mut state, opponent, "glorious_anthem");
    assert_eq!(stats(&state, bears), (5, 5));

    // The Mask counts every other enchantment, the opponent's included:
    // the Armor and two Anthems. With my Anthem, 2/4 becomes 9/11.
    enchant(&mut state, me, "ancestral_mask", spider);
    assert_eq!(stats(&state, spider), (2 + 6 + 1, 4 + 6 + 1));
    // And the Mask is one more enchantment for the Armor.
    assert_eq!(stats(&state, bears), (6, 6));
}

#[test]
fn satyr_enchanter_draws_and_the_sigil_makes_an_angel_for_each_enchantment_cast() {
    let (mut state, me, opponent) = board();
    put_lands(&mut state, opponent, "plains", 3);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, me, "satyr_enchanter");
    put_on_battlefield(&mut state, me, "sigil_of_the_empty_throne");
    enchant(&mut state, me, "divine_favor", bears);
    assert_eq!(state.player(me).hand.len(), 1);
    let angels = named(&state, me, "Angel");
    assert_eq!(angels.len(), 1);
    assert_eq!(stats(&state, angels[0]), (4, 4));

    // A creature spell is not an enchantment; an enchantment creature is.
    let lions = put_in_hand(&mut state, me, "savannah_lions");
    act(&mut state, me, cast(lions));
    settle(&mut state);
    assert_eq!(named(&state, me, "Angel").len(), 1);
    let ram = put_in_hand(&mut state, me, "nyx_fleece_ram");
    act(&mut state, me, cast(ram));
    settle(&mut state);
    assert_eq!(named(&state, me, "Angel").len(), 2);

    // Nor does an enchantment the opponent casts, in their own turn.
    let hand = state.player(me).hand.len();
    super::test_support::pass_until(&mut state, 2, super::state::Step::PrecombatMain);
    let drawn = state.player(me).hand.len() - hand;
    let theirs = put_in_hand(&mut state, opponent, "glorious_anthem");
    act(&mut state, opponent, cast(theirs));
    settle(&mut state);
    assert_eq!(named(&state, opponent, "Glorious Anthem").len(), 1);
    assert_eq!(named(&state, me, "Angel").len(), 2);
    assert_eq!(state.player(me).hand.len(), hand + drawn);
}

#[test]
fn setessan_champion_grows_and_draws_whenever_an_enchantment_of_mine_enters() {
    let (mut state, me, _) = board();
    let champion = put_on_battlefield(&mut state, me, "setessan_champion");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    enchant(&mut state, me, "divine_favor", bears);
    assert_eq!(state.objects[&champion].plus_counters, 1);
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(stats(&state, champion), (2, 4));
}

#[test]
fn rancor_comes_back_to_hand_when_it_goes_to_the_graveyard() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    enchant(&mut state, me, "rancor", bears);
    assert_eq!(stats(&state, bears), (4, 2));
    assert!(state.has_keyword(bears, Keyword::Trample).unwrap());
    assert_eq!(state.player(me).hand.len(), 0);

    // The creature dies, and the Aura follows it to the graveyard.
    let mut events = Vec::new();
    super::damage::creature_dies(&mut state, &mut events, bears).unwrap();
    settle(&mut state);
    settle(&mut state);
    let hand = &state.player(me).hand;
    assert_eq!(hand.len(), 1);
    assert_eq!(state.def(hand[0]).unwrap().name, "Rancor");
    assert!(
        state
            .player(me)
            .graveyard
            .iter()
            .all(|card| state.def(*card).unwrap().name != "Rancor")
    );
    state.assert_consistent();
}

#[test]
fn mantle_of_the_wolf_gives_four_and_four_and_leaves_two_wolves() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    enchant(&mut state, me, "mantle_of_the_wolf", bears);
    assert_eq!(stats(&state, bears), (6, 6));
    let mantle = named(&state, me, "Mantle of the Wolf")[0];
    bury(&mut state, mantle);
    settle(&mut state);
    assert_eq!(named(&state, me, "Wolf").len(), 2);
    assert_eq!(stats(&state, bears), (2, 2));
}
