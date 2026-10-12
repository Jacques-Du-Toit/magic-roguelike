//! Tests for the triggers of M4a (dies, attacks, deals damage, life gain,
//! casting a spell, upkeep on a card, prowess, once each turn), for anthems
//! and Auras that change power and toughness, and for the cards built on
//! them.

use serde_json::json;

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, config, empty_board, empty_hand, finish_combat, pass_both,
    pass_until, put_in_hand, put_lands, put_on_battlefield,
};
use super::view::StackItemViewKind;
use super::{apply_action, legal_actions, new_duel, player_view};
use crate::cards::{EmblemDef, Keyword};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

fn life(state: &DuelState, player: PlayerId) -> i32 {
    state.player(player).life
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

fn names<'a>(state: &'a DuelState, zone: &[ObjectId]) -> Vec<&'a str> {
    zone.iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

/// How many triggered abilities went on the stack in `events`.
fn triggers(events: &[GameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, GameEvent::AbilityTriggered { .. }))
        .count()
}

// ---- Life gain ----

#[test]
fn epicure_of_blood_drains_whenever_its_controller_gains_life() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_on_battlefield(&mut state, me, "epicure_of_blood");
    let revitalize = put_in_hand(&mut state, me, "revitalize");
    act(&mut state, me, cast(revitalize));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 1);
    assert_eq!(life(&state, me), 23);
    pass_both(&mut state);
    assert_eq!(life(&state, opponent), 19);

    // The opponent gaining life is nothing to it.
    put_lands(&mut state, opponent, "plains", 2);
    let theirs = put_in_hand(&mut state, opponent, "revitalize");
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast(theirs));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 0);
    assert_eq!(life(&state, opponent), 22);
}

#[test]
fn lifelink_damage_sets_off_a_life_gain_trigger() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "epicure_of_blood");
    let hawk = put_on_battlefield(&mut state, me, "healers_hawk");
    attack(&mut state, &[hawk]);
    finish_combat(&mut state);
    // One damage, one life gained, one more life lost.
    assert_eq!((life(&state, me), life(&state, opponent)), (21, 18));
}

// ---- Attacks, and damage to an opponent ----

#[test]
fn audacious_thief_draws_and_costs_a_life_when_it_attacks() {
    let (mut state, me, opponent) = empty_board(1);
    let thief = put_on_battlefield(&mut state, me, "audacious_thief");
    let events = attack(&mut state, &[thief]);
    // The ability is on the stack before anyone has priority.
    assert_eq!(triggers(&events), 1);
    assert_eq!(state.step, Step::DeclareAttackers);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(life(&state, me), 19);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 18);
}

#[test]
fn raging_regisaur_deals_1_damage_to_a_chosen_target_when_it_attacks() {
    let (mut state, me, opponent) = empty_board(1);
    let regisaur = put_on_battlefield(&mut state, me, "raging_regisaur");
    let goblin = put_on_battlefield(&mut state, opponent, "raging_goblin");
    attack(&mut state, &[regisaur]);
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: me,
            source: regisaur
        })
    );
    assert_eq!(
        player_view(&state, me).unwrap().aiming.as_deref(),
        Some("Raging Regisaur")
    );
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![at(goblin)],
        },
    );
    assert_eq!(player_view(&state, me).unwrap().aiming, None);
    pass_both(&mut state);
    // The would-be blocker is dead before blocks are declared.
    assert!(!state.is_on_battlefield(goblin));
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 16);
}

#[test]
fn thieving_magpie_draws_when_it_damages_the_opponent_and_not_when_blocked() {
    let (mut state, me, opponent) = empty_board(1);
    let magpie = put_on_battlefield(&mut state, me, "thieving_magpie");
    attack(&mut state, &[magpie]);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 19);
    assert_eq!(state.player(me).hand.len(), 1);

    let (mut state, me, opponent) = empty_board(2);
    let magpie = put_on_battlefield(&mut state, me, "thieving_magpie");
    let drake = put_on_battlefield(&mut state, opponent, "wind_drake");
    attack(&mut state, &[magpie]);
    block(&mut state, &[(drake, magpie)]);
    finish_combat(&mut state);
    assert_eq!(state.player(me).hand.len(), 0);
}

// ---- Dies ----

#[test]
fn black_cat_makes_the_opponent_discard_at_random_when_it_dies() {
    let discarded = |seed: u64| {
        let (mut state, me, opponent) = empty_board(seed);
        put_lands(&mut state, opponent, "swamp", 3);
        let cat = put_on_battlefield(&mut state, me, "black_cat");
        let murder = put_in_hand(&mut state, opponent, "murder");
        put_in_hand(&mut state, opponent, "grizzly_bears");
        put_in_hand(&mut state, opponent, "hill_giant");
        put_in_hand(&mut state, opponent, "shock");
        act(&mut state, me, Action::PassPriority);
        act(&mut state, opponent, cast_at(murder, at(cat)));
        let events = pass_both(&mut state);
        // The Cat is gone; its ability is on the stack, aimed at the
        // opponent without asking.
        assert_eq!(triggers(&events), 1);
        assert_eq!(state.stack[0].targets, [face(opponent)]);
        pass_both(&mut state);
        assert_eq!(state.player(opponent).hand.len(), 2);
        state.assert_consistent();
        let graveyard = &state.player(opponent).graveyard;
        state.objects[graveyard.last().unwrap()].def.0.clone()
    };
    // No choice is offered, and the same seed discards the same card.
    assert_eq!(discarded(1), discarded(1));
    let seen: std::collections::BTreeSet<String> = (1..12).map(discarded).collect();
    assert!(seen.len() > 1, "always {seen:?}");
}

#[test]
fn pitchburn_devils_deals_3_damage_from_the_graveyard_when_it_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "swamp", 3);
    let devils = put_on_battlefield(&mut state, me, "pitchburn_devils");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let murder = put_in_hand(&mut state, opponent, "murder");
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(murder, at(devils)));
    pass_both(&mut state);

    // Its controller chooses what the dead Devils burn.
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: me,
            source: devils
        })
    );
    assert_eq!(
        player_view(&state, me).unwrap().aiming.as_deref(),
        Some("Pitchburn Devils")
    );
    assert_eq!(legal_actions(&state, me).len(), 3);
    act(
        &mut state,
        me,
        Action::ChooseTargets {
            targets: vec![at(giant)],
        },
    );
    let events = pass_both(&mut state);
    assert_eq!(
        events[0],
        GameEvent::DamageDealt {
            source: devils,
            target: at(giant),
            amount: 3
        }
    );
    assert!(!state.is_on_battlefield(giant));
    state.assert_consistent();
}

#[test]
fn cruel_celebrant_drains_for_each_of_its_controllers_creatures_that_dies() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "mountain", 2);
    put_on_battlefield(&mut state, me, "cruel_celebrant");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "raging_goblin");
    let shock = put_in_hand(&mut state, opponent, "shock");
    let second = put_in_hand(&mut state, opponent, "shock");

    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 1);
    pass_both(&mut state);
    assert_eq!((life(&state, me), life(&state, opponent)), (21, 19));

    // An opponent's creature dying is nothing to it.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(second, at(theirs)));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 0);
    assert_eq!((life(&state, me), life(&state, opponent)), (21, 19));
}

#[test]
fn cruel_celebrant_sees_itself_and_a_creature_dying_with_it() {
    let (mut state, me, opponent) = empty_board(1);
    let celebrant = put_on_battlefield(&mut state, me, "cruel_celebrant");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let baloth = put_on_battlefield(&mut state, opponent, "rumbling_baloth");
    // Both attack into bigger blockers and die in the same damage step.
    attack(&mut state, &[celebrant, bears]);
    block(&mut state, &[(giant, celebrant), (baloth, bears)]);
    let events = finish_combat(&mut state);
    assert_eq!(triggers(&events), 2);
    assert!(!state.is_on_battlefield(celebrant) && !state.is_on_battlefield(bears));
    assert_eq!((life(&state, me), life(&state, opponent)), (22, 18));
    state.assert_consistent();
}

// ---- Casting a spell ----

#[test]
fn wee_dragonauts_grows_when_its_controller_casts_an_instant_or_sorcery() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "mountain", 1);
    let dragonauts = put_on_battlefield(&mut state, me, "wee_dragonauts");
    let shock = put_in_hand(&mut state, me, "shock");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let theirs = put_in_hand(&mut state, opponent, "shock");

    let events = act(&mut state, me, cast_at(shock, face(opponent)));
    // The ability goes on the stack above the spell, and its controller
    // still has priority.
    assert_eq!(triggers(&events), 1);
    assert_eq!(state.stack.len(), 2);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    pass_both(&mut state);
    assert_eq!(stats(&state, dragonauts), (3, 3));
    assert_eq!(life(&state, opponent), 20);
    pass_both(&mut state);
    assert_eq!(life(&state, opponent), 18);

    // Not for a creature spell, nor for the opponent's spells.
    assert_eq!(triggers(&act(&mut state, me, cast(bears))), 0);
    pass_both(&mut state);
    act(&mut state, me, Action::PassPriority);
    assert_eq!(
        triggers(&act(&mut state, opponent, cast_at(theirs, face(me)))),
        0
    );
    // A player who casts a spell in the other's turn keeps priority.
    assert_eq!(
        state.decision,
        Some(Decision::Priority { player: opponent })
    );
    pass_both(&mut state);

    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, dragonauts), (1, 3));
}

#[test]
fn prowess_gives_plus_1_plus_1_for_each_noncreature_spell() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 4);
    let swiftspear = put_on_battlefield(&mut state, me, "monastery_swiftspear");
    let first = put_in_hand(&mut state, me, "shock");
    let second = put_in_hand(&mut state, me, "shock");
    let goblin = put_in_hand(&mut state, me, "raging_goblin");

    assert_eq!(triggers(&act(&mut state, me, cast(goblin))), 0);
    pass_both(&mut state);

    act(&mut state, me, cast_at(first, face(opponent)));
    let view = player_view(&state, me).unwrap();
    assert_eq!(
        view.stack[1].kind,
        StackItemViewKind::Keyword {
            source: swiftspear,
            def: state.objects[&swiftspear].def.clone(),
            keyword: Keyword::Prowess
        }
    );
    // In response, the second Shock: prowess twice.
    assert_eq!(
        triggers(&act(&mut state, me, cast_at(second, face(opponent)))),
        1
    );
    pass_both(&mut state);
    assert_eq!(stats(&state, swiftspear), (2, 3));
    for _ in 0..3 {
        pass_both(&mut state);
    }
    assert!(state.stack.is_empty());
    assert_eq!(stats(&state, swiftspear), (3, 4));
    assert_eq!(life(&state, opponent), 16);
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, swiftspear), (1, 2));
}

// ---- Upkeep, on a card ----

#[test]
fn phyrexian_arena_draws_a_card_and_costs_a_life_each_upkeep_of_its_controller() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let arena = put_in_hand(&mut state, me, "phyrexian_arena");
    act(&mut state, me, cast(arena));
    pass_both(&mut state);
    assert!(
        state
            .battlefield
            .iter()
            .any(|id| state.objects[id].def.as_str() == "phyrexian_arena")
    );

    // Nothing in the opponent's upkeep.
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert_eq!((life(&state, me), life(&state, opponent)), (20, 20));
    let hand = state.player(me).hand.len();
    // In mine: the Arena's card, then the draw step's.
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(life(&state, me), 19);
    assert_eq!(state.player(me).hand.len(), hand + 2);
}

// ---- Anthems and Auras that change power and toughness ----

#[test]
fn glorious_anthem_gives_its_controllers_creatures_plus_1_plus_1() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let anthem = put_in_hand(&mut state, me, "glorious_anthem");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, cast(anthem));
    pass_both(&mut state);
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(stats(&state, giant), (3, 3));
    let view = player_view(&state, me).unwrap();
    let shown = view.battlefield.iter().find(|p| p.object == bears).unwrap();
    assert_eq!((shown.power, shown.toughness), (Some(3), Some(3)));

    // A Shock no longer kills the Bears.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    pass_both(&mut state);
    assert!(state.is_on_battlefield(bears));
    state.assert_consistent();
}

#[test]
fn dead_weight_shrinks_the_creature_it_enchants() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 2);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let first = put_in_hand(&mut state, me, "dead_weight");
    let second = put_in_hand(&mut state, me, "dead_weight");

    act(&mut state, me, cast_at(first, at(giant)));
    pass_both(&mut state);
    assert_eq!(stats(&state, giant), (1, 1));
    assert_eq!(stats(&state, bears), (2, 2));

    // On a 2/2 it is removal: the Bears die and the Aura follows them.
    act(&mut state, me, cast_at(second, at(bears)));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));
    assert_eq!(names(&state, &state.player(me).graveyard), ["dead_weight"]);
    assert_eq!(state.static_sources.len(), 1);
    state.assert_consistent();
}

#[test]
fn empyrean_eagle_pumps_its_controllers_other_fliers_only() {
    let (mut state, me, opponent) = empty_board(1);
    let eagle = put_on_battlefield(&mut state, me, "empyrean_eagle");
    let drake = put_on_battlefield(&mut state, me, "wind_drake");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "wind_drake");
    assert_eq!(stats(&state, eagle), (2, 3));
    assert_eq!(stats(&state, drake), (3, 3));
    assert_eq!(stats(&state, bears), (2, 2));
    assert_eq!(stats(&state, theirs), (2, 2));

    // Two Eagles pump each other.
    let other = put_on_battlefield(&mut state, me, "empyrean_eagle");
    assert_eq!(stats(&state, eagle), (3, 4));
    assert_eq!(stats(&state, other), (3, 4));
    assert_eq!(stats(&state, drake), (4, 4));
}

// ---- Once each turn, and the creature that set an ability off ----

fn firebrand_passive() -> EmblemDef {
    crate::data::embedded()
        .unwrap()
        .run
        .emblems
        .iter()
        .find(|emblem| emblem.id.as_str() == "emblem_fury")
        .unwrap()
        .emblem()
        .unwrap()
}

#[test]
fn the_firebrands_passive_gives_haste_to_the_first_creature_each_turn() {
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.emblems = vec![firebrand_passive()];
    }
    let mut state = new_duel(config, 1).unwrap();
    let me = state.starting_player;
    act(&mut state, me, Action::Keep);
    act(&mut state, me.opponent(), Action::Keep);
    pass_until(&mut state, 1, Step::PrecombatMain);
    empty_hand(&mut state, me);
    empty_hand(&mut state, me.opponent());
    put_lands(&mut state, me, "forest", 4);
    let first = put_in_hand(&mut state, me, "grizzly_bears");
    let second = put_in_hand(&mut state, me, "grizzly_bears");

    act(&mut state, me, cast(first));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 1);
    let first = *state.battlefield.last().unwrap();
    assert!(!state.has_keyword(first, Keyword::Haste).unwrap());
    let events = pass_both(&mut state);
    assert!(events.contains(&GameEvent::KeywordGranted {
        permanent: first,
        keyword: Keyword::Haste
    }));

    // The second creature of the turn gets nothing.
    act(&mut state, me, cast(second));
    let events = pass_both(&mut state);
    assert_eq!(triggers(&events), 0);
    let second = *state.battlefield.last().unwrap();
    assert!(!state.has_keyword(second, Keyword::Haste).unwrap());

    // Only the first can attack this turn.
    super::test_support::pass_until_other_decision(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareAttackers { player: me })
    );
    let attacks: Vec<Action> = legal_actions(&state, me)
        .into_iter()
        .filter(|action| matches!(action, Action::Attack { .. }))
        .collect();
    assert_eq!(attacks, [Action::Attack { creature: first }]);
    act(&mut state, me, Action::ConfirmAttackers);

    // Next turn of mine it works again.
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert!(!state.has_keyword(first, Keyword::Haste).unwrap());
    empty_hand(&mut state, me);
    let third = put_in_hand(&mut state, me, "grizzly_bears");
    act(&mut state, me, cast(third));
    assert_eq!(triggers(&pass_both(&mut state)), 1);
}

#[test]
fn an_ability_about_the_creature_that_set_it_off_does_nothing_once_that_creature_is_gone() {
    let mut config = config("deck_a", "deck_c");
    config.players[0].emblems = vec![firebrand_passive()];
    config.players[1].emblems = vec![firebrand_passive()];
    let mut state = new_duel(config, 1).unwrap();
    let me = state.starting_player;
    let opponent = me.opponent();
    act(&mut state, me, Action::Keep);
    act(&mut state, opponent, Action::Keep);
    pass_until(&mut state, 1, Step::PrecombatMain);
    empty_hand(&mut state, me);
    empty_hand(&mut state, opponent);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let shock = put_in_hand(&mut state, opponent, "shock");
    act(&mut state, me, cast(bears));
    pass_both(&mut state);
    let bears = *state.battlefield.last().unwrap();
    // With the haste ability on the stack, the Bears are shot.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));
    let events = pass_both(&mut state);
    assert!(matches!(events[..], [GameEvent::AbilityResolved { .. }]));
    assert_eq!(
        serde_json::to_value(&state.stack).unwrap(),
        json!([]),
        "the stack is empty"
    );
}
