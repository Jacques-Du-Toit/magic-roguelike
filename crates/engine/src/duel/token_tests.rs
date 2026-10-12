//! Tests for creature tokens and the M4a cards that make them.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::{apply_action, player_view};
use crate::cards::{CardDefId, Color, Keyword};
use crate::data;

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

/// The names of the tokens `player` controls, in the order they entered.
fn tokens(state: &DuelState, player: PlayerId) -> Vec<&str> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().token)
        .map(|permanent| state.def(permanent.id).unwrap().name.as_str())
        .collect()
}

fn token_ids(state: &DuelState, player: PlayerId) -> Vec<ObjectId> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().token)
        .map(|permanent| permanent.id)
        .collect()
}

fn created(events: &[GameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, GameEvent::TokenCreated { .. }))
        .count()
}

#[test]
fn token_definitions_are_creatures_with_a_colour_and_no_cost() {
    let cards = &data::embedded().unwrap().cards;
    for (id, color, power, toughness) in [
        ("soldier_token", Color::W, 1, 1),
        ("goblin_token", Color::R, 1, 1),
        ("centaur_token", Color::G, 3, 3),
        ("zombie_token", Color::B, 2, 2),
        ("snake_token", Color::G, 1, 1),
        ("wolf_token", Color::G, 2, 2),
        ("elephant_token", Color::G, 3, 3),
    ] {
        let token = cards.get(&CardDefId(id.to_string())).unwrap();
        assert!(token.token && token.is_creature(), "{id}");
        assert!(token.mana_cost.is_empty(), "{id}");
        assert_eq!(token.colors, [color], "{id}");
        assert_eq!(
            (token.power, token.toughness),
            (Some(power), Some(toughness)),
            "{id}"
        );
    }
    // A card file is never a token.
    assert!(
        !cards
            .get(&CardDefId("grizzly_bears".to_string()))
            .unwrap()
            .token
    );
}

#[test]
fn raise_the_alarm_creates_two_soldiers_at_instant_speed() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "plains", 2);
    let alarm = put_in_hand(&mut state, opponent, "raise_the_alarm");
    // In my turn, the opponent makes two blockers.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast(alarm));
    let events = pass_both(&mut state);
    assert_eq!(created(&events), 2);
    assert_eq!(tokens(&state, opponent), ["Soldier", "Soldier"]);
    let ids = token_ids(&state, opponent);
    // Each is its own card, and arrives summoning sick.
    assert_ne!(state.objects[&ids[0]].card, state.objects[&ids[1]].card);
    assert!(state.objects[&ids[0]].summoning_sick);
    assert_eq!(
        (
            state.power(ids[0]).unwrap(),
            state.toughness(ids[0]).unwrap()
        ),
        (1, 1)
    );
    let view = player_view(&state, me).unwrap();
    assert_eq!(
        view.battlefield
            .iter()
            .filter(|permanent| permanent.def.as_str() == "soldier_token")
            .count(),
        2
    );
    state.assert_consistent();
}

#[test]
fn tokens_attack_block_and_die_like_any_creature_and_then_cease_to_exist() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let fodder = put_in_hand(&mut state, me, "dragon_fodder");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    act(&mut state, me, cast(fodder));
    pass_both(&mut state);
    assert_eq!(tokens(&state, me), ["Goblin", "Goblin"]);

    // Next turn of mine they can attack; one is blocked and dies.
    pass_until(&mut state, 3, Step::PrecombatMain);
    let goblins = token_ids(&state, me);
    attack(&mut state, &goblins);
    block(&mut state, &[(bears, goblins[0])]);
    let events = finish_combat(&mut state);
    assert!(events.iter().any(|event| matches!(
        event,
        GameEvent::CreatureDied { permanent, .. } if *permanent == goblins[0]
    )));
    assert_eq!(state.player(opponent).life, 19);
    assert_eq!(tokens(&state, me), ["Goblin"]);
    // The dead token is in no graveyard: it is gone.
    assert!(
        state
            .player(me)
            .graveyard
            .iter()
            .all(|id| { state.objects[id].def.as_str() != "goblin_token" })
    );
    assert_eq!(
        state
            .objects
            .values()
            .filter(|object| object.def.as_str() == "goblin_token")
            .count(),
        1
    );
    assert!(state.stray_tokens.is_empty());
    state.assert_consistent();
}

#[test]
fn a_token_returned_to_hand_ceases_to_exist() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_lands(&mut state, me, "plains", 1);
    put_lands(&mut state, opponent, "island", 1);
    let call = put_in_hand(&mut state, me, "call_of_the_conclave");
    let unsummon = put_in_hand(&mut state, opponent, "unsummon");
    act(&mut state, me, cast(call));
    pass_both(&mut state);
    assert_eq!(tokens(&state, me), ["Centaur"]);
    let centaur = token_ids(&state, me)[0];

    act(&mut state, me, Action::PassPriority);
    act(
        &mut state,
        opponent,
        cast_at(unsummon, Target::Object { object: centaur }),
    );
    let events = pass_both(&mut state);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameEvent::PermanentReturnedToHand { .. }))
    );
    assert!(tokens(&state, me).is_empty());
    assert!(state.player(me).hand.is_empty());
    assert!(
        state
            .objects
            .values()
            .all(|object| object.def.as_str() != "centaur_token")
    );
    state.assert_consistent();
}

#[test]
fn creatures_that_arrive_with_a_token_bring_it_when_they_enter() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    put_lands(&mut state, me, "mountain", 2);
    let knight = put_in_hand(&mut state, me, "attended_knight");
    let instigator = put_in_hand(&mut state, me, "goblin_instigator");
    act(&mut state, me, cast(knight));
    pass_both(&mut state);
    assert!(tokens(&state, me).is_empty());
    assert_eq!(created(&pass_both(&mut state)), 1);
    act(&mut state, me, cast(instigator));
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(tokens(&state, me), ["Soldier", "Goblin"]);
}

#[test]
fn doomed_dissenter_leaves_a_zombie_behind() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 1);
    let dissenter = put_on_battlefield(&mut state, me, "doomed_dissenter");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, Action::PassPriority);
    act(
        &mut state,
        opponent,
        cast_at(shock, Target::Object { object: dissenter }),
    );
    pass_both(&mut state);
    assert!(tokens(&state, me).is_empty());
    pass_both(&mut state);
    assert_eq!(tokens(&state, me), ["Zombie"]);
    let zombie = token_ids(&state, me)[0];
    assert_eq!(
        (
            state.power(zombie).unwrap(),
            state.toughness(zombie).unwrap()
        ),
        (2, 2)
    );
}

#[test]
fn bestial_menace_creates_a_snake_a_wolf_and_an_elephant() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 5);
    let menace = put_in_hand(&mut state, me, "bestial_menace");
    act(&mut state, me, cast(menace));
    let events = pass_both(&mut state);
    assert_eq!(created(&events), 3);
    assert_eq!(tokens(&state, me), ["Snake", "Wolf", "Elephant"]);
}

#[test]
fn a_token_entering_and_dying_sets_off_abilities_like_any_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_lands(&mut state, opponent, "mountain", 1);
    put_on_battlefield(&mut state, me, "cruel_celebrant");
    let alarm = put_in_hand(&mut state, me, "raise_the_alarm");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, cast(alarm));
    pass_both(&mut state);
    let soldier = token_ids(&state, me)[0];
    act(&mut state, me, Action::PassPriority);
    act(
        &mut state,
        opponent,
        cast_at(shock, Target::Object { object: soldier }),
    );
    pass_both(&mut state);
    pass_both(&mut state);
    // The Celebrant drains for the dead Soldier.
    assert_eq!(
        (state.player(me).life, state.player(opponent).life),
        (21, 19)
    );
}

// ---- The go-wide package ----

/// Passes priority until the stack is empty.
fn resolve_all(state: &mut DuelState) -> Vec<GameEvent> {
    let mut events = pass_both(state);
    for _ in 0..20 {
        if state.stack.is_empty() {
            break;
        }
        events.extend(pass_both(state));
    }
    events
}

fn stats(state: &DuelState, id: ObjectId) -> (i32, i32) {
    (state.power(id).unwrap(), state.toughness(id).unwrap())
}

#[test]
fn two_tokens_may_share_a_name() {
    let cards = &data::embedded().unwrap().cards;
    let plain = cards.get(&CardDefId("soldier_token".to_string())).unwrap();
    let lifelink = cards
        .get(&CardDefId("soldier_lifelink_token".to_string()))
        .unwrap();
    assert_eq!(plain.name, lifelink.name);
    assert!(lifelink.has_keyword(Keyword::Lifelink) && !plain.has_keyword(Keyword::Lifelink));
}

#[test]
fn captains_call_creates_three_soldiers() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    let call = put_in_hand(&mut state, me, "captains_call");
    act(&mut state, me, cast(call));
    let events = resolve_all(&mut state);
    assert_eq!(created(&events), 3);
    assert_eq!(tokens(&state, me), ["Soldier", "Soldier", "Soldier"]);
    state.assert_consistent();
}

#[test]
fn sworn_companions_creates_two_soldiers_with_lifelink() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    let companions = put_in_hand(&mut state, me, "sworn_companions");
    act(&mut state, me, cast(companions));
    resolve_all(&mut state);
    let ids = token_ids(&state, me);
    assert_eq!(ids.len(), 2);
    for id in ids {
        assert_eq!(stats(&state, id), (1, 1));
        assert!(state.has_keyword(id, Keyword::Lifelink).unwrap());
    }
}

#[test]
fn impassioned_orator_gains_a_life_for_each_other_creature_that_enters() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    let orator = put_in_hand(&mut state, me, "impassioned_orator");
    let alarm = put_in_hand(&mut state, me, "raise_the_alarm");
    let life = state.player(me).life;
    act(&mut state, me, cast(orator));
    resolve_all(&mut state);
    // Not for itself.
    assert_eq!(state.player(me).life, life);
    act(&mut state, me, cast(alarm));
    resolve_all(&mut state);
    assert_eq!(state.player(me).life, life + 2);
}

#[test]
fn good_fortune_unicorn_puts_a_counter_on_each_other_creature_that_enters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "plains", 2);
    let unicorn = put_on_battlefield(&mut state, me, "good_fortune_unicorn");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let alarm = put_in_hand(&mut state, me, "raise_the_alarm");
    act(&mut state, me, cast(bears));
    resolve_all(&mut state);
    let bears = state.creatures_of(me).unwrap()[1];
    assert_eq!(state.objects[&bears].plus_counters, 1);
    assert_eq!(stats(&state, bears), (3, 3));
    // Tokens too, and never itself.
    act(&mut state, me, cast(alarm));
    resolve_all(&mut state);
    for token in token_ids(&state, me) {
        assert_eq!(stats(&state, token), (2, 2));
    }
    assert_eq!(state.objects[&unicorn].plus_counters, 0);
    // Not the opponent's creatures.
    let theirs = put_in_hand(&mut state, opponent, "raging_goblin");
    put_lands(&mut state, opponent, "mountain", 1);
    let turn = state.turn;
    pass_until(&mut state, turn + 1, Step::PrecombatMain);
    act(&mut state, opponent, cast(theirs));
    resolve_all(&mut state);
    let goblin = state.creatures_of(opponent).unwrap()[0];
    assert_eq!(state.objects[&goblin].plus_counters, 0);
}

#[test]
fn trostanis_summoner_brings_a_knight_a_centaur_and_a_rhino() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    put_lands(&mut state, me, "plains", 3);
    let summoner = put_in_hand(&mut state, me, "trostanis_summoner");
    act(&mut state, me, cast(summoner));
    let events = resolve_all(&mut state);
    assert_eq!(created(&events), 3);
    assert_eq!(tokens(&state, me), ["Knight", "Centaur", "Rhino"]);
    let ids = token_ids(&state, me);
    assert_eq!(stats(&state, ids[0]), (2, 2));
    assert!(state.has_keyword(ids[0], Keyword::Vigilance).unwrap());
    assert_eq!(stats(&state, ids[1]), (3, 3));
    assert_eq!(stats(&state, ids[2]), (4, 4));
    assert!(state.has_keyword(ids[2], Keyword::Trample).unwrap());
}

#[test]
fn fists_of_ironwood_gives_trample_and_two_saprolings() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let fists = put_in_hand(&mut state, me, "fists_of_ironwood");
    assert!(!state.has_keyword(bears, Keyword::Trample).unwrap());
    act(
        &mut state,
        me,
        cast_at(fists, Target::Object { object: bears }),
    );
    let events = resolve_all(&mut state);
    assert_eq!(created(&events), 2);
    assert_eq!(tokens(&state, me), ["Saproling", "Saproling"]);
    assert!(state.has_keyword(bears, Keyword::Trample).unwrap());
    // Only the enchanted creature.
    let saproling = token_ids(&state, me)[0];
    assert_eq!(stats(&state, saproling), (1, 1));
    assert!(!state.has_keyword(saproling, Keyword::Trample).unwrap());
    state.assert_consistent();
}

#[test]
fn intangible_virtue_helps_tokens_and_nothing_else() {
    let (mut state, me, opponent) = empty_board(1);
    let mine = put_on_battlefield(&mut state, me, "soldier_token");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "goblin_token");
    assert_eq!(stats(&state, mine), (1, 1));
    put_on_battlefield(&mut state, me, "intangible_virtue");
    assert_eq!(stats(&state, mine), (2, 2));
    assert!(state.has_keyword(mine, Keyword::Vigilance).unwrap());
    assert_eq!(stats(&state, bears), (2, 2));
    assert!(!state.has_keyword(bears, Keyword::Vigilance).unwrap());
    assert_eq!(stats(&state, theirs), (1, 1));
    assert!(!state.has_keyword(theirs, Keyword::Vigilance).unwrap());
}

#[test]
fn inspired_charge_pumps_my_creatures_until_end_of_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let soldier = put_on_battlefield(&mut state, me, "soldier_token");
    let theirs = put_on_battlefield(&mut state, opponent, "raging_goblin");
    let charge = put_in_hand(&mut state, me, "inspired_charge");
    act(&mut state, me, cast(charge));
    resolve_all(&mut state);
    assert_eq!(stats(&state, bears), (4, 3));
    assert_eq!(stats(&state, soldier), (3, 2));
    assert_eq!(stats(&state, theirs), (1, 1));
    // A creature that arrives afterwards gets nothing.
    let late = put_on_battlefield(&mut state, me, "glory_seeker");
    assert_eq!(stats(&state, late), (2, 2));
    let turn = state.turn;
    pass_until(&mut state, turn + 1, Step::PrecombatMain);
    assert_eq!(stats(&state, bears), (2, 2));
    assert_eq!(stats(&state, soldier), (1, 1));
}

#[test]
fn might_of_the_masses_counts_my_creatures_when_it_resolves() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, me, "soldier_token");
    put_on_battlefield(&mut state, me, "soldier_token");
    put_on_battlefield(&mut state, opponent, "raging_goblin");
    let might = put_in_hand(&mut state, me, "might_of_the_masses");
    act(
        &mut state,
        me,
        cast_at(might, Target::Object { object: bears }),
    );
    resolve_all(&mut state);
    // Three creatures of mine; the opponent's doesn't count.
    assert_eq!(stats(&state, bears), (5, 5));
}

#[test]
fn overrun_gives_three_and_three_and_trample() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 5);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let soldier = put_on_battlefield(&mut state, me, "soldier_token");
    let theirs = put_on_battlefield(&mut state, opponent, "raging_goblin");
    let overrun = put_in_hand(&mut state, me, "overrun");
    act(&mut state, me, cast(overrun));
    resolve_all(&mut state);
    for id in [bears, soldier] {
        assert!(state.has_keyword(id, Keyword::Trample).unwrap());
    }
    assert_eq!(stats(&state, bears), (5, 5));
    assert_eq!(stats(&state, soldier), (4, 4));
    assert_eq!(stats(&state, theirs), (1, 1));
    assert!(!state.has_keyword(theirs, Keyword::Trample).unwrap());
    let turn = state.turn;
    pass_until(&mut state, turn + 1, Step::PrecombatMain);
    assert_eq!(stats(&state, bears), (2, 2));
    assert!(!state.has_keyword(bears, Keyword::Trample).unwrap());
}

#[test]
fn collective_blessing_gives_my_creatures_three_and_three() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "raging_goblin");
    put_on_battlefield(&mut state, me, "collective_blessing");
    assert_eq!(stats(&state, bears), (5, 5));
    assert_eq!(stats(&state, theirs), (1, 1));
}
