//! Tests for the cards the balance pass added to the green-white pool, and
//! the small rules that came with them.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, cast, cast_at, empty_board, pass_both, pass_until, pass_until_other_decision,
    put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};
use crate::cards::Keyword;

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

/// Casts a card with no targets from hand and lets everything resolve.
fn play(state: &mut DuelState, player: PlayerId, card: &str) {
    let card = put_in_hand(state, player, card);
    act(state, player, cast(card));
    settle(state);
}

fn gain(state: &mut DuelState, player: PlayerId, amount: i32) {
    let mut events = Vec::new();
    super::damage::change_life(
        state,
        &mut events,
        player,
        amount,
        super::damage::LifeCause::Other,
    )
    .unwrap();
    super::stack::put_triggers_on_stack(state, &mut events).unwrap();
}

fn ability(source: ObjectId, ability: usize, targets: Vec<Target>) -> Action {
    Action::ActivateAbility {
        source,
        ability,
        targets,
        sacrifice: None,
    }
}

/// Puts a +1/+1 counter on a creature of `player`'s with Snakeskin Veil.
fn counter_on(state: &mut DuelState, player: PlayerId, creature: ObjectId) {
    put_lands(state, player, "forest", 1);
    let veil = put_in_hand(state, player, "snakeskin_veil");
    act(state, player, cast_at(veil, at(creature)));
    settle(state);
}

#[test]
fn abzan_falconer_outlasts_and_gives_flying_to_creatures_with_a_counter() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let falconer = put_on_battlefield(&mut state, me, "abzan_falconer");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "ironshell_beetle");
    assert!(!state.has_keyword(falconer, Keyword::Flying).unwrap());

    // Outlast: {W} and a tap for a counter, and then it flies itself.
    act(&mut state, me, ability(falconer, 0, Vec::new()));
    settle(&mut state);
    assert_eq!(stats(&state, falconer), (3, 4));
    assert!(state.object(falconer).unwrap().tapped);
    assert!(state.has_keyword(falconer, Keyword::Flying).unwrap());
    // Tapped, it can't outlast again.
    assert!(apply_action(&mut state, me, ability(falconer, 0, Vec::new())).is_err());

    assert!(!state.has_keyword(bears, Keyword::Flying).unwrap());
    counter_on(&mut state, me, bears);
    assert!(state.has_keyword(bears, Keyword::Flying).unwrap());
    // The opponent's creature with a counter gets nothing from it.
    state.object_mut(theirs).unwrap().plus_counters = 1;
    assert!(!state.has_keyword(theirs, Keyword::Flying).unwrap());

    // Only as a sorcery: an untapped one can't outlast in the opponent's
    // turn, with a Plains still untapped to pay for it.
    pass_until(&mut state, 2, Step::PrecombatMain);
    put_on_battlefield(&mut state, me, "abzan_falconer");
    act(&mut state, opponent, Action::PassPriority);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert!(
        !legal_actions(&state, me)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { .. }))
    );
}

#[test]
fn abzan_battle_priest_gives_lifelink_to_creatures_with_a_counter() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 1);
    let priest = put_on_battlefield(&mut state, me, "abzan_battle_priest");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let plain = put_on_battlefield(&mut state, me, "glory_seeker");
    assert_eq!(stats(&state, priest), (3, 2));
    counter_on(&mut state, me, bears);
    assert!(state.has_keyword(bears, Keyword::Lifelink).unwrap());
    assert!(!state.has_keyword(plain, Keyword::Lifelink).unwrap());
    assert!(!state.has_keyword(priest, Keyword::Lifelink).unwrap());

    let life = (state.player(me).life, state.player(opponent).life);
    attack(&mut state, &[bears, plain]);
    pass_until(&mut state, 1, Step::PostcombatMain);
    assert_eq!(state.player(opponent).life, life.1 - 5);
    assert_eq!(state.player(me).life, life.0 + 3);

    act(&mut state, me, ability(priest, 0, Vec::new()));
    settle(&mut state);
    assert_eq!(stats(&state, priest), (4, 3));
    assert!(state.has_keyword(priest, Keyword::Lifelink).unwrap());
}

#[test]
fn colossal_majesty_draws_at_upkeep_only_beside_a_creature_of_power_four() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "colossal_majesty");
    put_on_battlefield(&mut state, me, "centaur_courser");
    // My next turn: only the draw step's card.
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(state.player(me).hand.len(), 1);

    // With a 4/4 beside it, the turn after brings two.
    put_on_battlefield(&mut state, me, "rumbling_baloth");
    pass_until(&mut state, 5, Step::PrecombatMain);
    assert_eq!(state.player(me).hand.len(), 3);
}

#[test]
fn elemental_bond_draws_for_a_creature_of_power_three_or_more() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 9);
    put_on_battlefield(&mut state, me, "elemental_bond");
    play(&mut state, me, "grizzly_bears");
    assert_eq!(state.player(me).hand.len(), 0);
    play(&mut state, me, "centaur_courser");
    assert_eq!(state.player(me).hand.len(), 1);
    play(&mut state, me, "rumbling_baloth");
    assert_eq!(state.player(me).hand.len(), 2);
    // The opponent's creatures draw me nothing.
    put_lands(&mut state, opponent, "mountain", 4);
    pass_until(&mut state, 2, Step::PrecombatMain);
    let hand = state.player(me).hand.len();
    play(&mut state, opponent, "hill_giant");
    assert_eq!(state.player(me).hand.len(), hand);
}

#[test]
fn leatherback_baloth_is_a_four_five_for_three_green() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, me, "plains", 1);
    let baloth = put_in_hand(&mut state, me, "leatherback_baloth");
    // Two Forests and a Plains don't pay {G}{G}{G}.
    assert!(apply_action(&mut state, me, cast(baloth)).is_err());
    put_lands(&mut state, me, "forest", 1);
    act(&mut state, me, cast(baloth));
    settle(&mut state);
    let baloth = named(&state, me, "Leatherback Baloth")[0];
    assert_eq!(stats(&state, baloth), (4, 5));
}

#[test]
fn eidolon_of_blossoms_draws_for_itself_and_for_each_enchantment_after() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    put_lands(&mut state, me, "plains", 2);
    play(&mut state, me, "eidolon_of_blossoms");
    assert_eq!(state.player(me).hand.len(), 1);
    play(&mut state, me, "intangible_virtue");
    assert_eq!(state.player(me).hand.len(), 2);
    // A creature that is no enchantment draws nothing, nor does an
    // enchantment of the opponent's.
    play(&mut state, me, "grizzly_bears");
    assert_eq!(state.player(me).hand.len(), 2);
    put_on_battlefield(&mut state, opponent, "phyrexian_arena");
    assert_eq!(state.player(me).hand.len(), 2);
}

#[test]
fn yavimaya_enchantress_counts_every_enchantment_on_the_battlefield() {
    let (mut state, me, opponent) = empty_board(1);
    let enchantress = put_on_battlefield(&mut state, me, "yavimaya_enchantress");
    assert_eq!(stats(&state, enchantress), (2, 2));
    put_on_battlefield(&mut state, me, "intangible_virtue");
    put_on_battlefield(&mut state, me, "oakheart_dryads");
    put_on_battlefield(&mut state, opponent, "phyrexian_arena");
    assert_eq!(stats(&state, enchantress), (5, 5));
}

#[test]
fn celestial_ancient_puts_a_counter_on_each_of_my_creatures_when_i_cast_an_enchantment() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 4);
    let ancient = put_on_battlefield(&mut state, me, "celestial_ancient");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    assert!(state.has_keyword(ancient, Keyword::Flying).unwrap());
    play(&mut state, me, "intangible_virtue");
    assert_eq!(stats(&state, ancient), (4, 4));
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(stats(&state, theirs), (3, 3));
    // A creature spell is not an enchantment spell.
    play(&mut state, me, "glory_seeker");
    assert_eq!(stats(&state, ancient), (4, 4));
}

#[test]
fn battle_mastery_gives_double_strike() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let mastery = put_in_hand(&mut state, me, "battle_mastery");
    act(&mut state, me, cast_at(mastery, at(bears)));
    settle(&mut state);
    assert!(state.has_keyword(bears, Keyword::DoubleStrike).unwrap());
    let life = state.player(opponent).life;
    attack(&mut state, &[bears]);
    pass_until(&mut state, 1, Step::PostcombatMain);
    assert_eq!(state.player(opponent).life, life - 4);
}

#[test]
fn angelic_accord_makes_an_angel_after_four_life_in_a_turn() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "angelic_accord");
    gain(&mut state, me, 3);
    pass_until(&mut state, 1, Step::End);
    settle(&mut state);
    assert!(named(&state, me, "Angel").is_empty());

    pass_until(&mut state, 3, Step::PrecombatMain);
    gain(&mut state, me, 4);
    pass_until(&mut state, 3, Step::End);
    settle(&mut state);
    let angels = named(&state, me, "Angel");
    assert_eq!(angels.len(), 1);
    assert_eq!(stats(&state, angels[0]), (4, 4));
    assert!(state.has_keyword(angels[0], Keyword::Flying).unwrap());
}

#[test]
fn phantom_general_makes_my_tokens_bigger_and_nothing_else() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let general = put_on_battlefield(&mut state, me, "phantom_general");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    play(&mut state, me, "raise_the_alarm");
    for soldier in named(&state, me, "Soldier") {
        assert_eq!(stats(&state, soldier), (2, 2));
    }
    assert_eq!(stats(&state, general), (2, 3));
    assert_eq!(stats(&state, bears), (2, 2));
    let theirs = put_on_battlefield(&mut state, opponent, "goblin_token");
    assert_eq!(stats(&state, theirs), (1, 1));
}

#[test]
fn collective_unconscious_draws_a_card_for_each_of_my_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 6);
    for _ in 0..3 {
        put_on_battlefield(&mut state, me, "grizzly_bears");
    }
    put_on_battlefield(&mut state, opponent, "hill_giant");
    play(&mut state, me, "collective_unconscious");
    assert_eq!(state.player(me).hand.len(), 3);
}

#[test]
fn spearbreaker_behemoth_makes_big_creatures_indestructible() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let behemoth = put_on_battlefield(&mut state, me, "spearbreaker_behemoth");
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let baloth = put_on_battlefield(&mut state, me, "rumbling_baloth");
    assert_eq!(stats(&state, behemoth), (5, 5));
    assert!(
        state
            .has_keyword(behemoth, Keyword::Indestructible)
            .unwrap()
    );
    // A creature of power 4 is too small to aim at.
    assert!(apply_action(&mut state, me, ability(behemoth, 0, vec![at(baloth)])).is_err());
    act(&mut state, me, ability(behemoth, 0, vec![at(dreadmaw)]));
    settle(&mut state);
    assert!(
        state
            .has_keyword(dreadmaw, Keyword::Indestructible)
            .unwrap()
    );

    // Murder doesn't destroy it this turn.
    act(&mut state, me, Action::PassPriority);
    put_lands(&mut state, opponent, "swamp", 3);
    let murder = put_in_hand(&mut state, opponent, "murder");
    act(&mut state, opponent, cast_at(murder, at(dreadmaw)));
    settle(&mut state);
    assert!(state.is_on_battlefield(dreadmaw));
    pass_until(&mut state, 2, Step::Upkeep);
    assert!(
        !state
            .has_keyword(dreadmaw, Keyword::Indestructible)
            .unwrap()
    );
}

#[test]
fn high_sentinels_grow_for_each_other_creature_of_mine_with_a_counter() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 8);
    let sentinels = put_on_battlefield(&mut state, me, "high_sentinels_of_arashin");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let seeker = put_on_battlefield(&mut state, me, "glory_seeker");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    assert_eq!(stats(&state, sentinels), (3, 4));
    assert!(state.has_keyword(sentinels, Keyword::Flying).unwrap());

    // {3}{W}: a counter on any creature.
    act(&mut state, me, ability(sentinels, 1, vec![at(bears)]));
    settle(&mut state);
    assert_eq!(stats(&state, bears), (3, 3));
    assert_eq!(stats(&state, sentinels), (4, 5));
    // A counter on themselves is a counter, but not another creature's;
    // one on the opponent's creature counts for nothing.
    act(&mut state, me, ability(sentinels, 1, vec![at(sentinels)]));
    settle(&mut state);
    assert_eq!(stats(&state, sentinels), (5, 6));
    state.object_mut(theirs).unwrap().plus_counters = 1;
    assert_eq!(stats(&state, sentinels), (5, 6));
    counter_on(&mut state, me, seeker);
    assert_eq!(stats(&state, sentinels), (6, 7));
}

#[test]
fn champion_of_lambholt_grows_with_my_creatures_and_stops_weaker_blockers() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 4);
    let champion = put_on_battlefield(&mut state, me, "champion_of_lambholt");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let piker = put_on_battlefield(&mut state, opponent, "goblin_piker");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    assert_eq!(stats(&state, champion), (1, 1));
    // Two creatures enter under my control; the opponent's don't count.
    play(&mut state, me, "grizzly_bears");
    play(&mut state, me, "grizzly_bears");
    assert_eq!(stats(&state, champion), (3, 3));

    // Power 2 is less than the Champion's 3, so the Piker can't block
    // anything of mine; the 3/3 Giant can.
    attack(&mut state, &[bears, champion]);
    pass_until_other_decision(&mut state);
    assert!(matches!(
        state.decision,
        Some(Decision::DeclareBlockers { player }) if player == opponent
    ));
    let blocks: Vec<(ObjectId, ObjectId)> = legal_actions(&state, opponent)
        .into_iter()
        .filter_map(|action| match action {
            Action::Block { blocker, attacker } => Some((blocker, attacker)),
            _ => None,
        })
        .collect();
    assert!(blocks.contains(&(giant, bears)));
    assert!(blocks.contains(&(giant, champion)));
    assert!(blocks.iter().all(|(blocker, _)| *blocker != piker));
    assert!(
        apply_action(
            &mut state,
            opponent,
            Action::Block {
                blocker: piker,
                attacker: bears
            }
        )
        .is_err()
    );
}

#[test]
fn the_champions_rule_does_nothing_for_the_other_sides_attackers() {
    let (mut state, me, opponent) = empty_board(1);
    let champion = put_on_battlefield(&mut state, me, "champion_of_lambholt");
    let attacker = put_on_battlefield(&mut state, opponent, "hill_giant");
    // My 1/1 Champion may block the opponent's 3/3: the rule is about
    // creatures blocking mine.
    pass_until(&mut state, 2, Step::PrecombatMain);
    attack(&mut state, &[attacker]);
    pass_until_other_decision(&mut state);
    assert!(legal_actions(&state, me).contains(&Action::Block {
        blocker: champion,
        attacker
    }));
}

#[test]
fn hardened_scales_adds_a_counter_and_adds_up_with_conclave_mentor() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "hardened_scales");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    counter_on(&mut state, me, bears);
    assert_eq!(stats(&state, bears), (4, 4));
    put_on_battlefield(&mut state, me, "conclave_mentor");
    counter_on(&mut state, me, bears);
    assert_eq!(stats(&state, bears), (7, 7));
    // The opponent's creatures get what they are given and no more.
    put_lands(&mut state, me, "plains", 4);
    let sentinels = put_on_battlefield(&mut state, me, "high_sentinels_of_arashin");
    act(&mut state, me, ability(sentinels, 1, vec![at(theirs)]));
    settle(&mut state);
    assert_eq!(stats(&state, theirs), (4, 4));
}
