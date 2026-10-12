//! Commanders: a legendary creature that waits in the command zone, is cast
//! from there, and goes back there instead of to the graveyard. Also the
//! legendary cards themselves.

use super::action::{Action, GameEvent, Target};
use super::config::{CardInstance, DuelConfig};
use super::error::ConfigError;
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step, Zone};
use super::test_support::{
    cast, cast_at, config, empty_hand, pass_both, pass_until, play_randomly, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::turn::discard_card;
use super::{apply_action, legal_actions, new_duel, player_view};
use crate::ai_choose_action;
use crate::cards::CardDefId;

fn instance(card: &str) -> CardInstance {
    CardInstance::new(CardDefId(card.to_string()))
}

/// Decks A and C, each led by Jasmine Boreal ({3}{G}{W}, 4/5).
fn led_config() -> DuelConfig {
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.commander = Some(instance("jasmine_boreal"));
    }
    config
}

/// A duel with commanders at the first main phase of turn 1, both hands
/// empty: the active player and their opponent.
fn led_duel(config: DuelConfig, seed: u64) -> (DuelState, PlayerId, PlayerId) {
    let mut state = new_duel(config, seed).unwrap();
    let me = state.starting_player;
    apply_action(&mut state, me, Action::Keep).unwrap();
    apply_action(&mut state, me.opponent(), Action::Keep).unwrap();
    pass_until(&mut state, 1, Step::PrecombatMain);
    empty_hand(&mut state, me);
    empty_hand(&mut state, me.opponent());
    (state, me, me.opponent())
}

fn commander(state: &DuelState, player: PlayerId) -> ObjectId {
    state.player(player).command[0]
}

/// The lands that pay for Jasmine Boreal with `tax` more generic mana.
fn lands_for_jasmine(state: &mut DuelState, player: PlayerId, tax: usize) {
    put_lands(state, player, "plains", 1);
    put_lands(state, player, "forest", 4 + tax);
}

/// Casts the player's commander from the command zone and lets it resolve.
/// Returns the permanent.
fn cast_commander(state: &mut DuelState, player: PlayerId) -> ObjectId {
    let card = commander(state, player);
    apply_action(state, player, cast(card)).unwrap();
    pass_both(state);
    *state.battlefield.last().unwrap()
}

/// The opponent of the player with priority answers with an instant aimed
/// at `target`, paid for with `lands`, and it resolves. Returns the events.
fn answer(
    state: &mut DuelState,
    spell: &str,
    lands: (&str, usize),
    target: ObjectId,
) -> Vec<GameEvent> {
    let Some(Decision::Priority { player }) = state.decision else {
        panic!("nobody has priority: {:?}", state.decision);
    };
    let opponent = player.opponent();
    put_lands(state, opponent, lands.0, lands.1);
    let card = put_in_hand(state, opponent, spell);
    apply_action(state, player, Action::PassPriority).unwrap();
    let target = Target::Object { object: target };
    let mut events = apply_action(state, opponent, cast_at(card, target)).unwrap();
    events.extend(pass_both(state));
    events
}

fn cost_shown(state: &DuelState, player: PlayerId) -> String {
    let view = player_view(state, player).unwrap();
    let commander = view.players[player.index()].commander.clone().unwrap();
    commander.cost.to_string()
}

// ---- The command zone ----

#[test]
fn a_commander_starts_in_the_command_zone_outside_the_library() {
    let state = new_duel(led_config(), 1).unwrap();
    state.assert_consistent();
    for player in PlayerId::ALL {
        let player_state = state.player(player);
        // The deck is still 25 cards: the commander is one more.
        assert_eq!(player_state.hand.len() + player_state.library.len(), 25);
        assert_eq!(player_state.command.len(), 1);
        let object = &state.objects[&player_state.command[0]];
        assert_eq!(object.def.as_str(), "jasmine_boreal");
        assert_eq!(object.zone, Zone::Command);
        assert_eq!(player_state.commander, Some(object.card));
        assert_eq!(player_state.commander_casts, 0);
    }
    // Without one, nothing changes.
    let plain = new_duel(config("deck_a", "deck_c"), 1).unwrap();
    assert!(plain.player(PlayerId::P1).command.is_empty());
    assert_eq!(plain.player(PlayerId::P1).commander, None);
}

#[test]
fn only_a_legendary_creature_can_be_a_commander() {
    for card in [
        "grizzly_bears",
        "giant_growth",
        "forest",
        "soldier_token",
        "nobody",
    ] {
        let mut config = config("deck_a", "deck_c");
        config.players[1].commander = Some(instance(card));
        assert_eq!(
            new_duel(config, 1),
            Err(ConfigError::Commander {
                player: PlayerId::P2,
                card: CardDefId(card.to_string()),
            }),
            "{card}"
        );
    }
}

#[test]
fn both_players_see_a_commander_and_what_it_costs() {
    let (mut state, me, opponent) = led_duel(led_config(), 1);
    let object = commander(&state, me);
    for viewer in [me, opponent] {
        let view = player_view(&state, viewer).unwrap();
        let shown = view.players[me.index()].commander.clone().unwrap();
        assert_eq!(shown.def.as_str(), "jasmine_boreal");
        assert_eq!(shown.object, Some(object));
        assert_eq!(shown.cost.to_string(), "{3}{G}{W}");
        assert_eq!(shown.casts, 0);
        // It is not a card in hand.
        assert!(view.players[me.index()].hand.is_empty());
    }

    lands_for_jasmine(&mut state, me, 0);
    let permanent = cast_commander(&mut state, me);
    let view = player_view(&state, opponent).unwrap();
    let shown = view.players[me.index()].commander.clone().unwrap();
    // On the battlefield it has no object to cast, and it is marked.
    assert_eq!(shown.object, None);
    assert_eq!(shown.casts, 1);
    let seen = view
        .battlefield
        .iter()
        .find(|entry| entry.object == permanent)
        .unwrap();
    assert!(seen.commander);
    assert!(
        view.battlefield
            .iter()
            .filter(|entry| entry.commander)
            .count()
            == 1
    );
}

// ---- Casting it ----

#[test]
fn a_commander_is_cast_from_the_command_zone_like_a_creature_from_hand() {
    let (mut state, me, opponent) = led_duel(led_config(), 1);
    let card = commander(&state, me);
    // Not without the mana.
    put_lands(&mut state, me, "forest", 4);
    assert!(!legal_actions(&state, me).contains(&cast(card)));
    put_lands(&mut state, me, "plains", 1);
    assert_eq!(
        legal_actions(&state, me),
        [Action::PassPriority, cast(card)]
    );
    // Nobody casts the other player's.
    let theirs = commander(&state, opponent);
    assert!(apply_action(&mut state, me, cast(theirs)).is_err());

    let events = apply_action(&mut state, me, cast(card)).unwrap();
    assert!(matches!(&events[0], GameEvent::SpellCast { player, .. } if *player == me));
    assert!(state.player(me).command.is_empty());
    assert_eq!(state.player(me).commander_casts, 1);
    assert_eq!(state.stack.len(), 1);
    state.assert_consistent();

    pass_both(&mut state);
    let permanent = *state.battlefield.last().unwrap();
    assert_eq!(state.objects[&permanent].def.as_str(), "jasmine_boreal");
    assert!(state.objects[&permanent].summoning_sick);
    assert!(state.is_commander(permanent));
    assert!(
        state
            .permanents_of(me)
            .all(|land| land.tapped || land.id == permanent)
    );
    state.assert_consistent();
}

#[test]
fn a_commander_is_cast_only_when_a_creature_could_be() {
    let (mut state, me, opponent) = led_duel(led_config(), 1);
    lands_for_jasmine(&mut state, me, 0);
    lands_for_jasmine(&mut state, opponent, 0);
    let mine = commander(&state, me);
    let theirs = commander(&state, opponent);
    // Not in the opponent's turn, and not in the end step.
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert!(!legal_actions(&state, opponent).contains(&cast(theirs)));
    apply_action(&mut state, opponent, Action::PassPriority).unwrap();
    pass_until(&mut state, 1, Step::End);
    assert!(!legal_actions(&state, me).contains(&cast(mine)));
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert!(legal_actions(&state, opponent).contains(&cast(theirs)));
}

// ---- Going back ----

#[test]
fn a_commander_that_dies_returns_to_the_command_zone_and_costs_two_more() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    lands_for_jasmine(&mut state, me, 0);
    let permanent = cast_commander(&mut state, me);
    let card = state.objects[&permanent].card;

    let events = answer(&mut state, "murder", ("swamp", 3), permanent);
    let back = commander(&state, me);
    assert_eq!(state.objects[&back].card, card);
    assert_eq!(state.objects[&back].zone, Zone::Command);
    assert!(state.player(me).graveyard.is_empty());
    assert!(!state.battlefield.contains(&permanent));
    let died = events
        .iter()
        .position(|event| matches!(event, GameEvent::CreatureDied { permanent: dead, .. } if *dead == permanent))
        .unwrap();
    assert_eq!(
        events[died + 1],
        GameEvent::CommanderReturned {
            player: me,
            card: state.card_ref(back).unwrap()
        }
    );
    state.assert_consistent();

    // The second cast costs {2} more: seven lands, not six.
    assert_eq!(cost_shown(&state, me), "{5}{G}{W}");
    put_lands(&mut state, me, "plains", 1);
    put_lands(&mut state, me, "forest", 5);
    assert!(!legal_actions(&state, me).contains(&cast(back)));
    put_lands(&mut state, me, "forest", 1);
    assert!(legal_actions(&state, me).contains(&cast(back)));
    let again = cast_commander(&mut state, me);
    assert_eq!(state.player(me).commander_casts, 2);
    assert!(
        state
            .permanents_of(me)
            .all(|land| land.tapped || land.id == again)
    );

    // And the third {4} more.
    answer(&mut state, "murder", ("swamp", 3), again);
    assert_eq!(cost_shown(&state, me), "{7}{G}{W}");
}

#[test]
fn the_tax_comes_from_the_rules_config() {
    let mut config = led_config();
    config.rules.commander_tax = 3;
    let (mut state, me, opponent) = led_duel(config, 1);
    state.player_mut(me).commander_casts = 2;
    assert_eq!(cost_shown(&state, me), "{9}{G}{W}");
    assert_eq!(cost_shown(&state, opponent), "{3}{G}{W}");
}

#[test]
fn abilities_still_see_a_commander_die() {
    let (mut state, me, opponent) = led_duel(led_config(), 1);
    // "Whenever this creature or another creature you control dies, each
    // opponent loses 1 life and you gain 1 life."
    put_on_battlefield(&mut state, me, "cruel_celebrant");
    lands_for_jasmine(&mut state, me, 0);
    let permanent = cast_commander(&mut state, me);
    answer(&mut state, "murder", ("swamp", 3), permanent);
    assert_eq!(state.stack.len(), 1);
    pass_both(&mut state);
    assert_eq!(state.player(me).life, 21);
    assert_eq!(state.player(opponent).life, 19);
    assert_eq!(state.player(me).command.len(), 1);
}

#[test]
fn a_countered_commander_returns_to_the_command_zone_and_the_cast_still_counts() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    lands_for_jasmine(&mut state, me, 0);
    let card = commander(&state, me);
    apply_action(&mut state, me, cast(card)).unwrap();
    let spell = state.stack[0].id;

    let events = answer(&mut state, "cancel", ("island", 3), spell);
    assert!(state.stack.is_empty());
    let back = commander(&state, me);
    assert_eq!(state.objects[&back].zone, Zone::Command);
    assert!(state.player(me).graveyard.is_empty());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameEvent::SpellCountered { .. }))
    );
    assert!(events.contains(&GameEvent::CommanderReturned {
        player: me,
        card: state.card_ref(back).unwrap()
    }));
    assert_eq!(cost_shown(&state, me), "{5}{G}{W}");
    state.assert_consistent();
}

#[test]
fn a_bounced_commander_goes_to_hand_and_is_cast_from_there_without_tax() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    lands_for_jasmine(&mut state, me, 0);
    let permanent = cast_commander(&mut state, me);
    let events = answer(&mut state, "unsummon", ("island", 1), permanent);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameEvent::CommanderReturned { .. }))
    );
    assert!(state.player(me).command.is_empty());
    let in_hand = state.player(me).hand[0];
    assert!(state.is_commander(in_hand));
    // Still shown as the commander, with nothing to cast from the zone.
    let view = player_view(&state, me).unwrap();
    assert_eq!(
        view.players[me.index()].commander.clone().unwrap().object,
        None
    );

    // From hand it costs what is printed, and the tax doesn't grow.
    lands_for_jasmine(&mut state, me, 0);
    assert!(legal_actions(&state, me).contains(&cast(in_hand)));
    apply_action(&mut state, me, cast(in_hand)).unwrap();
    pass_both(&mut state);
    assert_eq!(state.player(me).commander_casts, 1);
    assert!(state.is_commander(*state.battlefield.last().unwrap()));
}

#[test]
fn a_discarded_commander_returns_to_the_command_zone() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    let card = commander(&state, me);
    let in_hand = state.move_object(card, Zone::Hand).unwrap();
    let mut events = Vec::new();
    discard_card(&mut state, &mut events, me, in_hand).unwrap();
    let back = commander(&state, me);
    assert!(state.player(me).hand.is_empty() && state.player(me).graveyard.is_empty());
    assert!(matches!(events[0], GameEvent::CardDiscarded { .. }));
    assert_eq!(
        events[1],
        GameEvent::CommanderReturned {
            player: me,
            card: state.card_ref(back).unwrap()
        }
    );
    state.assert_consistent();
}

#[test]
fn another_copy_of_the_card_is_not_the_commander() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    let copy = put_on_battlefield(&mut state, me, "jasmine_boreal");
    assert!(!state.is_commander(copy));
    answer(&mut state, "murder", ("swamp", 3), copy);
    assert_eq!(state.player(me).graveyard.len(), 1);
    assert_eq!(state.player(me).command.len(), 1);
}

// ---- Whole games ----

#[test]
fn the_ai_casts_its_commander() {
    let (mut state, me, _) = led_duel(led_config(), 1);
    lands_for_jasmine(&mut state, me, 0);
    let card = commander(&state, me);
    assert_eq!(ai_choose_action(&state, me), Ok(cast(card)));
}

#[test]
fn random_games_with_commanders_never_get_stuck() {
    let mut casts = 0;
    for seed in 0..12 {
        let mut config = led_config();
        config.players[1].commander = Some(instance("boris_devilboon"));
        let (state, _) = play_randomly(config, seed, seed + 100, |state, _, _| {
            state.assert_consistent();
        });
        assert!(state.result.is_some());
        for player in PlayerId::ALL {
            let commanders = state
                .objects
                .values()
                .filter(|object| state.player(player).commander == Some(object.card))
                .count();
            assert_eq!(commanders, 1, "seed {seed}");
        }
        casts += state
            .players
            .iter()
            .map(|player| player.commander_casts)
            .sum::<u32>();
    }
    assert!(casts > 0, "no commander was ever cast");
}

// ---- The cards ----

#[test]
fn the_vanilla_legends_are_as_printed() {
    let data = crate::data::embedded().unwrap();
    for (id, cost, power, toughness) in [
        ("jasmine_boreal", "{3}{G}{W}", 4, 5),
        ("sir_shandlar_of_eberyn", "{4}{G}{W}", 4, 7),
        ("tobias_andrion", "{3}{W}{U}", 4, 4),
        ("the_lady_of_the_mountain", "{4}{R}{G}", 5, 5),
    ] {
        let card = data.cards.get(&CardDefId(id.to_string())).unwrap();
        assert!(card.is_legendary() && card.is_creature(), "{id}");
        assert_eq!(card.mana_cost.to_string(), cost, "{id}");
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{id}"
        );
        assert!(
            card.keywords.is_empty() && card.abilities.is_empty(),
            "{id}"
        );
    }
}

#[test]
fn boris_devilboon_makes_minor_demons() {
    let (mut state, me, _) = led_duel(config("deck_a", "deck_c"), 1);
    let boris = put_on_battlefield(&mut state, me, "boris_devilboon");
    put_lands(&mut state, me, "swamp", 2);
    put_lands(&mut state, me, "mountain", 1);
    let make = Action::ActivateAbility {
        source: boris,
        ability: 0,
        targets: Vec::new(),
        sacrifice: None,
    };
    // {2}{B}{R}: three lands are not enough.
    assert!(!legal_actions(&state, me).contains(&make));
    put_lands(&mut state, me, "mountain", 1);
    apply_action(&mut state, me, make.clone()).unwrap();
    assert!(state.objects[&boris].tapped);
    pass_both(&mut state);
    let demon = *state.battlefield.last().unwrap();
    let def = state.def(demon).unwrap();
    assert_eq!(
        (def.id.as_str(), def.name.as_str()),
        ("minor_demon_token", "Minor Demon")
    );
    assert!(def.token);
    assert_eq!(def.subtypes, ["Demon"]);
    assert_eq!(def.colors.len(), 2);
    assert_eq!(
        (state.power(demon).unwrap(), state.toughness(demon).unwrap()),
        (1, 1)
    );
    // It taps, so once a turn.
    put_lands(&mut state, me, "swamp", 4);
    assert!(!legal_actions(&state, me).contains(&make));
}

#[test]
fn kongming_makes_the_other_creatures_bigger() {
    let (mut state, me, opponent) = led_duel(config("deck_a", "deck_c"), 1);
    let kongming = put_on_battlefield(&mut state, me, "kongming_sleeping_dragon");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let stats = |state: &DuelState, id| (state.power(id).unwrap(), state.toughness(id).unwrap());
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(stats(&state, kongming), (2, 2));
    assert_eq!(stats(&state, theirs), (2, 2));
}

#[test]
fn elas_il_kor_gains_life_for_arrivals_and_drains_for_deaths() {
    let (mut state, me, opponent) = led_duel(config("deck_a", "deck_c"), 1);
    let elas = put_on_battlefield(&mut state, me, "elas_il_kor_sadistic_pilgrim");
    assert!(
        state
            .has_keyword(elas, crate::cards::Keyword::Deathtouch)
            .unwrap()
    );
    put_lands(&mut state, me, "forest", 2);
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    apply_action(&mut state, me, cast(bears)).unwrap();
    pass_both(&mut state);
    pass_both(&mut state);
    assert_eq!(state.player(me).life, 21);
    // The opponent's creatures arriving do nothing.
    assert_eq!(state.player(opponent).life, 20);

    let bears = *state.battlefield.last().unwrap();
    answer(&mut state, "murder", ("swamp", 3), bears);
    pass_both(&mut state);
    assert_eq!(state.player(opponent).life, 19);
    assert_eq!(state.player(me).life, 21);

    // Its own death is not "another creature".
    answer(&mut state, "murder", ("swamp", 3), elas);
    assert!(state.stack.is_empty());
    assert_eq!(state.player(opponent).life, 19);
}
