use super::*;
use crate::cards::CardDefId;
use crate::duel::test_support::{
    attack, cast, cast_at, config, empty_board, empty_hand, pass_until_other_decision, put_in_hand,
    put_lands, put_on_battlefield,
};
use crate::duel::{GameEvent, Target, apply_action, new_duel};

fn choose(state: &DuelState, player: PlayerId) -> Action {
    ai_choose_action(state, player).unwrap()
}

/// Lets the AI act for whoever the game is waiting on, once.
fn step(state: &mut DuelState) -> (PlayerId, Action, Vec<GameEvent>) {
    let player = state.waiting_on().unwrap();
    let action = choose(state, player);
    let events = apply_action(state, player, action.clone()).unwrap();
    (player, action, events)
}

/// Plays a whole duel with the AI on both sides.
fn play(p1_deck: &str, p2_deck: &str, seed: u64) -> (DuelState, Vec<GameEvent>) {
    let mut state = new_duel(config(p1_deck, p2_deck), seed).unwrap();
    let mut log = Vec::new();
    for _ in 0..50_000 {
        if state.result.is_some() {
            return (state, log);
        }
        log.extend(step(&mut state).2);
    }
    panic!(
        "{p1_deck} v {p2_deck}, seed {seed}: no result by turn {}",
        state.turn
    );
}

/// A fresh duel waiting on the starting player's mulligan decision, with
/// their hand replaced by `cards`.
fn mulligan_decision(cards: &[&str]) -> (DuelState, PlayerId) {
    let mut state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
    let player = state.starting_player;
    empty_hand(&mut state, player);
    for card in cards {
        put_in_hand(&mut state, player, card);
    }
    (state, player)
}

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

#[test]
fn only_the_player_the_game_waits_on_gets_an_action() {
    let (state, me, opponent) = empty_board(1);
    assert_eq!(choose(&state, me), Action::PassPriority);
    assert_eq!(
        ai_choose_action(&state, opponent),
        Err(ActionError::NotWaitingOn(opponent))
    );
}

// ---- Mulligans ----

#[test]
fn keeps_hands_with_two_to_five_lands() {
    let hand = |lands: usize| {
        let mut cards = vec!["forest"; lands];
        cards.resize(7, "grizzly_bears");
        mulligan_decision(&cards)
    };
    for lands in [0, 1, 6, 7] {
        let (state, player) = hand(lands);
        assert_eq!(choose(&state, player), Action::Mulligan, "{lands} lands");
    }
    for lands in 2..=5 {
        let (state, player) = hand(lands);
        assert_eq!(choose(&state, player), Action::Keep, "{lands} lands");
    }
}

#[test]
fn never_mulligans_below_five_cards() {
    let no_lands = ["grizzly_bears"; 7];
    let (mut state, player) = mulligan_decision(&no_lands);
    // A second mulligan would still leave five cards.
    state.player_mut(player).mulligans_taken = 1;
    assert_eq!(choose(&state, player), Action::Mulligan);
    // A third would leave four.
    state.player_mut(player).mulligans_taken = 2;
    assert_eq!(choose(&state, player), Action::Keep);
}

#[test]
fn bottoms_a_spare_land_or_else_its_most_expensive_spell() {
    let bottomed = |cards: &[&str]| {
        let (mut state, player) = mulligan_decision(cards);
        state.player_mut(player).mulligans_taken = 1;
        apply_action(&mut state, player, Action::Keep).unwrap();
        match choose(&state, player) {
            Action::BottomCard { card } => state.objects[&card].def.0.clone(),
            other => panic!("expected to bottom a card, got {other:?}"),
        }
    };
    let flooded = [
        "forest",
        "forest",
        "plains",
        "plains",
        "grizzly_bears",
        "serra_angel",
        "pacifism",
    ];
    assert_eq!(bottomed(&flooded), "plains");
    let fine = [
        "forest",
        "plains",
        "forest",
        "grizzly_bears",
        "colossal_dreadmaw",
        "serra_angel",
        "pacifism",
    ];
    assert_eq!(bottomed(&fine), "colossal_dreadmaw");
}

#[test]
fn discards_at_cleanup() {
    let (mut state, me, _) = empty_board(1);
    for card in ["forest", "forest", "grizzly_bears", "serra_angel"] {
        put_in_hand(&mut state, me, card);
        put_in_hand(&mut state, me, card);
    }
    state.decision = Some(Decision::Discard {
        player: me,
        remaining: 1,
    });
    let Action::Discard { card } = choose(&state, me) else {
        panic!("expected a discard");
    };
    assert_eq!(state.objects[&card].def.as_str(), "forest");
}

// ---- Lands and spells ----

#[test]
fn plays_a_land_before_anything_else_and_picks_the_colour_it_lacks() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let forest = put_in_hand(&mut state, me, "forest");
    let plains = put_in_hand(&mut state, me, "plains");
    put_in_hand(&mut state, me, "savannah_lions");
    put_in_hand(&mut state, me, "llanowar_elves");
    assert_eq!(choose(&state, me), Action::PlayLand { card: plains });

    // With the colours covered it still plays a land.
    put_lands(&mut state, me, "plains", 1);
    assert!(matches!(
        choose(&state, me),
        Action::PlayLand { card } if card == forest || card == plains
    ));
}

#[test]
fn casts_the_best_creature_it_can_afford() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 3);
    put_in_hand(&mut state, me, "grizzly_bears");
    let courser = put_in_hand(&mut state, me, "centaur_courser");
    put_in_hand(&mut state, me, "colossal_dreadmaw");
    assert_eq!(choose(&state, me), cast(courser));
}

#[test]
fn keeps_casting_while_it_has_mana_then_passes() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 3);
    put_in_hand(&mut state, me, "grizzly_bears");
    put_in_hand(&mut state, me, "llanowar_elves");
    let mut cast_count = 0;
    while state.turn == 1 && state.step == Step::PrecombatMain {
        if let (_, Action::CastSpell { .. }, _) = step(&mut state) {
            cast_count += 1;
        }
    }
    assert_eq!(cast_count, 2);
    assert_eq!(state.creatures_of(me).unwrap().len(), 2);
}

#[test]
fn passes_when_there_is_nothing_worth_doing() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    put_in_hand(&mut state, me, "colossal_dreadmaw");
    assert_eq!(choose(&state, me), Action::PassPriority);
}

#[test]
fn burns_a_creature_it_can_kill() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    put_on_battlefield(&mut state, me, "grizzly_bears");
    assert_eq!(choose(&state, me), cast_at(shock, at(bears)));
}

#[test]
fn holds_burn_early_and_goes_face_once_the_opponent_is_low() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");
    assert_eq!(choose(&state, me), Action::PassPriority);

    state.player_mut(opponent).life = 12;
    assert_eq!(choose(&state, me), cast_at(shock, face(opponent)));
}

#[test]
fn takes_a_lethal_burn_shot_over_killing_a_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let strike = put_in_hand(&mut state, me, "lightning_strike");
    put_on_battlefield(&mut state, opponent, "serra_angel");
    put_on_battlefield(&mut state, opponent, "centaur_courser");
    state.player_mut(opponent).life = 3;
    assert_eq!(choose(&state, me), cast_at(strike, face(opponent)));
}

#[test]
fn murders_the_biggest_threat() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let murder = put_in_hand(&mut state, me, "murder");
    put_on_battlefield(&mut state, opponent, "llanowar_elves");
    let dreadmaw = put_on_battlefield(&mut state, opponent, "colossal_dreadmaw");
    put_on_battlefield(&mut state, me, "serra_angel");
    assert_eq!(choose(&state, me), cast_at(murder, at(dreadmaw)));
}

#[test]
fn pacifies_the_opponents_best_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let pacifism = put_in_hand(&mut state, me, "pacifism");
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let angel = put_on_battlefield(&mut state, opponent, "serra_angel");
    put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    assert_eq!(choose(&state, me), cast_at(pacifism, at(angel)));
}

#[test]
fn casts_divination_for_the_cards() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 3);
    let divination = put_in_hand(&mut state, me, "divination");
    assert_eq!(choose(&state, me), cast(divination));
}

#[test]
fn counters_a_creature_but_not_a_shock_to_the_face() {
    let (mut state, me, ai) = empty_board(1);
    put_lands(&mut state, ai, "island", 3);
    let cancel = put_in_hand(&mut state, ai, "cancel");
    put_lands(&mut state, me, "mountain", 5);
    let giant = put_in_hand(&mut state, me, "hill_giant");
    let shock = put_in_hand(&mut state, me, "shock");

    apply_action(&mut state, me, cast_at(shock, face(ai))).unwrap();
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert_eq!(choose(&state, ai), Action::PassPriority);
    apply_action(&mut state, ai, Action::PassPriority).unwrap();

    apply_action(&mut state, me, cast(giant)).unwrap();
    let spell = state.stack[0].id;
    apply_action(&mut state, me, Action::PassPriority).unwrap();
    assert_eq!(choose(&state, ai), cast_at(cancel, at(spell)));
}

#[test]
fn uses_giant_growth_to_win_a_combat_not_before() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 1);
    let growth = put_in_hand(&mut state, me, "giant_growth");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    // In the main phase the pump would achieve nothing.
    assert_eq!(choose(&state, me), Action::PassPriority);

    attack(&mut state, &[bears]);
    pass_until_other_decision(&mut state);
    apply_action(
        &mut state,
        opponent,
        Action::Block {
            blocker: giant,
            attacker: bears,
        },
    )
    .unwrap();
    apply_action(&mut state, opponent, Action::ConfirmBlockers).unwrap();
    assert_eq!(choose(&state, me), cast_at(growth, at(bears)));
}

// ---- Attacking ----

/// The creatures the AI attacks with when it is `player`'s turn to declare.
fn attackers_chosen(state: &mut DuelState, player: PlayerId) -> Vec<ObjectId> {
    while state.decision != Some(Decision::DeclareAttackers { player }) {
        if state.waiting_on().is_none() || state.step > Step::DeclareAttackers {
            return Vec::new();
        }
        step(state);
    }
    let mut attackers = Vec::new();
    loop {
        match step(state).1 {
            Action::Attack { creature } => attackers.push(creature),
            Action::ConfirmAttackers => return attackers,
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn attacks_an_empty_board_with_everything() {
    let (mut state, me, _) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let elves = put_on_battlefield(&mut state, me, "llanowar_elves");
    assert_eq!(attackers_chosen(&mut state, me), [bears, elves]);
}

#[test]
fn does_not_attack_into_a_blocker_that_wins_the_fight() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    assert_eq!(attackers_chosen(&mut state, me), []);
}

#[test]
fn attacks_when_no_blocker_can_stop_it_profitably() {
    let (mut state, me, opponent) = empty_board(1);
    let drake = put_on_battlefield(&mut state, me, "wind_drake");
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    put_on_battlefield(&mut state, me, "llanowar_elves");
    // Bears can't block the flyer and lose to the Giant; they would eat
    // the Elves.
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    assert_eq!(attackers_chosen(&mut state, me), [drake, giant]);
}

#[test]
fn a_tapped_blocker_is_no_deterrent() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    state.objects.get_mut(&giant).unwrap().tapped = true;
    assert_eq!(attackers_chosen(&mut state, me), [bears]);
}

#[test]
fn attacks_with_everything_when_that_is_lethal() {
    let (mut state, me, opponent) = empty_board(1);
    let first = put_on_battlefield(&mut state, me, "grizzly_bears");
    let second = put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    // One Bears gets blocked; the other deals 2.
    state.player_mut(opponent).life = 3;
    assert_eq!(attackers_chosen(&mut state.clone(), me), []);
    state.player_mut(opponent).life = 2;
    assert_eq!(attackers_chosen(&mut state, me), [first, second]);
}

#[test]
fn a_menace_creature_attacks_past_a_single_blocker() {
    let (mut state, me, opponent) = empty_board(1);
    let brute = put_on_battlefield(&mut state, me, "boggart_brute");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    assert_eq!(attackers_chosen(&mut state.clone(), me), [brute]);
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    assert_eq!(attackers_chosen(&mut state, me), []);
}

// ---- Blocking ----

/// The blocks the AI makes as `defender` against `attackers`.
fn blocks_chosen(
    state: &mut DuelState,
    defender: PlayerId,
    attackers: &[ObjectId],
) -> Vec<(ObjectId, ObjectId)> {
    attack(state, attackers);
    pass_until_other_decision(state);
    let mut blocks = Vec::new();
    while state.decision == Some(Decision::DeclareBlockers { player: defender }) {
        if let Action::Block { blocker, attacker } = step(state).1 {
            blocks.push((blocker, attacker));
        }
    }
    blocks
}

#[test]
fn blocks_when_it_kills_the_attacker_and_survives() {
    let (mut state, me, ai) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, ai, "hill_giant");
    put_on_battlefield(&mut state, ai, "llanowar_elves");
    assert_eq!(blocks_chosen(&mut state, ai, &[bears]), [(giant, bears)]);
}

#[test]
fn does_not_chump_block_when_the_damage_is_affordable() {
    let (mut state, me, ai) = empty_board(1);
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    put_on_battlefield(&mut state, ai, "llanowar_elves");
    assert_eq!(blocks_chosen(&mut state, ai, &[dreadmaw]), []);
}

#[test]
fn chump_blocks_to_survive() {
    let (mut state, me, ai) = empty_board(1);
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let elves = put_on_battlefield(&mut state, ai, "llanowar_elves");
    state.player_mut(ai).life = 7;
    // Taking 8 is lethal; blocking the Dreadmaw leaves 2 from the Bears.
    assert_eq!(
        blocks_chosen(&mut state, ai, &[dreadmaw, bears]),
        [(elves, dreadmaw)]
    );
}

#[test]
fn trades_evenly_but_not_at_a_loss() {
    let (mut state, me, ai) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let blocker = put_on_battlefield(&mut state, ai, "grizzly_bears");
    assert_eq!(
        blocks_chosen(&mut state.clone(), ai, &[bears]),
        [(blocker, bears)]
    );

    // An Angel doesn't trade with a deathtouch Rat.
    let (mut state, me, ai) = empty_board(1);
    let rats = put_on_battlefield(&mut state, me, "typhoid_rats");
    put_on_battlefield(&mut state, ai, "serra_angel");
    assert_eq!(blocks_chosen(&mut state, ai, &[rats]), []);
}

#[test]
fn blocks_a_menace_creature_with_two_only_to_survive() {
    let (mut state, me, ai) = empty_board(1);
    let brute = put_on_battlefield(&mut state, me, "boggart_brute");
    let first = put_on_battlefield(&mut state, ai, "llanowar_elves");
    let second = put_on_battlefield(&mut state, ai, "llanowar_elves");
    assert_eq!(blocks_chosen(&mut state.clone(), ai, &[brute]), []);
    state.player_mut(ai).life = 3;
    assert_eq!(
        blocks_chosen(&mut state, ai, &[brute]),
        [(first, brute), (second, brute)]
    );
}

// ---- Whole games ----

const PAIRINGS: [(&str, &str); 6] = [
    ("deck_a", "deck_a"),
    ("deck_a", "deck_b"),
    ("deck_a", "deck_c"),
    ("deck_b", "deck_b"),
    ("deck_b", "deck_c"),
    ("deck_c", "deck_c"),
];

#[test]
fn ai_games_finish_for_every_pairing() {
    for (p1, p2) in PAIRINGS {
        for seed in 0..4 {
            let (state, log) = play(p1, p2, seed);
            state.assert_consistent();
            assert!(matches!(log.last(), Some(GameEvent::GameOver { .. })));
        }
    }
}

#[test]
fn ai_games_are_deterministic() {
    assert_eq!(play("deck_a", "deck_c", 11), play("deck_a", "deck_c", 11));
    assert_ne!(
        play("deck_a", "deck_c", 11).1,
        play("deck_a", "deck_c", 12).1
    );
}

#[test]
fn ai_games_are_decided_by_play_not_fatigue() {
    // Creature and burn decks should usually finish each other off long
    // before the libraries run out.
    let by_damage = (0..10)
        .filter(|&seed| {
            let (state, _) = play("deck_a", "deck_c", seed);
            state.players.iter().all(|player| player.fatigue_draws == 0)
        })
        .count();
    assert!(
        by_damage >= 8,
        "only {by_damage}/10 games ended without fatigue"
    );
}

#[test]
fn the_ai_does_not_use_hidden_information() {
    // Replace every card the deciding player can't see with a Swamp. The
    // decision must not change.
    for (p1, p2) in [("deck_a", "deck_c"), ("deck_b", "deck_b")] {
        let mut state = new_duel(config(p1, p2), 5).unwrap();
        let mut decisions = 0;
        while state.result.is_none() {
            let player = state.waiting_on().unwrap();
            decisions += 1;
            if decisions % 5 == 0 {
                let opponent = state.player(player.opponent());
                let hidden: Vec<ObjectId> = opponent
                    .hand
                    .iter()
                    .chain(&opponent.library)
                    .chain(&state.player(player).library)
                    .copied()
                    .collect();
                let mut blind = state.clone();
                for id in hidden {
                    blind.objects.get_mut(&id).unwrap().def = CardDefId("swamp".to_string());
                }
                assert_eq!(
                    choose(&blind, player),
                    choose(&state, player),
                    "{p1} v {p2}, decision {decisions}"
                );
            }
            step(&mut state);
        }
        assert!(decisions > 100);
    }
}

// ---- Choices: targets of triggered abilities, scry ----

#[test]
fn aims_a_bounce_at_the_opponents_best_creature_not_its_own() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, opponent, "raging_goblin");
    let angel = put_on_battlefield(&mut state, opponent, "serra_angel");
    put_lands(&mut state, me, "island", 3);
    let man_o_war = put_in_hand(&mut state, me, "man_o_war");
    // It is worth casting for the bounce.
    assert_eq!(choose(&state, me), cast(man_o_war));
    apply_action(&mut state, me, cast(man_o_war)).unwrap();
    crate::duel::test_support::pass_both(&mut state);
    assert!(matches!(
        state.decision,
        Some(Decision::ChooseTargets { .. })
    ));
    assert_eq!(
        choose(&state, me),
        Action::ChooseTargets {
            targets: vec![at(angel)]
        }
    );
}

#[test]
fn scrying_it_bottoms_a_land_when_flooded_and_a_spell_it_cannot_cast_for_ages() {
    let scry_with = |lands: usize, top: &str| {
        let (mut state, me, _) = empty_board(1);
        put_lands(&mut state, me, "island", lands);
        let opt = put_in_hand(&mut state, me, "opt");
        apply_action(&mut state, me, cast(opt)).unwrap();
        // Put the card to look at on top of the library.
        let card = put_in_hand(&mut state, me, top);
        let moved = state.move_object(card, crate::duel::Zone::Library).unwrap();
        let library = &mut state.player_mut(me).library;
        library.retain(|other| *other != moved);
        library.push(moved);
        crate::duel::test_support::pass_both(&mut state);
        assert!(matches!(state.decision, Some(Decision::Scry { .. })));
        choose(&state, me)
    };
    assert_eq!(scry_with(1, "island"), Action::ScryKeep);
    assert_eq!(scry_with(6, "island"), Action::ScryBottom);
    assert_eq!(scry_with(1, "air_elemental"), Action::ScryBottom);
    assert_eq!(scry_with(3, "air_elemental"), Action::ScryKeep);
    assert_eq!(scry_with(1, "coral_merfolk"), Action::ScryKeep);
}

#[test]
fn puts_a_counter_on_its_own_best_creature_not_on_the_enemys() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_on_battlefield(&mut state, me, "elvish_visionary");
    put_on_battlefield(&mut state, opponent, "serra_angel");
    let beetle = put_in_hand(&mut state, me, "ironshell_beetle");
    apply_action(&mut state, me, cast(beetle)).unwrap();
    crate::duel::test_support::pass_both(&mut state);
    let Action::ChooseTargets { targets } = choose(&state, me) else {
        panic!("expected to aim the Beetle's ability");
    };
    let Target::Object { object } = targets[0] else {
        panic!("expected a creature");
    };
    assert_eq!(state.objects[&object].owner, me);
}
// ---- Activated abilities ----

#[test]
fn uses_an_ability_that_gains_something_and_not_one_that_only_costs() {
    // A ping that kills a creature.
    let (mut state, me, opponent) = empty_board(1);
    let pyromancer = put_on_battlefield(&mut state, me, "prodigal_pyromancer");
    let goblin = put_on_battlefield(&mut state, opponent, "raging_goblin");
    assert_eq!(
        choose(&state, me),
        Action::ActivateAbility {
            source: pyromancer,
            ability: 0,
            targets: vec![at(goblin)],
            sacrifice: None
        }
    );

    // Four spare mana for a card.
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 4);
    let sailor = put_on_battlefield(&mut state, me, "spectral_sailor");
    assert_eq!(
        choose(&state, me),
        Action::ActivateAbility {
            source: sailor,
            ability: 0,
            targets: vec![],
            sacrifice: None
        }
    );

    // A creature is not worth a scry, nor a pump nobody will feel.
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "viscera_seer");
    put_on_battlefield(&mut state, me, "bloodthrone_vampire");
    put_on_battlefield(&mut state, me, "grizzly_bears");
    assert_eq!(choose(&state, me), Action::PassPriority);
}
// ---- Enchantments ----

#[test]
fn casts_an_anthem_and_puts_a_shrinking_aura_on_the_enemy() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    let anthem = put_in_hand(&mut state, me, "glorious_anthem");
    assert_eq!(choose(&state, me), cast(anthem));

    let (mut state, me, opponent_two) = empty_board(1);
    let _ = opponent;
    put_lands(&mut state, me, "swamp", 1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent_two, "grizzly_bears");
    let weight = put_in_hand(&mut state, me, "dead_weight");
    assert_eq!(choose(&state, me), cast_at(weight, at(theirs)));

    // On a bigger creature it is still worth it: the creature is worth less.
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let weight = put_in_hand(&mut state, me, "dead_weight");
    assert_eq!(choose(&state, me), cast_at(weight, at(giant)));
}
// ---- The whole pool ----

/// A deck for a colour pair: fifteen spells of those colours, picked from
/// the whole pool by `seed`, and five basic lands of each.
fn pool_deck(colors: [Color; 2], seed: u64) -> crate::duel::PlayerConfig {
    use crate::duel::{CardInstance, DuelRng, PlayerConfig};
    let data = data::embedded().unwrap();
    let mut spells: Vec<CardDefId> = data
        .cards
        .iter()
        .filter(|card| {
            !card.is_land()
                && !card.token
                && !card.colors.is_empty()
                && card.colors.iter().all(|color| colors.contains(color))
        })
        .map(|card| card.id.clone())
        .collect();
    DuelRng::from_seed(seed).shuffle(&mut spells);
    spells.truncate(15);
    let land = |color: Color| match color {
        Color::W => "plains",
        Color::U => "island",
        Color::B => "swamp",
        Color::R => "mountain",
        Color::G => "forest",
    };
    for color in colors {
        spells.extend((0..5).map(|_| CardDefId(land(color).to_string())));
    }
    PlayerConfig {
        library: spells.into_iter().map(CardInstance::new).collect(),
        starting_life: data.balance.duel.starting_life,
        opening_hand_size: data.balance.duel.opening_hand_size,
        starting_permanents: Vec::new(),
        emblems: Vec::new(),
        commander: None,
    }
}

/// Every card in the pool turns up in AI hands here, whatever decks the
/// game's own content happens to use: decks for all ten colour pairs play
/// each other to the end without an error or a stuck game.
#[test]
fn plays_whole_games_with_decks_drawn_from_the_whole_pool() {
    use Color::*;
    let pairs = [
        [W, U],
        [U, B],
        [B, R],
        [R, G],
        [G, W],
        [W, B],
        [U, R],
        [B, G],
        [R, W],
        [G, U],
    ];
    let data = data::embedded().unwrap();
    let mut cast_cards = std::collections::BTreeSet::new();
    for seed in 0..3u64 {
        for (index, pair) in pairs.iter().enumerate() {
            let other = pairs[(index + 1 + seed as usize) % pairs.len()];
            let config = crate::duel::DuelConfig {
                players: [pool_deck(*pair, seed), pool_deck(other, seed + 100)],
                rules: crate::duel::RulesConfig::from_balance(&data.balance.duel),
            };
            let mut state = new_duel(config, seed).unwrap();
            let mut steps = 0;
            while state.result.is_none() {
                steps += 1;
                assert!(steps < 60_000, "{pair:?} v {other:?}, seed {seed}: stuck");
                for event in step(&mut state).2 {
                    if let GameEvent::SpellCast { card, .. } = event {
                        cast_cards.extend(card.face.map(|face| face.def));
                    }
                }
            }
            state.assert_consistent();
        }
    }
    // Most of the pool gets cast along the way.
    let spells = data
        .cards
        .iter()
        .filter(|card| !card.is_land() && !card.token)
        .count();
    assert!(
        cast_cards.len() * 10 >= spells * 7,
        "only {} of {spells} spells were ever cast",
        cast_cards.len()
    );
}

/// The search applies actions without the legality check and the safety
/// copy; that must give exactly the game `apply_action` gives.
#[test]
fn the_trusted_way_of_applying_an_action_gives_the_same_game() {
    for seed in 0..4 {
        let mut state = new_duel(config("deck_d", "deck_g"), seed).unwrap();
        for _ in 0..50_000 {
            let Some(player) = state.waiting_on() else {
                break;
            };
            let action = choose(&state, player);
            let mut trusted = state.clone();
            let trusted_events = apply_trusted(&mut trusted, player, &action).unwrap();
            let events = apply_action(&mut state, player, action).unwrap();
            assert_eq!(trusted_events, events);
            assert_eq!(trusted, state);
        }
        assert!(state.result.is_some());
    }
}

/// An object remembers its card definition, which a save leaves out: a
/// restored state is the same state and plays on the same.
#[test]
fn a_state_restored_from_a_save_plays_on_the_same() {
    let mut state = new_duel(config("deck_a", "deck_e"), 5).unwrap();
    for _ in 0..150 {
        step(&mut state);
    }
    assert!(state.result.is_none());
    let json = serde_json::to_string(&state).unwrap();
    let mut restored: DuelState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, state);
    for _ in 0..50_000 {
        if state.result.is_some() {
            break;
        }
        let (_, action, events) = step(&mut state);
        let (_, restored_action, restored_events) = step(&mut restored);
        assert_eq!(restored_action, action);
        assert_eq!(restored_events, events);
    }
    assert_eq!(restored, state);
    assert!(state.result.is_some());
}
