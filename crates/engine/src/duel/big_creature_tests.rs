//! Tests for the big-creatures package of the green-white pool.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step, Zone};
use super::test_support::{
    attack, cast, cast_at, empty_board, pass_both, pass_until, pass_until_other_decision,
    put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};
use crate::cards::{CardDefId, Keyword};

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

/// Casts a card from hand and lets everything resolve.
fn play(state: &mut DuelState, player: PlayerId, card: &str) {
    let card = put_in_hand(state, player, card);
    act(state, player, cast(card));
    settle(state);
}

#[test]
fn honey_mammoth_and_pelakka_wurm_gain_life_and_the_wurm_draws_when_it_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 13);
    put_lands(&mut state, opponent, "swamp", 3);
    let life = state.player(me).life;
    play(&mut state, me, "honey_mammoth");
    assert_eq!(state.player(me).life, life + 4);
    play(&mut state, me, "pelakka_wurm");
    assert_eq!(state.player(me).life, life + 11);
    let wurm = named(&state, me, "Pelakka Wurm")[0];
    assert_eq!(stats(&state, wurm), (7, 7));
    assert!(state.has_keyword(wurm, Keyword::Trample).unwrap());

    assert_eq!(state.player(me).hand.len(), 0);
    act(&mut state, me, Action::PassPriority);
    let murder = put_in_hand(&mut state, opponent, "murder");
    act(&mut state, opponent, cast_at(murder, at(wurm)));
    settle(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn rampant_growth_fetches_a_tapped_basic_of_the_colour_i_have_least_of() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    // With only Forests out, the land that comes is of another colour:
    // a Plains from deck A, or a Mountain from deck C, whichever is mine.
    let library = state.player(me).library.len();
    play(&mut state, me, "rampant_growth");
    let fetched = state.permanents_of(me).last().unwrap();
    let name = state.def(fetched.id).unwrap().name.as_str();
    assert!(name == "Plains" || name == "Mountain", "{name}");
    assert!(fetched.tapped);
    assert_eq!(state.permanents_of(me).count(), 3);
    assert_eq!(state.player(me).library.len(), library - 1);
    // It is not the turn's land drop.
    assert_eq!(state.player(me).lands_played_this_turn, 0);
    state.assert_consistent();

    // The same seed fetches and shuffles the same way.
    let again = |seed| {
        let (mut state, me, _) = empty_board(seed);
        put_lands(&mut state, me, "forest", 2);
        play(&mut state, me, "rampant_growth");
        state.player(me).library.clone()
    };
    assert_eq!(again(4), again(4));
}

#[test]
fn rampant_growth_only_shuffles_when_no_basic_land_is_left() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    for card in state.player(me).library.clone() {
        if state.def(card).unwrap().is_land() {
            state.move_object(card, Zone::Graveyard).unwrap();
        }
    }
    let library = state.player(me).library.len();
    play(&mut state, me, "rampant_growth");
    assert_eq!(state.player(me).library.len(), library);
    assert_eq!(state.permanents_of(me).count(), 2);
}

#[test]
fn garruks_uprising_draws_for_big_creatures_and_gives_trample() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 12);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    // With nothing of power 4, it draws nothing as it enters.
    play(&mut state, me, "garruks_uprising");
    assert_eq!(state.player(me).hand.len(), 0);
    assert!(state.has_keyword(bears, Keyword::Trample).unwrap());

    // A 2/2 entering draws nothing; a 4/4 draws a card.
    play(&mut state, me, "grizzly_bears");
    assert_eq!(state.player(me).hand.len(), 0);
    play(&mut state, me, "rumbling_baloth");
    assert_eq!(state.player(me).hand.len(), 1);

    // A second Uprising finds the Baloth and draws as it enters.
    play(&mut state, me, "garruks_uprising");
    assert_eq!(state.player(me).hand.len(), 2);

    // The opponent's creatures have nothing to do with it.
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    assert!(!state.has_keyword(theirs, Keyword::Trample).unwrap());
}

#[test]
fn nessian_hornbeetle_grows_at_combat_beside_a_big_creature() {
    let (mut state, me, _) = empty_board(1);
    let beetle = put_on_battlefield(&mut state, me, "nessian_hornbeetle");
    pass_until(&mut state, 1, Step::BeginCombat);
    settle(&mut state);
    assert_eq!(state.objects[&beetle].plus_counters, 0);

    put_on_battlefield(&mut state, me, "rumbling_baloth");
    pass_until(&mut state, 3, Step::BeginCombat);
    settle(&mut state);
    assert_eq!(state.objects[&beetle].plus_counters, 1);

    // Its own power doesn't count: "another creature".
    let (mut state, me, _) = empty_board(1);
    let beetle = put_on_battlefield(&mut state, me, "nessian_hornbeetle");
    state.objects.get_mut(&beetle).unwrap().plus_counters = 3;
    pass_until(&mut state, 1, Step::BeginCombat);
    settle(&mut state);
    assert_eq!(state.objects[&beetle].plus_counters, 3);
}

#[test]
fn sight_of_the_scalelords_pumps_my_tough_creatures_each_combat() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "sight_of_the_scalelords");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let spider = put_on_battlefield(&mut state, me, "giant_spider");
    pass_until(&mut state, 1, Step::BeginCombat);
    pass_until_other_decision(&mut state);
    assert_eq!(stats(&state, spider), (4, 6));
    assert!(state.has_keyword(spider, Keyword::Vigilance).unwrap());
    assert_eq!(stats(&state, bears), (2, 2));
    assert!(!state.has_keyword(bears, Keyword::Vigilance).unwrap());
    assert!(matches!(
        state.decision,
        Some(Decision::DeclareAttackers { .. })
    ));
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert_eq!(stats(&state, spider), (2, 4));
}

#[test]
fn thragtusk_gains_five_and_leaves_a_beast_however_it_leaves() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 5);
    put_lands(&mut state, opponent, "island", 3);
    let life = state.player(me).life;
    play(&mut state, me, "thragtusk");
    assert_eq!(state.player(me).life, life + 5);
    let tusk = named(&state, me, "Thragtusk")[0];

    // Bounced, not killed: it still leaves a Beast behind.
    let mut events = Vec::new();
    state.move_object(tusk, Zone::Hand).unwrap();
    super::stack::put_triggers_on_stack(&mut state, &mut events).unwrap();
    settle(&mut state);
    let beasts = named(&state, me, "Beast");
    assert_eq!(beasts.len(), 1);
    assert_eq!(stats(&state, beasts[0]), (3, 3));
    assert!(named(&state, me, "Thragtusk").is_empty());

    // And when it dies.
    let tusk = put_on_battlefield(&mut state, me, "thragtusk");
    act(&mut state, me, Action::PassPriority);
    put_lands(&mut state, opponent, "swamp", 3);
    let murder = put_in_hand(&mut state, opponent, "murder");
    act(&mut state, opponent, cast_at(murder, at(tusk)));
    settle(&mut state);
    assert_eq!(named(&state, me, "Beast").len(), 2);
    state.assert_consistent();
}

#[test]
fn armada_wurm_brings_a_second_wurm() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    put_lands(&mut state, me, "plains", 2);
    play(&mut state, me, "armada_wurm");
    let tokens = named(&state, me, "Wurm");
    assert_eq!(tokens.len(), 1);
    assert_eq!(stats(&state, tokens[0]), (5, 5));
    assert!(state.has_keyword(tokens[0], Keyword::Trample).unwrap());
    assert_eq!(named(&state, me, "Armada Wurm").len(), 1);
}

#[test]
fn steel_leaf_champion_cannot_be_blocked_by_small_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    let champion = put_on_battlefield(&mut state, me, "steel_leaf_champion");
    let small = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let big = put_on_battlefield(&mut state, opponent, "hill_giant");
    attack(&mut state, &[champion]);
    pass_until_other_decision(&mut state);
    let blocks = legal_actions(&state, opponent);
    assert!(blocks.contains(&Action::Block {
        blocker: big,
        attacker: champion
    }));
    assert!(!blocks.contains(&Action::Block {
        blocker: small,
        attacker: champion
    }));
    // A small creature made big enough may block.
    state.objects.get_mut(&small).unwrap().power_bonus = 1;
    assert!(legal_actions(&state, opponent).contains(&Action::Block {
        blocker: small,
        attacker: champion
    }));
}

#[test]
fn terra_stomper_cannot_be_countered() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    put_lands(&mut state, opponent, "island", 3);
    let stomper = put_in_hand(&mut state, me, "terra_stomper");
    let cancel = put_in_hand(&mut state, opponent, "cancel");
    act(&mut state, me, cast(stomper));
    act(&mut state, me, Action::PassPriority);
    let spell = state.stack.last().unwrap().id;
    act(&mut state, opponent, cast_at(cancel, at(spell)));
    settle(&mut state);
    let stompers = named(&state, me, "Terra Stomper");
    assert_eq!(stompers.len(), 1);
    assert_eq!(stats(&state, stompers[0]), (8, 8));
    // The counterspell was spent all the same.
    let graveyard = &state.player(opponent).graveyard;
    assert_eq!(
        state.objects[graveyard.last().unwrap()].def,
        CardDefId("cancel".to_string())
    );
    state.assert_consistent();
}
