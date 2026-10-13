//! Starting a duel: building the state and dealing opening hands.

use std::collections::BTreeSet;

use super::action::GameEvent;
use super::config::{CardInstance, DuelConfig};
use super::error::{ConfigError, InternalError};
use super::ids::{ObjectId, PlayerId};
use super::rng::DuelRng;
use super::state::{Decision, DuelState, PlayerState, Zone};
use crate::cards::{CardDef, Color, validate_emblem};
use crate::data;

/// Starts a duel. Both players have their opening hands and the game is
/// waiting on the starting player's mulligan decision.
pub fn new_duel(config: DuelConfig, seed: u64) -> Result<DuelState, ConfigError> {
    let cards = &data::embedded()?.cards;
    if config.rules.smoothing_candidates == 0 {
        return Err(ConfigError::SmoothingCandidates);
    }
    for player in PlayerId::ALL {
        let player_config = &config.players[player.index()];
        if player_config.starting_life <= 0 {
            return Err(ConfigError::StartingLife(player));
        }
        for emblem in &player_config.emblems {
            validate_emblem(emblem).map_err(|source| ConfigError::Emblem {
                player,
                name: emblem.name.clone(),
                source,
            })?;
        }
        if let Some(commander) = &player_config.commander {
            let fits = cards
                .get(&commander.card_id)
                .is_some_and(|def| def.is_legendary() && def.is_creature());
            if !fits {
                return Err(ConfigError::Commander {
                    player,
                    card: commander.card_id.clone(),
                });
            }
        }
        let cards_named = player_config
            .library
            .iter()
            .chain(&player_config.starting_permanents);
        // An upgraded copy of a card with no upgraded side is unknown too.
        for instance in cards_named {
            if cards.played_as(instance).is_none() {
                return Err(ConfigError::UnknownCard {
                    player,
                    card: instance.card_id.clone(),
                });
            }
        }
    }
    // What a card in a deck is played as: checked to be there just above.
    let played_as = |instance: CardInstance| match cards.played_as(&instance) {
        Some(def) => def.id.clone(),
        None => instance.card_id,
    };

    let mut rng = DuelRng::from_seed(seed);
    let starting_player = match config.starting_player {
        Some(player) => player,
        None => match rng.below(2) {
            0 => PlayerId::P1,
            _ => PlayerId::P2,
        },
    };
    let players = config
        .players
        .each_ref()
        .map(|player| PlayerState::new(player.starting_life, player.opening_hand_size));
    let mut state = DuelState::new(config.rules, players, starting_player, rng);
    // Libraries first, so the cards' ids don't depend on the extras.
    let mut extras = Vec::new();
    for (player, player_config) in PlayerId::ALL.into_iter().zip(config.players) {
        for instance in player_config.library {
            state.create_card(player, played_as(instance), Zone::Library);
        }
        extras.push((
            player,
            player_config.starting_permanents,
            player_config.emblems,
            player_config.commander,
        ));
    }
    for (player, permanents, emblems, commander) in extras {
        if let Some(instance) = commander {
            let id = state.create_card(player, instance.card_id, Zone::Command);
            let card = state.object(id)?.card;
            state.player_mut(player).commander = Some(card);
        }
        for instance in permanents {
            // There from the start, so not summoning sick, and nothing
            // triggers on it entering.
            let id = state.create_card(player, played_as(instance), Zone::Battlefield);
            state.object_mut(id)?.summoning_sick = false;
        }
        for emblem in emblems {
            state.add_emblem(player, emblem);
        }
    }
    for player in PlayerId::ALL {
        deal_opening_hand(&mut state, player)?;
    }
    state.decision = Some(Decision::Mulligan {
        player: starting_player,
    });
    Ok(state)
}

/// Deals a smoothed hand to a player whose cards are all in their library:
/// an opening hand, or the fresh hand of a mulligan.
///
/// The library is shuffled, and each shuffle's top cards are a candidate
/// hand. Dealing stops at the first hand that is right (`HandMiss`), or
/// after `smoothing_candidates` hands, when the one that misses least is
/// kept. The rest of the library is then shuffled again.
pub(crate) fn deal_opening_hand(
    state: &mut DuelState,
    player: PlayerId,
) -> Result<Vec<GameEvent>, InternalError> {
    let library = std::mem::take(&mut state.player_mut(player).library);
    let hand_size = usize::from(state.player(player).opening_hand_size).min(library.len());
    let smoothed = smoothed_library(state, library, hand_size)?;
    state.player_mut(player).library = smoothed.library;

    let mut events = Vec::new();
    for _ in 0..hand_size {
        events.extend(take_top_card(state, player)?);
    }

    let mut rest = std::mem::take(&mut state.player_mut(player).library);
    state.rng.shuffle(&mut rest);
    state.player_mut(player).library = rest;
    Ok(events)
}

/// How far a candidate hand is from right. A hand is right when its land
/// count fits the deck's land ratio and every colour among its spells has a
/// land in the hand that makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandMiss {
    /// Lands too few or too many. The deck's land ratio times the hand
    /// size, rounded down or up, both fit.
    lands: usize,
    /// Colours in the mana costs of the hand's spells that no land in the
    /// hand makes. A land that makes two colours is a source of each.
    colors: usize,
    /// How far the land count is from the exact ratio, in lands times the
    /// deck's size to stay in whole numbers. It only breaks ties.
    ratio_distance: usize,
}

impl HandMiss {
    pub(crate) fn is_right(self) -> bool {
        self.points() == 0
    }

    /// One for each land too few or too many, and one for each colour
    /// without a source.
    fn points(self) -> usize {
        self.lands + self.colors
    }

    fn of(hand: &[&CardDef], deck_lands: usize, deck_size: usize) -> HandMiss {
        let lands = hand.iter().filter(|card| card.is_land()).count();
        let wanted = hand.len() * deck_lands;
        let (fewest, most) = match deck_size {
            0 => (0, 0),
            _ => (wanted / deck_size, wanted.div_ceil(deck_size)),
        };
        let sources: BTreeSet<Color> = hand
            .iter()
            .filter(|card| card.is_land())
            .flat_map(|land| land.mana_colors())
            .collect();
        let needed: BTreeSet<Color> = hand
            .iter()
            .filter(|card| !card.is_land())
            .flat_map(|spell| spell.mana_cost.colored())
            .collect();
        HandMiss {
            lands: fewest.saturating_sub(lands) + lands.saturating_sub(most),
            colors: needed.difference(&sources).count(),
            ratio_distance: (lands * deck_size).abs_diff(wanted),
        }
    }
}

/// A library put in the order that deals the kept hand from its top, and
/// what each hand dealt on the way missed (which the tests read).
#[cfg_attr(not(test), allow(dead_code))]
struct Smoothed {
    library: Vec<ObjectId>,
    /// One for each candidate hand, in the order they were dealt.
    misses: Vec<HandMiss>,
    /// Which of them was kept.
    kept: usize,
}

fn smoothed_library(
    state: &mut DuelState,
    mut library: Vec<ObjectId>,
    hand_size: usize,
) -> Result<Smoothed, InternalError> {
    let deck_size = library.len();
    let deck_lands = count_lands(state, &library)?;

    let mut orders = Vec::new();
    let mut misses = Vec::new();
    for _ in 0..state.rules.smoothing_candidates.max(1) {
        state.rng.shuffle(&mut library);
        let mut hand = Vec::with_capacity(hand_size);
        for id in &library[deck_size - hand_size..] {
            hand.push(state.def(*id)?);
        }
        let miss = HandMiss::of(&hand, deck_lands, deck_size);
        misses.push(miss);
        orders.push(library.clone());
        if miss.is_right() {
            break;
        }
    }
    let kept = least_miss(&misses);
    Ok(Smoothed {
        library: orders.swap_remove(kept),
        misses,
        kept,
    })
}

/// Which candidate hand misses least: the fewest points, then the land
/// count closest to the exact ratio, then the first dealt.
fn least_miss(misses: &[HandMiss]) -> usize {
    misses
        .iter()
        .enumerate()
        .min_by_key(|&(_, miss)| (miss.points(), miss.ratio_distance))
        .map_or(0, |(index, _)| index)
}

fn count_lands(state: &DuelState, objects: &[ObjectId]) -> Result<usize, InternalError> {
    let mut lands = 0;
    for id in objects {
        if state.def(*id)?.is_land() {
            lands += 1;
        }
    }
    Ok(lands)
}

/// What the hand a player holds misses, measured against their whole deck
/// (hand and library).
#[cfg(test)]
pub(crate) fn miss_of_hand(state: &DuelState, player: PlayerId) -> HandMiss {
    let player_state = state.player(player);
    let def = |id: &ObjectId| state.def(*id).unwrap();
    let hand: Vec<&CardDef> = player_state.hand.iter().map(def).collect();
    let deck = player_state.hand.iter().chain(&player_state.library);
    let deck_lands = deck.clone().filter(|id| def(id).is_land()).count();
    HandMiss::of(&hand, deck_lands, deck.count())
}

/// Moves the top card of the player's library to their hand. Does nothing
/// if the library is empty; fatigue is the caller's business.
pub(crate) fn take_top_card(
    state: &mut DuelState,
    player: PlayerId,
) -> Result<Option<GameEvent>, InternalError> {
    let Some(&top) = state.player(player).library.last() else {
        return Ok(None);
    };
    let drawn = state.move_object(top, Zone::Hand)?;
    Ok(Some(GameEvent::CardDrawn {
        player,
        card: state.card_ref(drawn)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardDefId;
    use crate::duel::config::CardInstance;
    use crate::duel::test_support::{config, hand_lands};

    #[test]
    fn both_players_start_with_a_hand_a_library_and_their_life() {
        let state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
        state.assert_consistent();
        for player in PlayerId::ALL {
            let player_state = state.player(player);
            assert_eq!(player_state.hand.len(), 7);
            assert_eq!(player_state.library.len(), 18);
            assert_eq!(player_state.life, 20);
            assert_eq!(player_state.mulligans_taken, 0);
        }
        assert_eq!(state.objects.len(), 50);
        assert!(state.objects.values().all(|object| object.card.0 < 50));
    }

    #[test]
    fn each_player_gets_their_own_deck() {
        let state = new_duel(config("deck_a", "deck_c"), 1).unwrap();
        let defs_of = |player: PlayerId| -> BTreeSet<&str> {
            state
                .objects
                .values()
                .filter(|object| object.owner == player)
                .map(|object| object.def.as_str())
                .collect()
        };
        assert!(defs_of(PlayerId::P1).contains("serra_angel"));
        assert!(!defs_of(PlayerId::P1).contains("mountain"));
        assert_eq!(
            defs_of(PlayerId::P2),
            BTreeSet::from([
                "boggart_brute",
                "hill_giant",
                "lightning_strike",
                "mountain",
                "raging_goblin",
                "shock"
            ])
        );
    }

    #[test]
    fn the_game_waits_on_the_starting_player_to_mulligan_or_keep() {
        let state = new_duel(config("deck_a", "deck_b"), 5).unwrap();
        assert_eq!(
            state.decision,
            Some(Decision::Mulligan {
                player: state.starting_player
            })
        );
        assert_eq!(state.waiting_on(), Some(state.starting_player));
    }

    #[test]
    fn same_seed_gives_the_same_duel() {
        let a = new_duel(config("deck_a", "deck_b"), 99).unwrap();
        let b = new_duel(config("deck_a", "deck_b"), 99).unwrap();
        let c = new_duel(config("deck_a", "deck_b"), 100).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn starting_player_is_a_seeded_coin_flip() {
        let starters: Vec<PlayerId> = (0..40)
            .map(|seed| {
                new_duel(config("deck_a", "deck_b"), seed)
                    .unwrap()
                    .starting_player
            })
            .collect();
        let p1_starts = starters.iter().filter(|p| **p == PlayerId::P1).count();
        assert!((10..=30).contains(&p1_starts), "P1 started {p1_starts}/40");
    }

    #[test]
    fn a_config_can_name_the_starting_player() {
        for first in PlayerId::ALL {
            for seed in 0..20 {
                let mut named = config("deck_a", "deck_b");
                named.starting_player = Some(first);
                let state = new_duel(named, seed).unwrap();
                assert_eq!(state.starting_player, first);
                assert_eq!(state.decision, Some(Decision::Mulligan { player: first }));
            }
        }
    }

    #[test]
    fn state_survives_a_json_round_trip() {
        let state = new_duel(config("deck_b", "deck_c"), 3).unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let restored: DuelState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, state);
    }

    #[test]
    fn rejects_bad_configs() {
        let mut unknown = config("deck_a", "deck_b");
        unknown.players[1]
            .library
            .push(CardInstance::new(CardDefId("black_lotus".to_string())));
        assert_eq!(
            new_duel(unknown, 1),
            Err(ConfigError::UnknownCard {
                player: PlayerId::P2,
                card: CardDefId("black_lotus".to_string()),
            })
        );

        let mut dead = config("deck_a", "deck_b");
        dead.players[0].starting_life = 0;
        assert_eq!(
            new_duel(dead, 1),
            Err(ConfigError::StartingLife(PlayerId::P1))
        );

        let mut no_hands = config("deck_a", "deck_b");
        no_hands.rules.smoothing_candidates = 0;
        assert_eq!(new_duel(no_hands, 1), Err(ConfigError::SmoothingCandidates));
    }

    #[test]
    fn a_library_smaller_than_the_hand_is_dealt_whole() {
        let mut small = config("deck_a", "deck_b");
        small.players[0].library.truncate(4);
        let state = new_duel(small, 1).unwrap();
        assert_eq!(state.player(PlayerId::P1).hand.len(), 4);
        assert!(state.player(PlayerId::P1).library.is_empty());
        assert_eq!(state.player(PlayerId::P2).hand.len(), 7);
    }

    #[test]
    fn per_player_hand_size_and_life_are_respected() {
        let mut uneven = config("deck_a", "deck_b");
        uneven.players[0].opening_hand_size = 8;
        uneven.players[1].starting_life = 15;
        let state = new_duel(uneven, 1).unwrap();
        assert_eq!(state.player(PlayerId::P1).hand.len(), 8);
        assert_eq!(state.player(PlayerId::P2).hand.len(), 7);
        assert_eq!(state.player(PlayerId::P2).life, 15);
    }

    fn cards(ids: &[&str]) -> Vec<&'static CardDef> {
        let cards = &data::embedded().unwrap().cards;
        ids.iter()
            .map(|id| cards.get(&CardDefId(id.to_string())).unwrap())
            .collect()
    }

    /// What a hand misses in a deck of 10 lands in 25 cards, where a
    /// seven-card hand wants 2.8 lands: 2 and 3 both fit.
    fn miss(hand: &[&str]) -> HandMiss {
        HandMiss::of(&cards(hand), 10, 25)
    }

    #[test]
    fn a_hand_is_right_when_its_lands_fit_and_its_colours_have_sources() {
        let right = miss(&[
            "island",
            "swamp",
            "typhoid_rats",
            "wind_drake",
            "murder",
            "cancel",
            "divination",
        ]);
        assert_eq!((right.lands, right.colors), (0, 0));
        assert!(right.is_right());

        // Three lands fit as well as two.
        assert!(
            miss(&[
                "island", "island", "swamp", "murder", "cancel", "cancel", "cancel"
            ])
            .is_right()
        );
        // A hand with spells of one colour needs only that colour.
        assert!(
            miss(&[
                "swamp",
                "swamp",
                "murder",
                "murder",
                "murder",
                "typhoid_rats",
                "typhoid_rats"
            ])
            .is_right()
        );
    }

    #[test]
    fn each_land_too_few_or_too_many_is_a_point() {
        let lands = |count: usize| {
            let mut hand = vec!["swamp"; count];
            hand.resize(7, "murder");
            miss(&hand)
        };
        let points: Vec<usize> = (1..=7).map(|count| lands(count).lands).collect();
        assert_eq!(points, [1, 0, 0, 1, 2, 3, 4]);
        // No lands: two too few, and black has no source.
        assert_eq!((lands(0).lands, lands(0).colors), (2, 1));
        assert!(!lands(1).is_right());
        assert!(!lands(4).is_right());

        // A ratio that comes out whole has one count that fits: 10 lands in
        // 20 cards and a six-card hand want exactly 3.
        let exact = |count: usize| {
            let mut hand = vec!["swamp"; count];
            hand.resize(6, "murder");
            HandMiss::of(&cards(&hand), 10, 20).lands
        };
        assert_eq!([exact(2), exact(3), exact(4)], [1, 0, 1]);
    }

    #[test]
    fn each_colour_among_the_spells_without_a_land_that_makes_it_is_a_point() {
        // The lands fit, but nothing makes blue for the Wind Drake.
        let no_blue = miss(&[
            "swamp",
            "swamp",
            "swamp",
            "wind_drake",
            "murder",
            "murder",
            "murder",
        ]);
        assert_eq!((no_blue.lands, no_blue.colors), (0, 1));
        assert!(!no_blue.is_right());

        // A two-coloured spell needs both of its colours.
        let gold = |lands: [&'static str; 2]| {
            let mut hand = lands.to_vec();
            hand.resize(7, "call_of_the_conclave");
            miss(&hand).colors
        };
        assert_eq!(gold(["forest", "plains"]), 0);
        assert_eq!(gold(["forest", "forest"]), 1);
        assert_eq!(gold(["island", "swamp"]), 2);

        // A land's colour is not a need: lands of a colour no spell uses
        // cost nothing.
        assert_eq!(miss(&["island", "swamp", "murder"]).colors, 0);
    }

    #[test]
    fn a_dual_land_is_a_source_of_each_of_its_colours() {
        let with_lands = |lands: [&'static str; 2]| {
            let mut hand = lands.to_vec();
            hand.extend(["serra_angel", "serra_angel", "grizzly_bears"]);
            hand.extend(["grizzly_bears", "call_of_the_conclave"]);
            miss(&hand)
        };
        // One Guildgate covers the white and the green spells alike.
        assert!(with_lands(["selesnya_guildgate", "mountain"]).is_right());
        assert!(with_lands(["selesnya_guildgate", "selesnya_guildgate"]).is_right());
        // It is one source of each, as a Plains and a Forest together are.
        assert!(with_lands(["plains", "forest"]).is_right());
        assert_eq!(with_lands(["plains", "mountain"]).colors, 1);
        // A Guildgate of other colours covers only what it makes.
        assert_eq!(with_lands(["boros_guildgate", "mountain"]).colors, 1);
    }

    #[test]
    fn the_hand_that_misses_least_is_kept_and_ties_go_to_the_first() {
        let miss = |lands, colors, ratio_distance| HandMiss {
            lands,
            colors,
            ratio_distance,
        };
        // The fewest points, whether they are lands or colours.
        assert_eq!(
            least_miss(&[miss(2, 0, 55), miss(0, 1, 5), miss(1, 1, 30)]),
            1
        );
        assert_eq!(
            least_miss(&[miss(1, 1, 30), miss(2, 1, 55), miss(1, 0, 30)]),
            2
        );
        // A land and a colour weigh the same; the closer land count then
        // decides.
        assert_eq!(least_miss(&[miss(1, 0, 30), miss(0, 1, 20)]), 1);
        assert_eq!(least_miss(&[miss(0, 1, 5), miss(1, 0, 30)]), 0);
        // Level on both: the first dealt.
        assert_eq!(least_miss(&[miss(1, 1, 30), miss(1, 1, 30)]), 0);
        // A right hand beats everything.
        assert_eq!(least_miss(&[miss(0, 1, 5), miss(0, 0, 20)]), 1);
        assert_eq!(least_miss(&[miss(1, 0, 30)]), 0);
    }

    /// A duel whose first player's hand is back in their library, and that
    /// library.
    fn undealt(config: DuelConfig, seed: u64) -> (DuelState, Vec<ObjectId>) {
        let mut state = new_duel(config, seed).unwrap();
        for card in state.player(PlayerId::P1).hand.clone() {
            state.move_object(card, Zone::Library).unwrap();
        }
        let library = state.player(PlayerId::P1).library.clone();
        (state, library)
    }

    /// What the top `hand_size` cards of a library miss.
    fn miss_of_top(state: &DuelState, library: &[ObjectId], hand_size: usize) -> HandMiss {
        let def = |id: &ObjectId| state.def(*id).unwrap();
        let hand: Vec<&CardDef> = library[library.len() - hand_size..]
            .iter()
            .map(def)
            .collect();
        let deck_lands = library.iter().filter(|id| def(id).is_land()).count();
        HandMiss::of(&hand, deck_lands, library.len())
    }

    /// White spells and only Mountains: no hand of this deck is ever right.
    fn hopeless() -> DuelConfig {
        let mut config = config("deck_a", "deck_b");
        let card = |id: &str| CardInstance::new(CardDefId(id.to_string()));
        let mut library = vec![card("mountain"); 10];
        library.resize(25, card("serra_angel"));
        config.players[0].library = library;
        config
    }

    #[test]
    fn dealing_stops_at_the_cap_and_keeps_the_hand_that_misses_least() {
        let cap = usize::from(data::embedded().unwrap().balance.duel.smoothing_candidates);
        assert_eq!(cap, 5);
        let mut kept_later = 0;
        for seed in 0..40 {
            let (mut state, library) = undealt(hopeless(), seed);
            let smoothed = smoothed_library(&mut state, library, 7).unwrap();
            assert_eq!(smoothed.misses.len(), cap, "seed {seed}");
            assert!(smoothed.misses.iter().all(|miss| !miss.is_right()));
            assert_eq!(smoothed.kept, least_miss(&smoothed.misses));
            // The library comes back in the kept hand's order.
            assert_eq!(
                miss_of_top(&state, &smoothed.library, 7),
                smoothed.misses[smoothed.kept],
                "seed {seed}"
            );
            let fewest = smoothed.misses.iter().map(|miss| miss.points()).min();
            assert_eq!(Some(smoothed.misses[smoothed.kept].points()), fewest);
            kept_later += usize::from(smoothed.kept > 0);
        }
        // It is the best of the five, not the first or the last.
        assert!(
            (5..35).contains(&kept_later),
            "kept a later hand {kept_later}/40"
        );

        // The cap is the config's number.
        let mut three = hopeless();
        three.rules.smoothing_candidates = 3;
        let (mut state, library) = undealt(three, 1);
        assert_eq!(
            smoothed_library(&mut state, library, 7)
                .unwrap()
                .misses
                .len(),
            3
        );
    }

    #[test]
    fn dealing_stops_at_the_first_right_hand() {
        let mut dealt = BTreeSet::new();
        for seed in 0..200 {
            let (mut state, library) = undealt(config("deck_b", "deck_a"), seed);
            let smoothed = smoothed_library(&mut state, library, 7).unwrap();
            let (last, earlier) = smoothed.misses.split_last().unwrap();
            assert!(earlier.iter().all(|miss| !miss.is_right()), "seed {seed}");
            assert!(smoothed.misses.len() <= 5);
            if smoothed.misses.len() < 5 {
                assert!(last.is_right(), "seed {seed}");
            }
            if last.is_right() {
                assert_eq!(smoothed.kept, smoothed.misses.len() - 1, "seed {seed}");
            }
            assert_eq!(
                miss_of_top(&state, &smoothed.library, 7),
                smoothed.misses[smoothed.kept]
            );
            dealt.insert(smoothed.misses.len());
        }
        assert_eq!(dealt, BTreeSet::from([1, 2, 3, 4, 5]));
    }

    /// How many of 400 opening hands of the blue-black deck are right, and
    /// how far they are from 2.8 lands in all.
    fn right_hands(candidates: u8) -> (usize, usize) {
        let mut right = 0;
        let mut distance = 0;
        for seed in 0..200 {
            let mut config = config("deck_b", "deck_b");
            config.rules.smoothing_candidates = candidates;
            let state = new_duel(config, seed).unwrap();
            for player in PlayerId::ALL {
                right += usize::from(miss_of_hand(&state, player).is_right());
                distance += (hand_lands(&state, player) * 25).abs_diff(70);
            }
        }
        (right, distance)
    }

    #[test]
    fn smoothing_gives_both_players_right_hands_far_more_often() {
        let (off, off_distance) = right_hands(1);
        let (on, on_distance) = right_hands(5);
        assert!(off < 240, "1 hand: {off}/400 right");
        assert!(on > 340, "5 hands: {on}/400 right");
        assert!(
            on_distance * 10 < off_distance * 7,
            "5 hands: {on_distance}, 1 hand: {off_distance}"
        );
    }

    #[test]
    fn unsmoothed_hands_still_vary() {
        let lands: BTreeSet<usize> = (0..200)
            .map(|seed| {
                let mut config = config("deck_a", "deck_c");
                config.rules.smoothing_candidates = 1;
                hand_lands(&new_duel(config, seed).unwrap(), PlayerId::P1)
            })
            .collect();
        assert!(lands.len() >= 5, "land counts seen: {lands:?}");
    }
}
