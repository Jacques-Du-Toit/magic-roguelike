//! Tests for targets in a graveyard, mill, surveil and "you may" abilities,
//! and for the M4a cards that use them.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Zone};
use super::test_support::{
    cast, cast_at, empty_board, pass_both, put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions, player_view, redact_events};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn aim(targets: &[Target]) -> Action {
    Action::ChooseTargets {
        targets: targets.to_vec(),
    }
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

fn names<'a>(state: &'a DuelState, zone: &[ObjectId]) -> Vec<&'a str> {
    zone.iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

/// Puts a card straight into a player's graveyard.
fn put_in_graveyard(state: &mut DuelState, player: PlayerId, card: &str) -> ObjectId {
    let in_hand = put_in_hand(state, player, card);
    state.move_object(in_hand, Zone::Graveyard).unwrap()
}

/// Puts `cards` on top of the player's library, the last on top.
fn stack_library(state: &mut DuelState, player: PlayerId, cards: &[&str]) {
    for card in cards {
        let in_hand = put_in_hand(state, player, card);
        let moved = state.move_object(in_hand, Zone::Library).unwrap();
        let library = &mut state.player_mut(player).library;
        library.retain(|other| *other != moved);
        library.push(moved);
    }
}

fn milled(events: &[GameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, GameEvent::CardMilled { .. }))
        .count()
}

// ---- Targets in a graveyard ----

#[test]
fn raise_dead_returns_a_creature_card_from_its_casters_graveyard() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 1);
    let raise = put_in_hand(&mut state, me, "raise_dead");
    // Nothing to return: it can't be cast.
    assert_eq!(legal_actions(&state, me), [Action::PassPriority]);

    let bears = put_in_graveyard(&mut state, me, "grizzly_bears");
    put_in_graveyard(&mut state, me, "shock");
    put_in_graveyard(&mut state, opponent, "hill_giant");
    // Only its caster's creature cards.
    assert_eq!(
        legal_actions(&state, me),
        [Action::PassPriority, cast_at(raise, at(bears))]
    );
    act(&mut state, me, cast_at(raise, at(bears)));
    let events = pass_both(&mut state);
    assert!(matches!(
        events[0],
        GameEvent::CardReturnedToHand { player, .. } if player == me
    ));
    assert_eq!(names(&state, &state.player(me).hand), ["grizzly_bears"]);
    assert_eq!(
        names(&state, &state.player(me).graveyard),
        ["shock", "raise_dead"]
    );
    state.assert_consistent();
}

#[test]
fn a_spell_aimed_at_a_graveyard_card_that_has_gone_does_nothing() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 1);
    let raise = put_in_hand(&mut state, me, "raise_dead");
    let bears = put_in_graveyard(&mut state, me, "grizzly_bears");
    act(&mut state, me, cast_at(raise, at(bears)));
    // Nothing in the pool can do it yet, so the test takes the card out of
    // the graveyard itself while the spell is on the stack.
    state.move_object(bears, Zone::Library).unwrap();
    let events = pass_both(&mut state);
    assert!(matches!(events[..], [GameEvent::SpellFizzled { .. }]));
    assert!(state.player(me).hand.is_empty());
}

#[test]
fn gravedigger_may_return_a_creature_card_or_decline() {
    let setup = || {
        let (mut state, me, _) = empty_board(1);
        put_lands(&mut state, me, "swamp", 4);
        let bears = put_in_graveyard(&mut state, me, "grizzly_bears");
        let giant = put_in_graveyard(&mut state, me, "hill_giant");
        let digger = put_in_hand(&mut state, me, "gravedigger");
        act(&mut state, me, cast(digger));
        pass_both(&mut state);
        (state, me, bears, giant)
    };
    let (mut state, me, bears, giant) = setup();
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { player, .. }) if player == me
    ));
    assert_eq!(
        legal_actions(&state, me),
        [aim(&[at(bears)]), aim(&[at(giant)]), aim(&[])]
    );
    act(&mut state, me, aim(&[at(giant)]));
    pass_both(&mut state);
    assert_eq!(names(&state, &state.player(me).hand), ["hill_giant"]);

    // Declined, the ability never goes on the stack.
    let (mut state, me, _, _) = setup();
    let events = act(&mut state, me, aim(&[]));
    assert_eq!(events, []);
    assert!(state.stack.is_empty());
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert!(state.player(me).hand.is_empty());
}

#[test]
fn a_you_may_ability_with_one_target_still_asks_and_with_none_is_skipped() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 8);
    let first = put_in_hand(&mut state, me, "gravedigger");
    let second = put_in_hand(&mut state, me, "gravedigger");
    // An empty graveyard: nothing to ask.
    act(&mut state, me, cast(first));
    pass_both(&mut state);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert!(state.stack.is_empty());

    let bears = put_in_graveyard(&mut state, me, "grizzly_bears");
    act(&mut state, me, cast(second));
    pass_both(&mut state);
    assert_eq!(legal_actions(&state, me), [aim(&[at(bears)]), aim(&[])]);
}

#[test]
fn golgari_findbroker_returns_a_permanent_card_and_has_no_choice_about_it() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    put_lands(&mut state, me, "forest", 2);
    put_in_graveyard(&mut state, me, "shock");
    let anthem = put_in_graveyard(&mut state, me, "glorious_anthem");
    let broker = put_in_hand(&mut state, me, "golgari_findbroker");
    act(&mut state, me, cast(broker));
    pass_both(&mut state);
    // The instant is not a permanent card, so the Anthem is the only
    // target and is taken without asking.
    assert_eq!(state.stack[0].targets, [at(anthem)]);
    pass_both(&mut state);
    assert_eq!(names(&state, &state.player(me).hand), ["glorious_anthem"]);
}

// ---- Mill ----

#[test]
fn mire_triton_mills_two_cards_and_gains_2_life() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    stack_library(&mut state, me, &["grizzly_bears", "shock"]);
    let library = state.player(me).library.len();
    let triton = put_in_hand(&mut state, me, "mire_triton");
    act(&mut state, me, cast(triton));
    pass_both(&mut state);
    let events = pass_both(&mut state);
    assert_eq!(milled(&events), 2);
    // The top card goes first. Everyone sees what was milled.
    assert_eq!(
        names(&state, &state.player(me).graveyard),
        ["shock", "grizzly_bears"]
    );
    assert!(matches!(
        redact_events(&events, opponent)[0],
        GameEvent::CardMilled { ref card, .. } if card.face.is_some()
    ));
    assert_eq!(state.player(me).library.len(), library - 2);
    assert_eq!(state.player(me).life, 22);
    state.assert_consistent();
}

#[test]
fn stitchers_supplier_mills_three_when_it_enters_and_when_it_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 1);
    put_lands(&mut state, opponent, "mountain", 1);
    let supplier = put_in_hand(&mut state, me, "stitchers_supplier");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, cast(supplier));
    pass_both(&mut state);
    assert_eq!(milled(&pass_both(&mut state)), 3);
    let supplier = *state.battlefield.last().unwrap();

    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(supplier)));
    pass_both(&mut state);
    assert_eq!(milled(&pass_both(&mut state)), 3);
    // Six milled cards and the Supplier itself.
    assert_eq!(state.player(me).graveyard.len(), 7);
}

#[test]
fn milling_more_cards_than_the_library_holds_mills_what_is_there() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    for card in state.player(me).library.clone() {
        state.move_object(card, Zone::Hand).unwrap();
    }
    stack_library(&mut state, me, &["shock"]);
    let triton = put_in_hand(&mut state, me, "mire_triton");
    act(&mut state, me, cast(triton));
    pass_both(&mut state);
    let events = pass_both(&mut state);
    assert_eq!(milled(&events), 1);
    // No fatigue for milling an empty library.
    assert_eq!(state.player(me).life, 22);
}

// ---- Surveil ----

#[test]
fn notion_rain_surveils_two_draws_two_and_deals_2_damage_to_its_caster() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "island", 2);
    put_lands(&mut state, me, "swamp", 1);
    stack_library(
        &mut state,
        me,
        &["hill_giant", "grizzly_bears", "shock", "murder"],
    );
    let rain = put_in_hand(&mut state, me, "notion_rain");
    act(&mut state, me, cast(rain));
    pass_both(&mut state);

    assert_eq!(
        state.decision,
        Some(Decision::Scry {
            player: me,
            kept: 0,
            remaining: 2,
            surveil: true
        })
    );
    assert_eq!(
        legal_actions(&state, me),
        [Action::ScryKeep, Action::ScryGraveyard]
    );
    let looking = |state: &DuelState| {
        let card = player_view(state, me).unwrap().looking_at.unwrap();
        card.face.unwrap().def.0
    };
    assert_eq!(looking(&state), "murder");
    assert_eq!(player_view(&state, opponent).unwrap().looking_at, None);

    // Murder to the graveyard, Shock kept.
    let events = act(&mut state, me, Action::ScryGraveyard);
    assert!(matches!(events[..], [GameEvent::CardMilled { .. }]));
    assert_eq!(looking(&state), "shock");
    act(&mut state, me, Action::ScryKeep);

    // Then the two draws (Shock and the Bears), then the damage.
    assert_eq!(
        names(&state, &state.player(me).hand),
        ["shock", "grizzly_bears"]
    );
    assert_eq!(
        names(&state, &state.player(me).graveyard),
        ["murder", "notion_rain"]
    );
    assert_eq!(
        (state.player(me).life, state.player(opponent).life),
        (18, 20)
    );
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    state.assert_consistent();
}

#[test]
fn a_card_put_in_the_graveyard_by_surveil_can_be_returned_from_it() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 2);
    put_lands(&mut state, me, "swamp", 2);
    stack_library(&mut state, me, &["shock", "shock", "shock", "hill_giant"]);
    let rain = put_in_hand(&mut state, me, "notion_rain");
    let raise = put_in_hand(&mut state, me, "raise_dead");
    act(&mut state, me, cast(rain));
    pass_both(&mut state);
    act(&mut state, me, Action::ScryGraveyard);
    act(&mut state, me, Action::ScryKeep);
    let giant = state.player(me).graveyard[0];
    assert_eq!(state.objects[&giant].def.as_str(), "hill_giant");
    act(&mut state, me, cast_at(raise, at(giant)));
    pass_both(&mut state);
    assert!(names(&state, &state.player(me).hand).contains(&"hill_giant"));
}

#[test]
fn a_creature_that_died_can_be_raised_and_cast_again() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let card = state.objects[&bears].card;
    let shock = put_in_hand(&mut state, opponent, "shock");
    let raise = put_in_hand(&mut state, me, "raise_dead");
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    pass_both(&mut state);
    let dead = state.player(me).graveyard[0];
    act(&mut state, me, cast_at(raise, at(dead)));
    pass_both(&mut state);
    let in_hand = state.player(me).hand[0];
    // The same physical card all the way round.
    assert_eq!(state.objects[&in_hand].card, card);
    act(&mut state, me, cast(in_hand));
    pass_both(&mut state);
    assert_eq!(state.objects[state.battlefield.last().unwrap()].card, card);
}
