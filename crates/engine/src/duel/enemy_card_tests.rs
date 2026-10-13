//! Tests for the cards added for the rebuilt enemy decks of the first act,
//! and the two small rules that came with them: damage to each creature,
//! and a count of the other creatures with flying.

use super::action::{Action, GameEvent, Target};
use super::apply_action;
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
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

/// Casts a card with no targets from hand and lets everything resolve.
fn play(state: &mut DuelState, player: PlayerId, card: &str) {
    let card = put_in_hand(state, player, card);
    act(state, player, cast(card));
    settle(state);
}

#[test]
fn pyroclasm_deals_two_damage_to_each_creature_on_both_sides() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let courser = put_on_battlefield(&mut state, me, "centaur_courser");
    let piker = put_on_battlefield(&mut state, opponent, "goblin_piker");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let life = (state.player(me).life, state.player(opponent).life);
    play(&mut state, me, "pyroclasm");
    // The 2/2 and the 2/1 die; the 3/3s live, with 2 damage on them.
    assert!(!state.is_on_battlefield(bears));
    assert!(!state.is_on_battlefield(piker));
    assert!(state.is_on_battlefield(courser));
    assert!(state.is_on_battlefield(giant));
    assert_eq!(state.object(courser).unwrap().damage, 2);
    assert_eq!(state.object(giant).unwrap().damage, 2);
    // No player is dealt anything.
    assert_eq!((state.player(me).life, state.player(opponent).life), life);
    state.assert_consistent();
}

#[test]
fn lightning_bolt_deals_three_and_lightning_helix_also_gains_three() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_lands(&mut state, me, "plains", 1);
    let life = (state.player(me).life, state.player(opponent).life);
    let bolt = put_in_hand(&mut state, me, "lightning_bolt");
    act(
        &mut state,
        me,
        cast_at(bolt, Target::Player { player: opponent }),
    );
    settle(&mut state);
    assert_eq!(state.player(opponent).life, life.1 - 3);

    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let helix = put_in_hand(&mut state, me, "lightning_helix");
    act(&mut state, me, cast_at(helix, at(giant)));
    settle(&mut state);
    assert!(!state.is_on_battlefield(giant));
    assert_eq!(state.player(me).life, life.0 + 3);
}

#[test]
fn flametongue_kavu_deals_four_to_a_creature_and_to_itself_if_it_is_alone() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 8);
    // Alone on the battlefield, it is the only creature it can aim at.
    play(&mut state, me, "flametongue_kavu");
    assert!(named(&state, me, "Flametongue Kavu").is_empty());

    let baloth = put_on_battlefield(&mut state, opponent, "rumbling_baloth");
    let kavu = put_in_hand(&mut state, me, "flametongue_kavu");
    act(&mut state, me, cast(kavu));
    pass_both(&mut state);
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { player, .. }) if player == me
    ));
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![at(baloth)],
        },
    );
    settle(&mut state);
    assert!(!state.is_on_battlefield(baloth));
    assert_eq!(named(&state, me, "Flametongue Kavu").len(), 1);
}

#[test]
fn borderland_marauder_gets_two_power_when_it_attacks_until_end_of_turn() {
    let (mut state, me, _) = empty_board(1);
    let marauder = put_on_battlefield(&mut state, me, "borderland_marauder");
    assert_eq!(stats(&state, marauder), (1, 2));
    attack(&mut state, &[marauder]);
    settle(&mut state);
    assert_eq!(stats(&state, marauder), (3, 2));
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, marauder), (1, 2));
}

#[test]
fn furnace_whelp_flies_and_breathes_fire_for_red_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let whelp = put_on_battlefield(&mut state, me, "furnace_whelp");
    assert!(state.has_keyword(whelp, Keyword::Flying).unwrap());
    for _ in 0..2 {
        act(
            &mut state,
            me,
            Action::ActivateAbility {
                source: whelp,
                ability: 0,
                targets: Vec::new(),
                sacrifice: None,
            },
        );
        settle(&mut state);
    }
    assert_eq!(stats(&state, whelp), (4, 2));
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, whelp), (2, 2));
}

#[test]
fn volcanic_dragon_and_gifted_aetherborn_have_their_keywords() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 6);
    play(&mut state, me, "volcanic_dragon");
    let dragon = named(&state, me, "Volcanic Dragon")[0];
    assert_eq!(stats(&state, dragon), (4, 4));
    assert!(state.has_keyword(dragon, Keyword::Flying).unwrap());
    // Haste: it attacks the turn it comes.
    let life = state.player(opponent).life;
    attack(&mut state, &[dragon]);
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, life - 4);

    let aetherborn = put_on_battlefield(&mut state, me, "gifted_aetherborn");
    assert_eq!(stats(&state, aetherborn), (2, 3));
    assert!(state.has_keyword(aetherborn, Keyword::Deathtouch).unwrap());
    assert!(state.has_keyword(aetherborn, Keyword::Lifelink).unwrap());
}

#[test]
fn phyrexian_rager_draws_a_card_for_a_life() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let life = state.player(me).life;
    play(&mut state, me, "phyrexian_rager");
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(state.player(me).life, life - 1);
}

#[test]
fn lilianas_specter_makes_the_opponent_discard() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    put_in_hand(&mut state, opponent, "hill_giant");
    put_in_hand(&mut state, me, "grizzly_bears");
    play(&mut state, me, "lilianas_specter");
    assert_eq!(state.player(opponent).hand.len(), 0);
    assert_eq!(state.player(opponent).graveyard.len(), 1);
    // My own hand is left alone.
    assert_eq!(state.player(me).hand.len(), 1);
    let specter = named(&state, me, "Liliana's Specter")[0];
    assert!(state.has_keyword(specter, Keyword::Flying).unwrap());
}

#[test]
fn vampire_sovereign_drains_three() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 5);
    let life = (state.player(me).life, state.player(opponent).life);
    play(&mut state, me, "vampire_sovereign");
    assert_eq!(state.player(me).life, life.0 + 3);
    assert_eq!(state.player(opponent).life, life.1 - 3);
    let sovereign = named(&state, me, "Vampire Sovereign")[0];
    assert!(state.has_keyword(sovereign, Keyword::Flying).unwrap());
}

#[test]
fn eviscerate_destroys_a_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 4);
    let baloth = put_on_battlefield(&mut state, opponent, "rumbling_baloth");
    let eviscerate = put_in_hand(&mut state, me, "eviscerate");
    act(&mut state, me, cast_at(eviscerate, at(baloth)));
    settle(&mut state);
    assert!(!state.is_on_battlefield(baloth));
}

#[test]
fn radiant_grows_for_each_other_creature_with_flying_on_either_side() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let radiant = put_on_battlefield(&mut state, me, "radiant_archangel");
    assert_eq!(stats(&state, radiant), (3, 3));
    // A creature without flying counts for nothing; hers and theirs with
    // flying count one each; she doesn't count herself.
    put_on_battlefield(&mut state, me, "grizzly_bears");
    assert_eq!(stats(&state, radiant), (3, 3));
    put_on_battlefield(&mut state, me, "wind_drake");
    put_on_battlefield(&mut state, opponent, "wind_drake");
    assert_eq!(stats(&state, radiant), (5, 5));
    // Flying that comes from a counter counts while it is there.
    let walker = put_on_battlefield(&mut state, me, "lightwalker");
    assert_eq!(stats(&state, radiant), (5, 5));
    let veil = put_in_hand(&mut state, me, "snakeskin_veil");
    put_lands(&mut state, me, "forest", 1);
    act(&mut state, me, cast_at(veil, at(walker)));
    settle(&mut state);
    assert_eq!(stats(&state, radiant), (6, 6));
    assert!(state.has_keyword(radiant, Keyword::Vigilance).unwrap());
}

#[test]
fn borborygmos_puts_a_counter_on_each_of_its_controllers_creatures_when_it_connects() {
    let (mut state, me, opponent) = empty_board(1);
    let borborygmos = put_on_battlefield(&mut state, me, "borborygmos");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "typhoid_rats");
    attack(&mut state, &[borborygmos]);
    finish_combat(&mut state);
    assert_eq!(stats(&state, borborygmos), (7, 8));
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(stats(&state, theirs), (1, 1));
}
