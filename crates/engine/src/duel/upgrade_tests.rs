//! Tests for the upgraded sides of the Warden's cards (`"upgraded"` in a
//! card file): a table of what every one of them is, and a test of each
//! one that changes what its card does.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, Step};
use super::test_support::{
    cast, cast_at, empty_board, pass_both, pass_until, put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};
use crate::cards::{CardDef, CardDefId, Color, Keyword};
use crate::data;

use Keyword::{FirstStrike, Flying, Hexproof, Indestructible, Lifelink, Reach, Trample, Vigilance};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

/// Passes priority until the stack is empty.
fn settle(state: &mut DuelState) {
    pass_both(state);
    for _ in 0..40 {
        if state.stack.is_empty() {
            break;
        }
        pass_both(state);
    }
}

fn stats(state: &DuelState, creature: ObjectId) -> (i32, i32) {
    (
        state.power(creature).unwrap(),
        state.toughness(creature).unwrap(),
    )
}

fn has(state: &DuelState, creature: ObjectId, keyword: Keyword) -> bool {
    state.has_keyword(creature, keyword).unwrap()
}

fn named(state: &DuelState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().name == name)
        .map(|permanent| permanent.id)
        .collect()
}

/// A board with plenty of both colours of mana for the active player.
fn board() -> (DuelState, PlayerId, PlayerId) {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 8);
    put_lands(&mut state, me, "plains", 8);
    (state, me, opponent)
}

/// Casts a card with no targets from hand and lets everything resolve.
fn play(state: &mut DuelState, player: PlayerId, card: &str) {
    let card = put_in_hand(state, player, card);
    act(state, player, cast(card));
    settle(state);
}

/// Casts a permanent with no targets and returns it on the battlefield.
fn played(state: &mut DuelState, player: PlayerId, card: &str) -> ObjectId {
    play(state, player, card);
    state
        .permanents_of(player)
        .find(|permanent| state.def(permanent.id).unwrap().id.as_str() == card)
        .unwrap()
        .id
}

/// Casts a card at one target and lets everything resolve.
fn play_at(state: &mut DuelState, player: PlayerId, card: &str, target: ObjectId) {
    let card = put_in_hand(state, player, card);
    act(state, player, cast_at(card, at(target)));
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

fn lands_of(state: &DuelState, player: PlayerId) -> usize {
    state
        .permanents_of(player)
        .filter(|permanent| state.def(permanent.id).unwrap().is_land())
        .count()
}

// ---- Every upgraded side, as a table ----

const NONE: &[Keyword] = &[];

/// A card, and its upgraded side's cost, power and toughness, keywords and
/// the +1/+1 counters it enters with.
type Row = (
    &'static str,
    &'static str,
    Option<(i32, i32)>,
    &'static [Keyword],
    u32,
);

const SIDES: [Row; 112] = [
    // The starting deck.
    ("glory_seeker", "{1}{W}", Some((3, 3)), NONE, 0),
    ("grizzly_bears", "{G}", Some((2, 2)), NONE, 0),
    ("youthful_knight", "{1}{W}", Some((3, 2)), &[FirstStrike], 0),
    ("elvish_visionary", "{1}{G}", Some((2, 2)), NONE, 0),
    ("centaur_courser", "{2}{G}", Some((4, 4)), NONE, 0),
    ("giant_spider", "{3}{G}", Some((3, 5)), &[Reach], 0),
    ("siege_mastodon", "{4}{W}", Some((4, 6)), &[Vigilance], 0),
    ("giant_growth", "{G}", None, NONE, 0),
    // Commons.
    ("attended_knight", "{2}{W}", Some((2, 2)), &[FirstStrike], 0),
    ("call_of_the_conclave", "{G}{W}", None, NONE, 0),
    ("captains_call", "{3}{W}", None, NONE, 0),
    ("inspired_charge", "{1}{W}{W}", None, NONE, 0),
    ("might_of_the_masses", "{G}", None, NONE, 0),
    ("raise_the_alarm", "{1}{W}", None, NONE, 0),
    ("sworn_companions", "{2}{W}", None, NONE, 0),
    ("crowned_ceratok", "{3}{G}", Some((4, 3)), &[Trample], 1),
    ("ironshell_beetle", "{1}{G}", Some((1, 1)), NONE, 0),
    ("lightwalker", "{1}{W}", Some((2, 1)), NONE, 1),
    ("pridemalkin", "{2}{G}", Some((3, 2)), NONE, 0),
    ("snakeskin_veil", "{G}", None, NONE, 0),
    ("unruly_mob", "{1}{W}", Some((2, 2)), NONE, 0),
    ("angel_of_mercy", "{3}{W}", Some((3, 3)), &[Flying], 0),
    ("centaur_healer", "{1}{G}{W}", Some((3, 3)), NONE, 0),
    ("dawning_angel", "{4}{W}", Some((4, 4)), &[Flying], 0),
    ("healers_hawk", "{W}", Some((1, 2)), &[Flying, Lifelink], 0),
    ("revitalize", "{1}{W}", None, NONE, 0),
    (
        "colossal_dreadmaw",
        "{3}{G}{G}",
        Some((6, 6)),
        &[Trample],
        0,
    ),
    ("druid_of_the_cowl", "{1}{G}", Some((2, 4)), NONE, 0),
    ("honey_mammoth", "{4}{G}{G}", Some((6, 6)), NONE, 0),
    ("llanowar_elves", "{G}", Some((1, 2)), NONE, 0),
    ("rabid_bite", "{G}", None, NONE, 0),
    ("rampant_growth", "{1}{G}", None, NONE, 0),
    ("rumbling_baloth", "{2}{G}{G}", Some((5, 5)), NONE, 0),
    (
        "sentinel_spider",
        "{2}{G}{G}",
        Some((4, 4)),
        &[Reach, Vigilance],
        0,
    ),
    ("stampeding_rhino", "{4}{G}", Some((5, 5)), &[Trample], 0),
    ("cartouche_of_solidarity", "{W}", None, NONE, 0),
    ("divine_favor", "{1}{W}", None, NONE, 0),
    ("fists_of_ironwood", "{1}{G}", None, NONE, 0),
    ("gladecover_scout", "{G}", Some((2, 1)), &[Hexproof], 0),
    ("knightly_valor", "{3}{W}", None, NONE, 0),
    ("setessan_training", "{1}{G}", None, NONE, 0),
    ("smite_the_monstrous", "{3}{W}", None, NONE, 0),
    // Uncommons.
    ("angel_of_renewal", "{4}{W}", Some((4, 4)), &[Flying], 0),
    ("bestial_menace", "{2}{G}{G}", None, NONE, 0),
    ("good_fortune_unicorn", "{1}{G}{W}", Some((3, 3)), NONE, 0),
    ("impassioned_orator", "{1}{W}", Some((2, 3)), NONE, 0),
    ("intangible_virtue", "{W}", None, NONE, 0),
    ("jade_mage", "{1}{G}", Some((2, 1)), NONE, 0),
    ("overrun", "{1}{G}{G}{G}", None, NONE, 0),
    ("phantom_general", "{2}{W}", Some((2, 3)), NONE, 0),
    ("trostanis_summoner", "{4}{G}{W}", Some((1, 1)), NONE, 0),
    ("woodland_champion", "{1}{G}", Some((2, 2)), NONE, 1),
    ("pacifism", "{W}", None, NONE, 0),
    ("abzan_battle_priest", "{3}{W}", Some((3, 2)), NONE, 1),
    ("abzan_falconer", "{2}{W}", Some((3, 3)), NONE, 0),
    ("armorcraft_judge", "{2}{G}", Some((3, 3)), NONE, 0),
    ("basris_solidarity", "{W}", None, NONE, 0),
    ("conclave_mentor", "{G}{W}", Some((2, 3)), NONE, 0),
    ("inspiring_call", "{1}{G}", None, NONE, 0),
    ("invigorating_surge", "{1}{G}", None, NONE, 0),
    ("iridescent_hornbeetle", "{3}{G}", Some((3, 4)), NONE, 0),
    ("ridgescale_tusker", "{3}{G}{G}", Some((6, 6)), NONE, 0),
    ("ajanis_pridemate", "{1}{W}", Some((2, 3)), NONE, 0),
    ("angelic_accord", "{2}{W}", None, NONE, 0),
    ("celestial_unicorn", "{2}{W}", Some((3, 3)), NONE, 0),
    ("griffin_aerie", "{1}{W}", None, NONE, 0),
    ("nyx_fleece_ram", "{1}{W}", Some((0, 5)), NONE, 0),
    ("soulmender", "{W}", Some((1, 1)), NONE, 0),
    ("twinblade_paladin", "{3}{W}", Some((3, 3)), NONE, 0),
    ("colossal_majesty", "{1}{G}", None, NONE, 0),
    ("elemental_bond", "{1}{G}", None, NONE, 0),
    ("garruks_uprising", "{1}{G}", None, NONE, 0),
    ("leatherback_baloth", "{G}{G}{G}", Some((5, 5)), NONE, 0),
    ("nessian_hornbeetle", "{1}{G}", Some((2, 3)), NONE, 0),
    ("pelakka_wurm", "{3}{G}{G}{G}", Some((7, 7)), &[Trample], 0),
    ("sight_of_the_scalelords", "{2}{G}", None, NONE, 0),
    ("ancestral_mask", "{1}{G}", None, NONE, 0),
    ("battle_mastery", "{1}{W}", None, NONE, 0),
    ("ethereal_armor", "{W}", None, NONE, 0),
    ("oakheart_dryads", "{2}{G}", Some((2, 3)), NONE, 0),
    ("rancor", "{G}", None, NONE, 0),
    ("satyr_enchanter", "{1}{G}{W}", Some((3, 3)), NONE, 0),
    ("yavimaya_enchantress", "{1}{G}", Some((2, 2)), NONE, 0),
    // Rares.
    ("benalish_marshal", "{W}{W}{W}", Some((3, 4)), NONE, 0),
    ("cathars_crusade", "{2}{W}{W}", None, NONE, 0),
    ("champion_of_lambholt", "{1}{G}{G}", Some((2, 2)), NONE, 0),
    ("collective_blessing", "{2}{G}{G}{W}", None, NONE, 0),
    ("collective_unconscious", "{3}{G}{G}", None, NONE, 0),
    ("glorious_anthem", "{W}{W}", None, NONE, 0),
    (
        "avatar_of_the_resolute",
        "{G}{G}",
        Some((3, 3)),
        &[Reach, Trample],
        0,
    ),
    ("hardened_scales", "{G}", None, NONE, 0),
    (
        "high_sentinels_of_arashin",
        "{3}{W}",
        Some((3, 4)),
        &[Flying],
        0,
    ),
    ("kalonian_hydra", "{3}{G}{G}", Some((0, 0)), &[Trample], 5),
    ("luminarch_aspirant", "{1}{W}", Some((1, 2)), NONE, 0),
    (
        "archangel_of_thune",
        "{3}{W}{W}",
        Some((3, 5)),
        &[Flying, Lifelink],
        0,
    ),
    ("angelic_chorus", "{2}{W}{W}", None, NONE, 0),
    ("resplendent_angel", "{1}{W}{W}", Some((3, 3)), &[Flying], 0),
    (
        "speaker_of_the_heavens",
        "{W}",
        Some((1, 1)),
        &[Lifelink, Vigilance],
        0,
    ),
    (
        "armada_wurm",
        "{1}{G}{G}{W}{W}",
        Some((5, 5)),
        &[Trample],
        0,
    ),
    (
        "gigantosaurus",
        "{G}{G}{G}{G}{G}",
        Some((10, 10)),
        &[Trample],
        0,
    ),
    ("overwhelming_stampede", "{2}{G}{G}", None, NONE, 0),
    (
        "spearbreaker_behemoth",
        "{4}{G}{G}",
        Some((5, 5)),
        &[Indestructible],
        0,
    ),
    ("steel_leaf_champion", "{G}{G}{G}", Some((5, 5)), NONE, 0),
    ("terra_stomper", "{2}{G}{G}{G}", Some((8, 8)), &[Trample], 0),
    ("thragtusk", "{4}{G}", Some((5, 4)), NONE, 0),
    ("celestial_ancient", "{2}{W}{W}", Some((3, 3)), &[Flying], 0),
    ("daybreak_coronet", "{W}{W}", None, NONE, 0),
    ("eidolon_of_blossoms", "{2}{G}{G}", Some((2, 3)), NONE, 0),
    ("kor_spiritdancer", "{1}{W}", Some((0, 3)), NONE, 0),
    ("mantle_of_the_wolf", "{2}{G}", None, NONE, 0),
    ("setessan_champion", "{2}{G}", Some((2, 3)), NONE, 0),
    ("sigil_of_the_empty_throne", "{2}{W}{W}", None, NONE, 0),
];

fn side_of(card: &str) -> &'static CardDef {
    let cards = &data::embedded().unwrap().cards;
    let card = cards.get(&CardDefId(card.to_string())).unwrap();
    cards.get(card.upgrade.as_ref().unwrap()).unwrap()
}

fn side_id(card: &str) -> String {
    format!("{card}+")
}

#[test]
fn every_card_the_warden_can_hold_has_an_upgraded_side_and_no_other_card_has() {
    let data = data::embedded().unwrap();
    let warden = data
        .run
        .planeswalkers
        .iter()
        .find(|planeswalker| planeswalker.id.0 == "warden")
        .unwrap();
    let starts_with = |card: &CardDef| warden.spells.iter().any(|entry| entry.card == card.id);
    let offered = |card: &CardDef| {
        !card.is_land()
            && !card.token
            && !card.is_legendary()
            && !card.reserved
            && !card.colors.is_empty()
            && card
                .colors
                .iter()
                .all(|color| warden.colors.contains(color))
    };
    for card in data.cards.iter() {
        let listed = SIDES.iter().any(|row| row.0 == card.id.as_str());
        assert_eq!(starts_with(card) || offered(card), listed, "{}", card.id);
        assert_eq!(card.upgrade.is_some(), listed, "{}", card.id);
    }
    assert_eq!(data.cards.upgraded_sides().count(), SIDES.len());
}

#[test]
fn every_upgraded_side_is_what_the_table_says_and_differs_from_its_card() {
    let cards = &data::embedded().unwrap().cards;
    for (id, cost, size, keywords, counters) in SIDES {
        let card = cards.get(&CardDefId(id.to_string())).unwrap();
        let side = side_of(id);
        assert_eq!(side.id.as_str(), side_id(id), "{id}");
        assert_eq!(side.name, format!("{}+", card.name), "{id}");
        assert_eq!(side.upgrade_of.as_ref(), Some(&card.id), "{id}");
        assert_eq!(side.mana_cost.to_string(), cost, "{id}");
        assert_eq!(side.power.zip(side.toughness), size, "{id}");
        let mut listed = side.keywords.clone();
        listed.sort();
        assert_eq!(listed, keywords, "{id}");
        assert_eq!(side.enters_with_counters, counters, "{id}");
        // The rest of the card is the card: its name aside, only these and
        // its abilities and text may differ.
        assert_eq!(
            (&side.colors, &side.types, &side.subtypes, side.rarity),
            (&card.colors, &card.types, &card.subtypes, card.rarity),
            "{id}"
        );
        let changed = side.mana_cost != card.mana_cost
            || side.power != card.power
            || side.toughness != card.toughness
            || side.keywords != card.keywords
            || side.enters_with_counters != card.enters_with_counters
            || side.abilities != card.abilities;
        assert!(changed, "{id}");
        // Never dearer, and never smaller.
        assert!(
            side.mana_cost.mana_value() <= card.mana_cost.mana_value(),
            "{id}"
        );
        assert!(
            side.power >= card.power && side.toughness >= card.toughness,
            "{id}"
        );
        // A change to what the card does is a change to its text.
        let rules_changed = side.keywords != card.keywords
            || side.enters_with_counters != card.enters_with_counters
            || side.abilities != card.abilities;
        assert_eq!(rules_changed, side.oracle_text != card.oracle_text, "{id}");
    }
}

#[test]
fn an_upgraded_creature_is_its_size_on_the_battlefield() {
    for (id, _, size, keywords, counters) in SIDES {
        let Some((power, toughness)) = size else {
            continue;
        };
        let (mut state, me, _) = empty_board(1);
        let creature = put_on_battlefield(&mut state, me, &side_id(id));
        let counters = i32::try_from(counters).unwrap();
        assert_eq!(
            stats(&state, creature),
            (power + counters, toughness + counters),
            "{id}"
        );
        for &keyword in keywords {
            assert!(has(&state, creature, keyword), "{id} {keyword:?}");
        }
    }
}

/// Whether `player` may cast `card` right now, at anything.
fn can_cast(state: &DuelState, player: PlayerId, card: ObjectId) -> bool {
    legal_actions(state, player)
        .iter()
        .any(|action| matches!(action, Action::CastSpell { card: cast, .. } if *cast == card))
}

#[test]
fn an_upgraded_card_is_cast_for_its_own_cost_and_no_less() {
    for (id, cost, ..) in SIDES {
        // Daybreak Coronet needs a creature that already wears an Aura: its
        // cost is the printed one, and its own test casts it.
        if id == "daybreak_coronet" {
            continue;
        }
        let side = side_of(id);
        let colored: Vec<Color> = side.mana_cost.colored().collect();
        let generic = side.mana_cost.mana_value() as usize - colored.len();
        // One land short, then exactly enough.
        for short in [true, false] {
            let (mut state, me, opponent) = empty_board(1);
            put_on_battlefield(&mut state, me, "grizzly_bears");
            put_on_battlefield(&mut state, opponent, "hill_giant");
            let (mut forests, mut plains) = (generic, 0);
            for color in &colored {
                match color {
                    Color::G => forests += 1,
                    _ => plains += 1,
                }
            }
            if short && forests > 0 {
                forests -= 1;
            } else if short {
                plains -= 1;
            }
            put_lands(&mut state, me, "forest", forests);
            put_lands(&mut state, me, "plains", plains);
            let card = put_in_hand(&mut state, me, &side_id(id));
            assert_eq!(state.casting_cost(card).unwrap().to_string(), cost, "{id}");
            assert_eq!(can_cast(&state, me, card), !short, "{id}, short: {short}");
        }
    }
}

// ---- The starting deck ----

#[test]
fn giant_growth_upgraded_gives_four_and_trample() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    play_at(&mut state, me, "giant_growth+", bears);
    assert_eq!(stats(&state, bears), (6, 6));
    assert!(has(&state, bears, Trample));
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, bears), (2, 2));
    assert!(!has(&state, bears, Trample));
}

// ---- Go wide ----

#[test]
fn attended_knight_upgraded_brings_two_soldiers() {
    let (mut state, me, _) = board();
    play(&mut state, me, "attended_knight+");
    assert_eq!(named(&state, me, "Soldier").len(), 2);
}

#[test]
fn call_of_the_conclave_upgraded_makes_a_centaur_and_a_soldier() {
    let (mut state, me, _) = board();
    play(&mut state, me, "call_of_the_conclave+");
    let centaurs = named(&state, me, "Centaur");
    assert_eq!(centaurs.len(), 1);
    assert_eq!(stats(&state, centaurs[0]), (3, 3));
    assert_eq!(named(&state, me, "Soldier").len(), 1);
}

#[test]
fn the_token_spells_upgraded_make_one_more_token() {
    for (card, token, count) in [
        ("captains_call+", "Soldier", 4),
        ("raise_the_alarm+", "Soldier", 3),
        ("sworn_companions+", "Soldier", 3),
    ] {
        let (mut state, me, _) = board();
        play(&mut state, me, card);
        let tokens = named(&state, me, token);
        assert_eq!(tokens.len(), count, "{card}");
        let lifelink = card == "sworn_companions+";
        assert!(
            tokens
                .iter()
                .all(|&token| has(&state, token, Lifelink) == lifelink)
        );
    }
}

#[test]
fn might_of_the_masses_upgraded_gives_trample_too() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    put_on_battlefield(&mut state, me, "glory_seeker");
    play_at(&mut state, me, "might_of_the_masses+", bears);
    assert_eq!(stats(&state, bears), (4, 4));
    assert!(has(&state, bears, Trample));
}

#[test]
fn jade_mage_upgraded_makes_a_saproling_for_two_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let plain = put_on_battlefield(&mut state, me, "jade_mage");
    assert!(apply_action(&mut state, me, ability(plain, 0, Vec::new())).is_err());
    let mage = put_on_battlefield(&mut state, me, "jade_mage+");
    act(&mut state, me, ability(mage, 0, Vec::new()));
    settle(&mut state);
    assert_eq!(named(&state, me, "Saproling").len(), 1);
}

// ---- +1/+1 counters ----

#[test]
fn the_creatures_upgraded_to_enter_with_a_counter_do() {
    for (card, size) in [
        ("crowned_ceratok+", (5, 4)),
        ("lightwalker+", (3, 2)),
        ("woodland_champion+", (3, 3)),
        ("abzan_battle_priest+", (4, 3)),
        ("kalonian_hydra+", (5, 5)),
    ] {
        let (mut state, me, _) = board();
        let creature = played(&mut state, me, card);
        assert_eq!(stats(&state, creature), size, "{card}");
        let counters = if card == "kalonian_hydra+" { 5 } else { 1 };
        assert_eq!(state.object(creature).unwrap().plus_counters, counters);
        // They are counters like any other: the turn's count has them.
        assert_eq!(state.player(me).counters_put_this_turn, counters);
    }
}

#[test]
fn a_counter_it_enters_with_turns_on_what_asks_for_one() {
    let (mut state, me, _) = board();
    // Lightwalker flies from the start, and the Battle Priest has its own
    // lifelink.
    let walker = played(&mut state, me, "lightwalker+");
    assert!(has(&state, walker, Flying));
    let priest = played(&mut state, me, "abzan_battle_priest+");
    assert!(has(&state, priest, Lifelink));
    assert!(has(&state, walker, Lifelink));
}

#[test]
fn ironshell_beetle_and_snakeskin_veil_upgraded_put_two_counters() {
    // Alone, the Beetle's ability can only be aimed at itself.
    let (mut state, me, _) = board();
    let beetle = played(&mut state, me, "ironshell_beetle+");
    assert_eq!(stats(&state, beetle), (3, 3));

    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    play_at(&mut state, me, "snakeskin_veil+", bears);
    assert_eq!(stats(&state, bears), (4, 4));
    assert!(has(&state, bears, Hexproof));
}

#[test]
fn hardened_scales_upgraded_puts_a_counter_as_it_enters() {
    let (mut state, me, opponent) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    // The Scales are there by then, so the one counter is two.
    play(&mut state, me, "hardened_scales+");
    assert_eq!(stats(&state, bears), (4, 4));
    assert_eq!(stats(&state, theirs), (3, 3));
}

#[test]
fn high_sentinels_upgraded_put_a_counter_for_three_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 3);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let plain = put_on_battlefield(&mut state, me, "high_sentinels_of_arashin");
    assert!(apply_action(&mut state, me, ability(plain, 1, vec![at(bears)])).is_err());
    let sentinels = put_on_battlefield(&mut state, me, "high_sentinels_of_arashin+");
    act(&mut state, me, ability(sentinels, 1, vec![at(bears)]));
    settle(&mut state);
    assert_eq!(stats(&state, bears), (3, 3));
}

// ---- Lifegain ----

#[test]
fn the_life_gainers_upgraded_gain_more() {
    for (card, life) in [
        ("centaur_healer+", 5),
        ("revitalize+", 5),
        ("honey_mammoth+", 8),
    ] {
        let (mut state, me, _) = board();
        let before = state.player(me).life;
        play(&mut state, me, card);
        assert_eq!(state.player(me).life, before + life, "{card}");
    }
}

#[test]
fn soulmender_upgraded_taps_for_two_life() {
    let (mut state, me, _) = empty_board(1);
    let mender = put_on_battlefield(&mut state, me, "soulmender+");
    let before = state.player(me).life;
    act(&mut state, me, ability(mender, 0, Vec::new()));
    settle(&mut state);
    assert_eq!(state.player(me).life, before + 2);
}

#[test]
fn nyx_fleece_ram_upgraded_gains_two_life_each_upkeep() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "nyx_fleece_ram+");
    let before = state.player(me).life;
    pass_until(&mut state, 3, Step::PrecombatMain);
    assert_eq!(state.player(me).life, before + 2);
}

#[test]
fn angelic_accord_and_griffin_aerie_upgraded_ask_for_one_life_less() {
    for (card, token, life) in [
        ("angelic_accord", "Angel", 3),
        ("griffin_aerie", "Griffin", 2),
    ] {
        // The card as printed makes nothing for that much life.
        for (id, made) in [(card.to_string(), 0), (side_id(card), 1)] {
            let (mut state, me, _) = empty_board(1);
            put_on_battlefield(&mut state, me, &id);
            gain(&mut state, me, life);
            pass_until(&mut state, 1, Step::End);
            settle(&mut state);
            assert_eq!(named(&state, me, token).len(), made, "{id}");
        }
    }
}

#[test]
fn twinblade_paladin_upgraded_has_double_strike_three_life_up() {
    let (mut state, me, _) = empty_board(1);
    let plain = put_on_battlefield(&mut state, me, "twinblade_paladin");
    let paladin = put_on_battlefield(&mut state, me, "twinblade_paladin+");
    gain(&mut state, me, 2);
    assert!(!has(&state, paladin, Keyword::DoubleStrike));
    gain(&mut state, me, 1);
    assert!(has(&state, paladin, Keyword::DoubleStrike));
    assert!(!has(&state, plain, Keyword::DoubleStrike));
}

#[test]
fn speaker_of_the_heavens_upgraded_makes_an_angel_five_life_up() {
    let (mut state, me, _) = empty_board(1);
    let plain = put_on_battlefield(&mut state, me, "speaker_of_the_heavens");
    let speaker = put_on_battlefield(&mut state, me, "speaker_of_the_heavens+");
    gain(&mut state, me, 4);
    assert!(apply_action(&mut state, me, ability(speaker, 0, Vec::new())).is_err());
    gain(&mut state, me, 1);
    assert!(apply_action(&mut state, me, ability(plain, 0, Vec::new())).is_err());
    act(&mut state, me, ability(speaker, 0, Vec::new()));
    settle(&mut state);
    assert_eq!(named(&state, me, "Angel").len(), 1);
}

#[test]
fn resplendent_angel_upgraded_pumps_for_five_mana() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "plains", 5);
    let plain = put_on_battlefield(&mut state, me, "resplendent_angel");
    assert!(apply_action(&mut state, me, ability(plain, 1, Vec::new())).is_err());
    let angel = put_on_battlefield(&mut state, me, "resplendent_angel+");
    act(&mut state, me, ability(angel, 1, Vec::new()));
    settle(&mut state);
    assert_eq!(stats(&state, angel), (5, 5));
    assert!(has(&state, angel, Lifelink));
}

// ---- Big creatures ----

#[test]
fn rampant_growth_upgraded_draws_a_card_as_well() {
    let (mut state, me, _) = board();
    let lands = lands_of(&state, me);
    play(&mut state, me, "rampant_growth+");
    assert_eq!(lands_of(&state, me), lands + 1);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn smite_the_monstrous_upgraded_destroys_a_creature_of_power_three() {
    let (mut state, me, opponent) = board();
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let plain = put_in_hand(&mut state, me, "smite_the_monstrous");
    assert!(apply_action(&mut state, me, cast_at(plain, at(giant))).is_err());
    let smite = put_in_hand(&mut state, me, "smite_the_monstrous+");
    assert!(apply_action(&mut state, me, cast_at(smite, at(bears))).is_err());
    act(&mut state, me, cast_at(smite, at(giant)));
    settle(&mut state);
    assert!(!state.is_on_battlefield(giant));
}

// ---- Auras and enchantments ----

#[test]
fn the_auras_upgraded_give_more() {
    for (card, size) in [
        ("cartouche_of_solidarity+", (4, 3)),
        ("divine_favor+", (4, 5)),
        ("fists_of_ironwood+", (3, 3)),
        ("setessan_training+", (4, 2)),
        ("rancor+", (4, 3)),
        // The Armor counts itself.
        ("ethereal_armor+", (3, 3)),
    ] {
        let (mut state, me, _) = board();
        let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
        play_at(&mut state, me, card, bears);
        assert_eq!(stats(&state, bears), size, "{card}");
    }
}

#[test]
fn the_auras_upgraded_keep_what_they_did() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let life = state.player(me).life;
    play_at(&mut state, me, "cartouche_of_solidarity+", bears);
    assert!(has(&state, bears, FirstStrike));
    assert_eq!(named(&state, me, "Warrior").len(), 1);
    play_at(&mut state, me, "divine_favor+", bears);
    assert_eq!(state.player(me).life, life + 3);
    play_at(&mut state, me, "fists_of_ironwood+", bears);
    assert!(has(&state, bears, Trample));
    assert_eq!(named(&state, me, "Saproling").len(), 2);
    play_at(&mut state, me, "setessan_training+", bears);
    assert_eq!(state.player(me).hand.len(), 1);
}

#[test]
fn ethereal_armor_upgraded_gives_vigilance_too() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    play_at(&mut state, me, "ethereal_armor+", bears);
    assert!(has(&state, bears, FirstStrike));
    assert!(has(&state, bears, Vigilance));
}

#[test]
fn oakheart_dryads_upgraded_pump_by_two() {
    // Alone, their ability can only be aimed at themselves.
    let (mut state, me, _) = board();
    let dryads = played(&mut state, me, "oakheart_dryads+");
    assert_eq!(stats(&state, dryads), (4, 5));
    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(stats(&state, dryads), (2, 3));
}

#[test]
fn daybreak_coronet_upgraded_gives_four() {
    let (mut state, me, _) = board();
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let coronet = put_in_hand(&mut state, me, "daybreak_coronet+");
    // Still only for a creature that wears another Aura.
    assert!(apply_action(&mut state, me, cast_at(coronet, at(bears))).is_err());
    play_at(&mut state, me, "rancor", bears);
    act(&mut state, me, cast_at(coronet, at(bears)));
    settle(&mut state);
    assert_eq!(stats(&state, bears), (8, 6));
    assert!(has(&state, bears, Lifelink));
}
