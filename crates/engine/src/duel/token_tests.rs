//! Tests for creature tokens and the M4a cards that make them.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::{apply_action, player_view};
use crate::cards::{CardDefId, Color};
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
