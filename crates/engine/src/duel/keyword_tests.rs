//! Tests for flash, defender, hexproof and double strike, and for the M4a
//! cards that need nothing more than a keyword or an existing effect.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};
use crate::cards::{CardDefId, Keyword};
use crate::data;

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn life(state: &DuelState, player: PlayerId) -> i32 {
    state.player(player).life
}

fn can(state: &DuelState, player: PlayerId, action: &Action) -> bool {
    legal_actions(state, player).contains(action)
}

/// The combat damage dealt in `events`, as `(source, target, amount)`.
fn damage(events: &[GameEvent]) -> Vec<(ObjectId, Target, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameEvent::DamageDealt {
                source,
                target,
                amount,
            } => Some((*source, *target, *amount)),
            _ => None,
        })
        .collect()
}

// ---- Flash ----

#[test]
fn a_creature_with_flash_can_be_cast_on_the_opponents_turn() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "forest", 2);
    let viper = put_in_hand(&mut state, opponent, "ambush_viper");
    let bears = put_in_hand(&mut state, opponent, "grizzly_bears");

    // My main phase: once I pass, the opponent may cast the Viper but not
    // the Bears.
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(can(&state, opponent, &cast(viper)));
    assert!(!can(&state, opponent, &cast(bears)));

    apply_action(&mut state, opponent, cast(viper)).unwrap();
    pass_both(&mut state);
    let viper = *state.battlefield.last().unwrap();
    assert_eq!(state.objects[&viper].def.as_str(), "ambush_viper");
    assert_eq!(state.objects[&viper].owner, opponent);
    assert_eq!(state.step, Step::PrecombatMain);
    state.assert_consistent();
}

#[test]
fn a_creature_with_flash_can_be_cast_in_response_to_a_spell() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, opponent, "forest", 2);
    let shock = put_in_hand(&mut state, me, "shock");
    let viper = put_in_hand(&mut state, opponent, "ambush_viper");
    apply_action(
        &mut state,
        me,
        cast_at(shock, Target::Player { player: opponent }),
    )
    .unwrap();
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(can(&state, opponent, &cast(viper)));
}

// ---- Defender ----

#[test]
fn a_creature_with_defender_cannot_attack_but_can_block() {
    let (mut state, me, opponent) = empty_board(1);
    let wall = put_on_battlefield(&mut state, me, "angelic_wall");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let drake = put_on_battlefield(&mut state, opponent, "wind_drake");

    super::test_support::pass_until_other_decision(&mut state);
    assert!(can(&state, me, &Action::Attack { creature: bears }));
    assert!(!can(&state, me, &Action::Attack { creature: wall }));
    assert!(apply_action(&mut state, me, Action::Attack { creature: wall }).is_err());
    apply_action(&mut state, me, Action::ConfirmAttackers).unwrap();

    // On the opponent's turn the Wall, which flies, blocks the Drake.
    pass_until(&mut state, 2, Step::PrecombatMain);
    attack(&mut state, &[drake]);
    block(&mut state, &[(wall, drake)]);
    finish_combat(&mut state);
    assert_eq!(life(&state, me), 20);
    assert!(state.is_on_battlefield(wall) && state.is_on_battlefield(drake));
}

#[test]
fn with_only_a_defender_there_is_no_attack_to_declare() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "angelic_wall");
    let events = pass_until(&mut state, 1, Step::PostcombatMain);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameEvent::AttackersDeclared { .. }))
    );
}

// ---- Hexproof ----

#[test]
fn hexproof_stops_the_opponents_spells_but_not_its_controllers() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    put_lands(&mut state, opponent, "forest", 1);
    let scout = put_on_battlefield(&mut state, opponent, "gladecover_scout");
    let shock = put_in_hand(&mut state, me, "shock");
    let growth = put_in_hand(&mut state, opponent, "giant_growth");

    assert!(!can(&state, me, &cast_at(shock, at(scout))));
    assert!(apply_action(&mut state, me, cast_at(shock, at(scout))).is_err());
    // Its controller is still a legal target, and so is the Scout for them.
    assert!(can(
        &state,
        me,
        &cast_at(shock, Target::Player { player: opponent })
    ));
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(can(&state, opponent, &cast_at(growth, at(scout))));
    apply_action(&mut state, opponent, cast_at(growth, at(scout))).unwrap();
    pass_both(&mut state);
    assert_eq!(state.power(scout).unwrap(), 4);
}

#[test]
fn an_aura_cannot_be_aimed_at_an_opponents_hexproof_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let scout = put_on_battlefield(&mut state, opponent, "gladecover_scout");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let pacifism = put_in_hand(&mut state, me, "pacifism");
    assert!(!can(&state, me, &cast_at(pacifism, at(scout))));
    assert!(can(&state, me, &cast_at(pacifism, at(bears))));
}

// ---- Double strike ----

#[test]
fn an_unblocked_double_striker_deals_its_damage_twice() {
    let (mut state, me, opponent) = empty_board(1);
    let ace = put_on_battlefield(&mut state, me, "fencing_ace");
    let mut events = attack(&mut state, &[ace]);
    events.extend(finish_combat(&mut state));
    let face = Target::Player { player: opponent };
    assert_eq!(damage(&events), [(ace, face, 1), (ace, face, 1)]);
    assert_eq!(life(&state, opponent), 18);
    let steps: Vec<Step> = events
        .iter()
        .filter_map(|event| match event {
            GameEvent::StepBegan { step } => Some(*step),
            _ => None,
        })
        .collect();
    assert!(steps.contains(&Step::FirstStrikeDamage) && steps.contains(&Step::CombatDamage));
}

#[test]
fn a_double_striker_kills_a_small_blocker_before_it_hits_back() {
    let (mut state, me, opponent) = empty_board(1);
    let ace = put_on_battlefield(&mut state, me, "fencing_ace");
    let piker = put_on_battlefield(&mut state, opponent, "goblin_piker");
    attack(&mut state, &[ace]);
    let mut events = block(&mut state, &[(piker, ace)]);
    events.extend(finish_combat(&mut state));
    // The 2/1 Piker dies to the first hit and never deals its damage.
    assert_eq!(damage(&events), [(ace, at(piker), 1)]);
    assert!(state.is_on_battlefield(ace));
    assert!(!state.is_on_battlefield(piker));
    assert_eq!(life(&state, opponent), 20);
}

#[test]
fn a_double_striker_hits_a_tough_blocker_twice_and_is_hit_once() {
    let (mut state, me, opponent) = empty_board(1);
    let ace = put_on_battlefield(&mut state, me, "fencing_ace");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    attack(&mut state, &[ace]);
    let mut events = block(&mut state, &[(spider, ace)]);
    events.extend(finish_combat(&mut state));
    assert_eq!(
        damage(&events),
        [
            (ace, at(spider), 1),
            (ace, at(spider), 1),
            (spider, at(ace), 2)
        ]
    );
    assert!(!state.is_on_battlefield(ace));
    assert!(state.is_on_battlefield(spider));
}

#[test]
fn swiftblade_vindicator_tramples_over_once_its_blocker_is_dead_and_stays_untapped() {
    let (mut state, me, opponent) = empty_board(1);
    let vindicator = put_on_battlefield(&mut state, me, "swiftblade_vindicator");
    let goblin = put_on_battlefield(&mut state, opponent, "raging_goblin");
    attack(&mut state, &[vindicator]);
    assert!(!state.objects[&vindicator].tapped);
    let mut events = block(&mut state, &[(goblin, vindicator)]);
    events.extend(finish_combat(&mut state));
    // First strike kills the 1/1; the second hit has nothing left to get
    // through, so all of it tramples over.
    assert_eq!(
        damage(&events),
        [
            (vindicator, at(goblin), 1),
            (vindicator, Target::Player { player: opponent }, 1)
        ]
    );
    assert!(state.is_on_battlefield(vindicator));
    assert_eq!(life(&state, opponent), 19);
}

// ---- Cards ----

#[test]
fn the_keyword_creatures_of_m4a_have_their_printed_keywords() {
    use Keyword::*;
    let expected: [(&str, i32, i32, &[Keyword]); 9] = [
        ("thornweald_archer", 2, 1, &[Deathtouch, Reach]),
        ("kraken_hatchling", 0, 4, &[]),
        ("skyknight_legionnaire", 2, 2, &[Flying, Haste]),
        ("druid_of_the_cowl", 1, 3, &[]),
        ("ambush_viper", 2, 1, &[Deathtouch, Flash]),
        ("angelic_wall", 0, 4, &[Defender, Flying]),
        ("gladecover_scout", 1, 1, &[Hexproof]),
        ("fencing_ace", 1, 1, &[DoubleStrike]),
        (
            "swiftblade_vindicator",
            1,
            1,
            &[DoubleStrike, Trample, Vigilance],
        ),
    ];
    let cards = &data::embedded().unwrap().cards;
    for (id, power, toughness, keywords) in expected {
        let card = cards.get(&CardDefId(id.to_string())).unwrap();
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{id}"
        );
        assert_eq!(card.keywords, keywords, "{id}");
    }
}

#[test]
fn skyknight_legionnaire_attacks_in_the_air_the_turn_it_arrives() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_lands(&mut state, me, "plains", 1);
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let knight = put_in_hand(&mut state, me, "skyknight_legionnaire");
    apply_action(&mut state, me, cast(knight)).unwrap();
    pass_both(&mut state);
    let knight = *state.battlefield.last().unwrap();
    attack(&mut state, &[knight]);
    // The Bears can't block a flier, so nobody is asked to block.
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 18);
}

#[test]
fn druid_of_the_cowl_taps_for_green_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let druid = put_on_battlefield(&mut state, me, "druid_of_the_cowl");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    apply_action(&mut state, me, cast(bears)).unwrap();
    assert!(state.objects[&druid].tapped);
}

#[test]
fn cloudkin_seer_draws_a_card_when_it_enters() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 3);
    let seer = put_in_hand(&mut state, me, "cloudkin_seer");
    apply_action(&mut state, me, cast(seer)).unwrap();
    pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 0);
    pass_both(&mut state);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn dawning_angel_gains_4_life_when_it_enters() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 5);
    let angel = put_in_hand(&mut state, me, "dawning_angel");
    apply_action(&mut state, me, cast(angel)).unwrap();
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(life(&state, me), 24);
}
