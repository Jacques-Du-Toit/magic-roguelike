//! Tests for what a `PlayerConfig` can add to a duel: emblems and starting
//! permanents.

use serde_json::{Value, json};

use super::action::{Action, GameEvent};
use super::config::{CardInstance, DuelConfig};
use super::error::ConfigError;
use super::ids::PlayerId;
use super::state::{Decision, DuelState, StackItemKind, Step, TriggeredAbility};
use super::test_support::{
    attack, cast, config, empty_hand, finish_combat, pass_both, pass_until, put_in_hand, put_lands,
    put_on_battlefield,
};
use super::view::StackItemViewKind;
use super::{apply_action, legal_actions, new_duel, player_view};
use crate::cards::{CardDefId, EmblemDef, EmblemError, Keyword};

fn emblem(name: &str, abilities: Value) -> EmblemDef {
    serde_json::from_value(json!({ "name": name, "text": "Test text.", "abilities": abilities }))
        .unwrap()
}

fn upkeep(effect: Value) -> EmblemDef {
    emblem(
        "Upkeep",
        json!([{ "kind": "triggered", "trigger": { "type": "beginning_of_upkeep" },
            "effects": [effect] }]),
    )
}

fn anthem(power: i32, toughness: i32) -> EmblemDef {
    emblem(
        "Banner",
        json!([{ "kind": "static", "type": "anthem", "affects": "creatures_you_control",
            "power": power, "toughness": toughness }]),
    )
}

fn grant(keyword: &str) -> EmblemDef {
    emblem(
        "Grant",
        json!([{ "kind": "static", "type": "grant_keyword",
            "affects": "creatures_you_control", "keyword": keyword }]),
    )
}

/// A duel in which `player` has the emblems, at the first main phase of
/// turn 1 with both hands empty. Returns the active player and the other.
fn with_emblems(player: PlayerId, emblems: Vec<EmblemDef>) -> (DuelState, PlayerId, PlayerId) {
    let mut config = config("deck_a", "deck_c");
    config.players[player.index()].emblems = emblems;
    main_phase(config, 1)
}

fn main_phase(config: DuelConfig, seed: u64) -> (DuelState, PlayerId, PlayerId) {
    let mut state = new_duel(config, seed).unwrap();
    let first = state.starting_player;
    let second = first.opponent();
    apply_action(&mut state, first, Action::Keep).unwrap();
    apply_action(&mut state, second, Action::Keep).unwrap();
    // Any upkeep trigger resolves on the way.
    pass_until(&mut state, 1, Step::PrecombatMain);
    empty_hand(&mut state, first);
    empty_hand(&mut state, second);
    (state, first, second)
}

#[test]
fn an_upkeep_emblem_triggers_in_its_controllers_upkeep_only() {
    for owner in PlayerId::ALL {
        let mut config = config("deck_a", "deck_c");
        config.players[owner.index()].emblems = vec![upkeep(
            json!({ "type": "gain_life", "player": "controller", "amount": 1 }),
        )];
        let mut state = new_duel(config, 1).unwrap();
        let first = state.starting_player;
        apply_action(&mut state, first, Action::Keep).unwrap();
        let events = apply_action(&mut state, first.opponent(), Action::Keep).unwrap();

        // Turn 1 is `first`'s: the emblem triggers only if it is theirs.
        let triggered = events
            .iter()
            .any(|event| matches!(event, GameEvent::AbilityTriggered { .. }));
        assert_eq!(triggered, owner == first);
        assert_eq!(state.stack.len(), usize::from(owner == first));

        pass_until(&mut state, 1, Step::PrecombatMain);
        assert_eq!(state.player(owner).life, 20 + i32::from(owner == first));
        pass_until(&mut state, 2, Step::PrecombatMain);
        // By now each player has had one upkeep.
        assert_eq!(state.player(owner).life, 21);
        assert_eq!(state.player(owner.opponent()).life, 20);
    }
}

#[test]
fn an_emblem_trigger_uses_the_stack_and_shows_in_the_view() {
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.emblems = vec![upkeep(
            json!({ "type": "lose_life", "player": "opponent", "amount": 2 }),
        )];
    }
    let mut state = new_duel(config, 1).unwrap();
    let first = state.starting_player;
    let second = first.opponent();
    apply_action(&mut state, first, Action::Keep).unwrap();
    apply_action(&mut state, second, Action::Keep).unwrap();

    let emblem_id = state
        .emblems
        .iter()
        .find(|emblem| emblem.controller == first)
        .unwrap()
        .id;
    assert!(matches!(
        &state.stack[0].kind,
        StackItemKind::Trigger { ability: TriggeredAbility::Emblem { emblem, index: 0 } }
            if *emblem == emblem_id
    ));
    assert_eq!(state.decision, Some(Decision::Priority { player: first }));
    // Nothing has happened yet: the ability is waiting on the stack.
    assert_eq!(state.player(second).life, 20);

    let view = player_view(&state, second).unwrap();
    assert_eq!(view.players[first.index()].emblems.len(), 1);
    assert_eq!(view.players[first.index()].emblems[0].name, "Upkeep");
    assert_eq!(view.players[first.index()].emblems[0].id, emblem_id);
    assert_eq!(
        view.stack[0].kind,
        StackItemViewKind::Emblem {
            emblem: emblem_id,
            name: "Upkeep".to_string()
        }
    );

    let events = pass_both(&mut state);
    assert_eq!(state.player(second).life, 18);
    assert_eq!(state.player(first).life, 20);
    assert!(events.contains(&GameEvent::LifeChanged {
        player: second,
        change: -2,
        life: 18
    }));
    assert!(state.stack.is_empty());
    state.assert_consistent();
}

#[test]
fn losing_life_to_an_emblem_can_end_the_game() {
    let mut config = config("deck_a", "deck_c");
    config.players[0].emblems = vec![upkeep(
        json!({ "type": "lose_life", "player": "opponent", "amount": 20 }),
    )];
    let mut state = new_duel(config, 1).unwrap();
    let first = state.starting_player;
    apply_action(&mut state, first, Action::Keep).unwrap();
    apply_action(&mut state, first.opponent(), Action::Keep).unwrap();
    for _ in 0..200 {
        if state.result.is_some() {
            break;
        }
        pass_both(&mut state);
    }
    assert_eq!(
        state.result,
        Some(super::GameResult::Win {
            player: PlayerId::P1,
            reason: super::WinReason::LifeZero
        })
    );
}

#[test]
fn a_creature_entering_triggers_its_controllers_emblem_only() {
    let enters = emblem(
        "Welcome",
        json!([{ "kind": "triggered",
            "trigger": { "type": "enters_battlefield", "subject": "creature_you_control" },
            "effects": [{ "type": "gain_life", "player": "controller", "amount": 1 }] }]),
    );
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.emblems = vec![enters.clone()];
    }
    let (mut state, me, opponent) = main_phase(config, 1);
    put_lands(&mut state, me, "forest", 3);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    let land = put_in_hand(&mut state, me, "forest");

    // A land entering is not a creature entering.
    apply_action(&mut state, me, Action::PlayLand { card: land }).unwrap();
    assert!(state.stack.is_empty());

    apply_action(&mut state, me, cast(bears)).unwrap();
    pass_both(&mut state);
    // The Bears are on the battlefield and the emblem's ability is on the
    // stack.
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].controller, me);
    pass_both(&mut state);
    assert_eq!(state.player(me).life, 21);
    assert_eq!(state.player(opponent).life, 20);
}

#[test]
fn an_anthem_changes_power_and_toughness_of_its_controllers_creatures() {
    let (mut state, me, opponent) = with_emblems(PlayerId::P1, vec![anthem(1, 2)]);
    let p1_bears = put_on_battlefield(&mut state, PlayerId::P1, "grizzly_bears");
    let p2_bears = put_on_battlefield(&mut state, PlayerId::P2, "grizzly_bears");
    let land = put_on_battlefield(&mut state, PlayerId::P1, "forest");
    assert_eq!(state.power(p1_bears).unwrap(), 3);
    assert_eq!(state.toughness(p1_bears).unwrap(), 4);
    assert_eq!(state.power(p2_bears).unwrap(), 2);
    assert_eq!(state.toughness(p2_bears).unwrap(), 2);
    assert_eq!(state.power(land).unwrap(), 0);

    let view = player_view(&state, me).unwrap();
    let shown = |object| {
        let permanent = view
            .battlefield
            .iter()
            .find(|permanent| permanent.object == object)
            .unwrap();
        (permanent.power, permanent.toughness)
    };
    assert_eq!(shown(p1_bears), (Some(3), Some(4)));
    assert_eq!(shown(p2_bears), (Some(2), Some(2)));
    let _ = opponent;
}

#[test]
fn anthems_add_up_and_count_in_combat() {
    // Two +1/+0 emblems: Grizzly Bears hits for 4.
    let (mut state, me, opponent) = {
        let mut config = config("deck_a", "deck_c");
        for player in &mut config.players {
            player.emblems = vec![anthem(1, 0), anthem(1, 0)];
        }
        main_phase(config, 1)
    };
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, 16);
}

#[test]
fn a_toughness_anthem_keeps_a_damaged_creature_alive() {
    let (mut state, me, opponent) = {
        let mut config = config("deck_a", "deck_c");
        for player in &mut config.players {
            player.emblems = vec![anthem(0, 1)];
        }
        main_phase(config, 1)
    };
    put_lands(&mut state, me, "mountain", 1);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let shock = put_in_hand(&mut state, me, "shock");
    apply_action(
        &mut state,
        me,
        super::test_support::cast_at(shock, super::Target::Object { object: bears }),
    )
    .unwrap();
    pass_both(&mut state);
    // 2 damage on a 2/3.
    assert!(state.is_on_battlefield(bears));
}

#[test]
fn granted_haste_lets_a_new_creature_attack_and_tap_for_mana() {
    for (hasty, expected) in [(true, true), (false, false)] {
        let emblems = if hasty { vec![grant("haste")] } else { vec![] };
        let (mut state, me, _) = {
            let mut config = config("deck_a", "deck_c");
            for player in &mut config.players {
                player.emblems = emblems.clone();
            }
            main_phase(config, 1)
        };
        put_lands(&mut state, me, "forest", 2);
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        apply_action(&mut state, me, cast(bears)).unwrap();
        pass_both(&mut state);
        let bears = *state.battlefield.last().unwrap();
        assert!(state.objects[&bears].summoning_sick);
        assert_eq!(state.has_keyword(bears, Keyword::Haste).unwrap(), expected);

        // With haste the player is asked to declare attackers.
        pass_until(&mut state, 1, Step::BeginCombat);
        pass_both(&mut state);
        let can_attack = legal_actions(&state, me).contains(&Action::Attack { creature: bears });
        assert_eq!(can_attack, expected);
    }

    // A hasty Llanowar Elves taps for mana the turn it arrives.
    for (hasty, expected) in [(true, true), (false, false)] {
        let emblems = if hasty { vec![grant("haste")] } else { vec![] };
        let (mut state, me, _) = {
            let mut config = config("deck_a", "deck_c");
            for player in &mut config.players {
                player.emblems = emblems.clone();
            }
            main_phase(config, 1)
        };
        let elves = state.create_card(
            me,
            CardDefId("llanowar_elves".to_string()),
            super::Zone::Battlefield,
        );
        assert!(state.objects[&elves].summoning_sick);
        let more_elves = put_in_hand(&mut state, me, "llanowar_elves");
        assert_eq!(
            legal_actions(&state, me).contains(&cast(more_elves)),
            expected
        );
    }
}
#[test]
fn granted_keywords_only_apply_to_the_controllers_creatures_and_show_in_the_view() {
    let (mut state, me, _) = with_emblems(PlayerId::P1, vec![grant("vigilance")]);
    let p1_bears = put_on_battlefield(&mut state, PlayerId::P1, "grizzly_bears");
    let p2_bears = put_on_battlefield(&mut state, PlayerId::P2, "grizzly_bears");
    let angel = put_on_battlefield(&mut state, PlayerId::P1, "serra_angel");
    assert!(state.has_keyword(p1_bears, Keyword::Vigilance).unwrap());
    assert!(!state.has_keyword(p2_bears, Keyword::Vigilance).unwrap());

    let view = player_view(&state, me).unwrap();
    let keywords = |object| {
        view.battlefield
            .iter()
            .find(|permanent| permanent.object == object)
            .unwrap()
            .keywords
            .clone()
    };
    assert_eq!(keywords(p1_bears), [Keyword::Vigilance]);
    assert_eq!(keywords(p2_bears), []);
    // Printed keywords aren't listed twice.
    assert_eq!(keywords(angel), [Keyword::Flying, Keyword::Vigilance]);
}

#[test]
fn a_creature_with_granted_vigilance_attacks_without_tapping() {
    let (mut state, _, _) = {
        let mut config = config("deck_a", "deck_c");
        for player in &mut config.players {
            player.emblems = vec![grant("vigilance")];
        }
        main_phase(config, 1)
    };
    let me = state.active_player;
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    assert!(!state.objects[&bears].tapped);
}

#[test]
fn starting_permanents_begin_on_the_battlefield_ready_to_use() {
    let mut config = config("deck_a", "deck_c");
    config.players[0].starting_permanents =
        vec![CardInstance::new(CardDefId("forest".to_string()))];
    let state = new_duel(config, 1).unwrap();
    state.assert_consistent();
    let lands: Vec<_> = state.permanents_of(PlayerId::P1).collect();
    assert_eq!(lands.len(), 1);
    assert_eq!(lands[0].def.as_str(), "forest");
    assert!(!lands[0].summoning_sick && !lands[0].tapped);
    assert_eq!(state.permanents_of(PlayerId::P2).count(), 0);
    // The library and the hand are untouched by it.
    let p1 = state.player(PlayerId::P1);
    assert_eq!(p1.hand.len() + p1.library.len(), 25);
    // The library's cards keep the ids they have without the extra land.
    assert!(
        state
            .objects
            .values()
            .filter(|object| object.zone != super::Zone::Battlefield)
            .all(|object| object.card.0 < 50)
    );
}

#[test]
fn bad_extras_are_config_errors() {
    let mut unknown = config("deck_a", "deck_c");
    unknown.players[1].starting_permanents =
        vec![CardInstance::new(CardDefId("mox_pearl".to_string()))];
    assert_eq!(
        new_duel(unknown, 1),
        Err(ConfigError::UnknownCard {
            player: PlayerId::P2,
            card: CardDefId("mox_pearl".to_string())
        })
    );

    let mut empty = config("deck_a", "deck_c");
    empty.players[0].emblems = vec![emblem("Nothing", json!([]))];
    assert_eq!(
        new_duel(empty, 1),
        Err(ConfigError::Emblem {
            player: PlayerId::P1,
            name: "Nothing".to_string(),
            source: EmblemError::NoAbilities
        })
    );
}

#[test]
fn a_duel_with_emblems_survives_a_json_round_trip_and_is_deterministic() {
    let build = || {
        let mut config = config("deck_a", "deck_c");
        config.players[0].emblems = vec![anthem(1, 0), grant("haste")];
        config.players[1].emblems = vec![upkeep(
            json!({ "type": "lose_life", "player": "opponent", "amount": 1 }),
        )];
        config
    };
    let state = new_duel(build(), 5).unwrap();
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<DuelState>(&json).unwrap(), state);
    assert_eq!(new_duel(build(), 5).unwrap(), state);
}

// ---- "A random creature an opponent controls", on the opponent's turn ----

fn gash() -> EmblemDef {
    emblem(
        "Gash",
        json!([{ "kind": "triggered",
            "trigger": { "type": "beginning_of_combat", "opponents_turn": true },
            "effects": [{ "type": "modify_pt_until_eot",
                "target": { "random_opponent_creature": {} }, "power": 1, "toughness": 1 }] }]),
    )
}

/// A duel in which the first player has the Gash, with one Grizzly Bears
/// of their own and `enemy_bears` of the opponent's. Returns the bonuses of
/// the opponent's bears, in the order they entered, once the ability has
/// resolved in the opponent's beginning of combat.
fn gashed(seed: u64, enemy_bears: usize) -> (DuelState, PlayerId, Vec<(i32, i32)>) {
    let mut config = config("deck_a", "deck_c");
    config.starting_player = Some(PlayerId::P1);
    config.players[0].emblems = vec![gash()];
    let (mut state, me, opponent) = main_phase(config, seed);
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs: Vec<_> = (0..enemy_bears)
        .map(|_| put_on_battlefield(&mut state, opponent, "grizzly_bears"))
        .collect();

    // Not in its controller's own combat.
    pass_until(&mut state, 1, Step::BeginCombat);
    assert!(state.stack.is_empty());

    // In the opponent's: it goes on the stack, and nothing has changed yet.
    pass_until(&mut state, 2, Step::BeginCombat);
    assert_eq!(state.stack.len(), 1);
    assert!(theirs.iter().all(|id| state.objects[id].power_bonus == 0));
    pass_both(&mut state);
    assert!(state.stack.is_empty());
    // Never the controller's own creature.
    assert_eq!(state.objects[&mine].power_bonus, 0);
    let bonuses = theirs
        .iter()
        .map(|id| {
            (
                state.objects[id].power_bonus,
                state.objects[id].toughness_bonus,
            )
        })
        .collect();
    (state, opponent, bonuses)
}

#[test]
fn a_random_enemy_creature_is_pumped_in_the_enemys_combat_until_end_of_turn() {
    let mut picked = std::collections::BTreeSet::new();
    for seed in 1..=12 {
        let (mut state, opponent, bonuses) = gashed(seed, 3);
        // One creature, +1/+1.
        let pumped: Vec<usize> = (0..3).filter(|&index| bonuses[index] != (0, 0)).collect();
        assert_eq!(pumped.len(), 1, "seed {seed}: {bonuses:?}");
        assert_eq!(bonuses[pumped[0]], (1, 1));
        picked.insert(pumped[0]);
        // The same seed picks the same creature.
        assert_eq!(gashed(seed, 3).2, bonuses);

        // It wears off at the end of the turn, so it never builds up.
        pass_until(&mut state, 3, Step::PrecombatMain);
        let any_bonus = state
            .permanents_of(opponent)
            .any(|permanent| permanent.power_bonus != 0 || permanent.toughness_bonus != 0);
        assert!(!any_bonus);
        state.assert_consistent();
    }
    // The pick is random: over a dozen duels it is not always the same one.
    assert!(picked.len() > 1, "{picked:?}");
}

#[test]
fn with_no_enemy_creature_the_pump_does_nothing_and_draws_nothing_from_the_rng() {
    let mut config = config("deck_a", "deck_c");
    config.starting_player = Some(PlayerId::P1);
    config.players[0].emblems = vec![gash()];
    let (mut state, me, _) = main_phase(config, 1);
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until(&mut state, 2, Step::BeginCombat);
    assert_eq!(state.stack.len(), 1);
    let rng = state.rng.clone();
    let events = pass_both(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.rng, rng);
    assert_eq!(state.objects[&mine].power_bonus, 0);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameEvent::PowerToughnessChanged { .. }))
    );
}
