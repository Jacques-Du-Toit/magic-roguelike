//! Tests for +1/+1 counters and the M4a cards that use them.

use serde_json::json;

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, config, empty_board, finish_combat, pass_both, pass_until,
    put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions, new_duel, player_view};
use crate::cards::{EmblemDef, Keyword};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn aim(object: ObjectId) -> Action {
    Action::ChooseTargets {
        targets: vec![at(object)],
    }
}

fn stats(state: &DuelState, creature: ObjectId) -> (i32, i32) {
    (
        state.power(creature).unwrap(),
        state.toughness(creature).unwrap(),
    )
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

fn counters(state: &DuelState, creature: ObjectId) -> u32 {
    state.objects[&creature].plus_counters
}

#[test]
fn ironshell_beetle_puts_a_lasting_counter_on_a_chosen_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let beetle = put_in_hand(&mut state, me, "ironshell_beetle");
    act(&mut state, me, cast(beetle));
    pass_both(&mut state);
    let beetle = *state.battlefield.last().unwrap();
    assert_eq!(
        legal_actions(&state, me),
        [aim(bears), aim(giant), aim(beetle)]
    );
    act(&mut state, me, aim(bears));
    let events = pass_both(&mut state);
    assert_eq!(
        events[0],
        GameEvent::CountersAdded {
            permanent: bears,
            amount: 1,
            counters: 1
        }
    );
    assert_eq!(stats(&state, bears), (3, 3));
    let view = player_view(&state, me).unwrap();
    let shown = view.battlefield.iter().find(|p| p.object == bears).unwrap();
    assert_eq!((shown.counters, shown.power), (1, Some(3)));

    // It does not wear off.
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(counters(&state, bears), 1);
}

#[test]
fn counters_are_gone_when_the_creature_leaves_the_battlefield() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    put_lands(&mut state, opponent, "island", 1);
    let beetle = put_in_hand(&mut state, me, "ironshell_beetle");
    let unsummon = put_in_hand(&mut state, opponent, "unsummon");
    act(&mut state, me, cast(beetle));
    pass_both(&mut state);
    // Alone, it puts the counter on itself.
    pass_both(&mut state);
    let beetle = *state.battlefield.last().unwrap();
    assert_eq!(stats(&state, beetle), (2, 2));

    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(unsummon, at(beetle)));
    pass_both(&mut state);
    let in_hand = state.player(me).hand[0];
    assert_eq!(counters(&state, in_hand), 0);
    act(&mut state, me, cast(in_hand));
    pass_both(&mut state);
    let again = *state.battlefield.last().unwrap();
    assert_eq!(stats(&state, again), (1, 1));
}

#[test]
fn ajanis_pridemate_grows_each_time_its_controller_gains_life() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let pridemate = put_on_battlefield(&mut state, me, "ajanis_pridemate");
    let hawk = put_on_battlefield(&mut state, me, "healers_hawk");
    let revitalize = put_in_hand(&mut state, me, "revitalize");
    act(&mut state, me, cast(revitalize));
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(stats(&state, pridemate), (3, 3));

    // A lifelink hit is another life gain.
    attack(&mut state, &[hawk]);
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, 19);
    assert_eq!(counters(&state, pridemate), 2);
    assert_eq!(stats(&state, pridemate), (4, 4));
}

#[test]
fn pridemalkin_gives_a_counter_to_one_of_yours_and_trample_to_those_with_counters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 3);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let courser = put_on_battlefield(&mut state, me, "centaur_courser");
    let goblin = put_on_battlefield(&mut state, opponent, "raging_goblin");
    let pridemalkin = put_in_hand(&mut state, me, "pridemalkin");
    act(&mut state, me, cast(pridemalkin));
    pass_both(&mut state);
    let pridemalkin = *state.battlefield.last().unwrap();
    // Not the opponent's creature.
    assert_eq!(
        legal_actions(&state, me),
        [aim(bears), aim(courser), aim(pridemalkin)]
    );
    act(&mut state, me, aim(courser));
    pass_both(&mut state);
    assert!(state.has_keyword(courser, Keyword::Trample).unwrap());
    assert!(!state.has_keyword(bears, Keyword::Trample).unwrap());
    assert!(!state.has_keyword(pridemalkin, Keyword::Trample).unwrap());
    assert!(!state.has_keyword(goblin, Keyword::Trample).unwrap());

    // The 4/4 trampler runs over a 1/1 blocker for 3.
    attack(&mut state, &[courser]);
    block(&mut state, &[(goblin, courser)]);
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, 17);
}

#[test]
fn unruly_mob_grows_when_another_of_its_controllers_creatures_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 3);
    let mob = put_on_battlefield(&mut state, me, "unruly_mob");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "raging_goblin");
    let shocks = [
        put_in_hand(&mut state, opponent, "shock"),
        put_in_hand(&mut state, opponent, "shock"),
        put_in_hand(&mut state, opponent, "shock"),
    ];
    let shoot = |state: &mut DuelState, shock: ObjectId, target: ObjectId| {
        act(state, me, Action::PassPriority);
        act(state, opponent, cast_at(shock, at(target)));
        pass_both(state);
        if !state.stack.is_empty() {
            pass_both(state);
        }
    };
    shoot(&mut state, shocks[0], bears);
    assert_eq!(stats(&state, mob), (2, 2));
    // Not for the opponent's creature...
    shoot(&mut state, shocks[1], theirs);
    assert_eq!(counters(&state, mob), 1);
    // ...and its own death is not "another creature".
    shoot(&mut state, shocks[2], mob);
    assert!(!state.is_on_battlefield(mob));
    assert!(state.stack.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
}

#[test]
fn an_emblem_can_create_a_token_each_upkeep() {
    let emblem: EmblemDef = serde_json::from_value(json!({
        "name": "Muster", "text": "At the beginning of your upkeep, create a 1/1 Soldier.",
        "abilities": [{ "kind": "triggered", "trigger": { "type": "beginning_of_upkeep" },
            "effects": [{ "type": "create_token", "token": "soldier_token" }] }]
    }))
    .unwrap();
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.emblems = vec![emblem.clone()];
    }
    let mut state = new_duel(config, 1).unwrap();
    let me = state.starting_player;
    act(&mut state, me, Action::Keep);
    act(&mut state, me.opponent(), Action::Keep);
    pass_until(&mut state, 3, Step::PrecombatMain);
    let soldiers = |player: PlayerId| {
        state
            .permanents_of(player)
            .filter(|permanent| permanent.def.as_str() == "soldier_token")
            .count()
    };
    assert_eq!((soldiers(me), soldiers(me.opponent())), (2, 1));
}

// ---- The counters package of the green-white pool ----

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

fn give_counters(state: &mut DuelState, creature: ObjectId, amount: u32) {
    state.objects.get_mut(&creature).unwrap().plus_counters = amount;
}

fn named(state: &DuelState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().name == name)
        .map(|permanent| permanent.id)
        .collect()
}

#[test]
fn snakeskin_veil_gives_a_counter_and_hexproof_for_the_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let veil = put_in_hand(&mut state, me, "snakeskin_veil");
    // Only a creature of mine can be chosen.
    assert!(legal_actions(&state, me).contains(&cast_at(veil, at(bears))));
    assert!(!legal_actions(&state, me).contains(&cast_at(veil, at(theirs))));
    act(&mut state, me, cast_at(veil, at(bears)));
    settle(&mut state);
    assert_eq!(counters(&state, bears), 1);
    assert_eq!(stats(&state, bears), (3, 3));
    assert!(state.has_keyword(bears, Keyword::Hexproof).unwrap());
    let turn = state.turn;
    pass_until(&mut state, turn + 1, Step::PrecombatMain);
    assert!(!state.has_keyword(bears, Keyword::Hexproof).unwrap());
    assert_eq!(counters(&state, bears), 1);
}

#[test]
fn crowned_ceratok_gives_trample_to_my_creatures_with_a_counter() {
    let (mut state, me, _) = empty_board(1);
    let ceratok = put_on_battlefield(&mut state, me, "crowned_ceratok");
    let plain = put_on_battlefield(&mut state, me, "grizzly_bears");
    let grown = put_on_battlefield(&mut state, me, "grizzly_bears");
    give_counters(&mut state, grown, 1);
    assert!(state.has_keyword(ceratok, Keyword::Trample).unwrap());
    assert!(state.has_keyword(grown, Keyword::Trample).unwrap());
    assert!(!state.has_keyword(plain, Keyword::Trample).unwrap());
}

#[test]
fn lightwalker_flies_only_while_it_has_a_counter() {
    let (mut state, me, _) = empty_board(1);
    let lightwalker = put_on_battlefield(&mut state, me, "lightwalker");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    give_counters(&mut state, bears, 1);
    assert!(!state.has_keyword(lightwalker, Keyword::Flying).unwrap());
    give_counters(&mut state, lightwalker, 1);
    assert!(state.has_keyword(lightwalker, Keyword::Flying).unwrap());
    // It is about itself: another creature with a counter gets nothing.
    assert!(!state.has_keyword(bears, Keyword::Flying).unwrap());
}

#[test]
fn basris_solidarity_puts_a_counter_on_each_of_my_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let knight = put_on_battlefield(&mut state, me, "youthful_knight");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let solidarity = put_in_hand(&mut state, me, "basris_solidarity");
    act(&mut state, me, cast(solidarity));
    settle(&mut state);
    assert_eq!((counters(&state, bears), counters(&state, knight)), (1, 1));
    assert_eq!(counters(&state, theirs), 0);
    assert_eq!(state.player(me).counters_put_this_turn, 2);
}

#[test]
fn ridgescale_tusker_puts_a_counter_on_each_other_creature_of_mine() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 5);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let tusker = put_in_hand(&mut state, me, "ridgescale_tusker");
    act(&mut state, me, cast(tusker));
    settle(&mut state);
    let tusker = named(&state, me, "Ridgescale Tusker")[0];
    assert_eq!(counters(&state, bears), 1);
    assert_eq!(counters(&state, tusker), 0);
    assert_eq!(stats(&state, tusker), (5, 5));
}

#[test]
fn woodland_champion_grows_with_each_token_of_mine_that_enters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "plains", 2);
    let champion = put_on_battlefield(&mut state, me, "woodland_champion");
    let alarm = put_in_hand(&mut state, me, "raise_the_alarm");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let theirs = put_in_hand(&mut state, opponent, "raise_the_alarm");
    act(&mut state, me, cast(alarm));
    settle(&mut state);
    assert_eq!(counters(&state, champion), 2);
    assert_eq!(stats(&state, champion), (4, 4));
    // A card is not a token, and the opponent's tokens are not mine.
    act(&mut state, me, cast(bears));
    settle(&mut state);
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast(theirs));
    settle(&mut state);
    assert_eq!(named(&state, opponent, "Soldier").len(), 2);
    assert_eq!(counters(&state, champion), 2);
}

#[test]
fn armorcraft_judge_draws_a_card_for_each_creature_of_mine_with_a_counter() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    for counters in [1, 3, 0] {
        let creature = put_on_battlefield(&mut state, me, "grizzly_bears");
        give_counters(&mut state, creature, counters);
    }
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    give_counters(&mut state, theirs, 1);
    let judge = put_in_hand(&mut state, me, "armorcraft_judge");
    act(&mut state, me, cast(judge));
    settle(&mut state);
    assert_eq!(state.player(me).hand.len(), 2);

    // With no counters about, it draws nothing.
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    let judge = put_in_hand(&mut state, me, "armorcraft_judge");
    act(&mut state, me, cast(judge));
    settle(&mut state);
    assert_eq!(state.player(me).hand.len(), 0);
}

#[test]
fn invigorating_surge_adds_a_counter_and_then_doubles_them() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    let fresh = put_on_battlefield(&mut state, me, "grizzly_bears");
    let grown = put_on_battlefield(&mut state, me, "grizzly_bears");
    give_counters(&mut state, grown, 2);
    for target in [fresh, grown] {
        let surge = put_in_hand(&mut state, me, "invigorating_surge");
        act(&mut state, me, cast_at(surge, at(target)));
        settle(&mut state);
    }
    assert_eq!(counters(&state, fresh), 2);
    assert_eq!(counters(&state, grown), 6);
    assert_eq!(stats(&state, grown), (8, 8));
}

#[test]
fn iridescent_hornbeetle_makes_an_insect_for_each_counter_i_put_this_turn() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_on_battlefield(&mut state, me, "iridescent_hornbeetle");
    put_on_battlefield(&mut state, me, "grizzly_bears");
    let solidarity = put_in_hand(&mut state, me, "basris_solidarity");
    act(&mut state, me, cast(solidarity));
    settle(&mut state);
    let turn = state.turn;
    pass_until(&mut state, turn, Step::End);
    assert!(named(&state, me, "Insect").is_empty());
    settle(&mut state);
    assert_eq!(named(&state, me, "Insect").len(), 2);

    // The count starts again each turn: nothing at my next end step.
    pass_until(&mut state, turn + 2, Step::End);
    assert_eq!(state.player(me).counters_put_this_turn, 0);
    settle(&mut state);
    assert_eq!(named(&state, me, "Insect").len(), 2);
    state.assert_consistent();
}

#[test]
fn cathars_crusade_puts_a_counter_on_everything_whenever_a_creature_of_mine_enters() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_on_battlefield(&mut state, me, "cathars_crusade");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let alarm = put_in_hand(&mut state, me, "raise_the_alarm");
    act(&mut state, me, cast(alarm));
    settle(&mut state);
    // Two Soldiers entered, so it triggered twice.
    assert_eq!(counters(&state, bears), 2);
    for soldier in named(&state, me, "Soldier") {
        assert_eq!(stats(&state, soldier), (3, 3));
    }
    assert_eq!(counters(&state, theirs), 0);
}

#[test]
fn kalonian_hydra_enters_with_four_counters_and_doubles_them_when_it_attacks() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 5);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    give_counters(&mut state, bears, 1);
    let hydra = put_in_hand(&mut state, me, "kalonian_hydra");
    act(&mut state, me, cast(hydra));
    settle(&mut state);
    // A 0/0 that arrives as a 4/4 and stays.
    let hydra = named(&state, me, "Kalonian Hydra")[0];
    assert_eq!(counters(&state, hydra), 4);
    assert_eq!(stats(&state, hydra), (4, 4));
    assert_eq!(state.player(me).counters_put_this_turn, 4);

    pass_until(&mut state, 3, Step::PrecombatMain);
    attack(&mut state, &[hydra]);
    settle(&mut state);
    assert_eq!(counters(&state, hydra), 8);
    assert_eq!(counters(&state, bears), 2);
    state.assert_consistent();
}
