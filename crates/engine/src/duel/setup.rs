//! Starting a duel: building the state and dealing opening hands.

use super::action::GameEvent;
use super::config::DuelConfig;
use super::error::{ConfigError, InternalError};
use super::ids::{ObjectId, PlayerId};
use super::rng::DuelRng;
use super::state::{Decision, DuelState, PlayerState, Zone};
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
        for instance in &player_config.library {
            if cards.get(&instance.card_id).is_none() {
                return Err(ConfigError::UnknownCard {
                    player,
                    card: instance.card_id.clone(),
                });
            }
        }
    }

    let mut rng = DuelRng::from_seed(seed);
    let starting_player = match rng.below(2) {
        0 => PlayerId::P1,
        _ => PlayerId::P2,
    };
    let players = config
        .players
        .each_ref()
        .map(|player| PlayerState::new(player.starting_life, player.opening_hand_size));
    let mut state = DuelState::new(config.rules, players, starting_player, rng);
    for (player, player_config) in PlayerId::ALL.into_iter().zip(config.players) {
        for instance in player_config.library {
            state.create_card_in_library(player, instance.card_id);
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

/// Deals a smoothed opening hand to a player whose cards are all in their
/// library.
///
/// The library is shuffled `smoothing_candidates` times. Each shuffle's top
/// cards are a candidate hand, and the one whose land count is closest to
/// the deck's land ratio is kept (the first, on a tie). The rest of the
/// library is then shuffled again.
pub(crate) fn deal_opening_hand(
    state: &mut DuelState,
    player: PlayerId,
) -> Result<Vec<GameEvent>, InternalError> {
    let mut library = std::mem::take(&mut state.player_mut(player).library);
    let hand_size = usize::from(state.player(player).opening_hand_size).min(library.len());
    let deck_lands = count_lands(state, &library)?;

    let mut orders = Vec::new();
    let mut hand_lands = Vec::new();
    for _ in 0..state.rules.smoothing_candidates.max(1) {
        state.rng.shuffle(&mut library);
        let hand = &library[library.len() - hand_size..];
        hand_lands.push(count_lands(state, hand)?);
        orders.push(library.clone());
    }
    let best = closest_to_land_ratio(&hand_lands, hand_size, deck_lands, library.len());
    state.player_mut(player).library = orders.swap_remove(best);

    let mut events = Vec::new();
    for _ in 0..hand_size {
        events.extend(take_top_card(state, player)?);
    }

    let mut rest = std::mem::take(&mut state.player_mut(player).library);
    state.rng.shuffle(&mut rest);
    state.player_mut(player).library = rest;
    Ok(events)
}

/// Which candidate hand has a land count closest to the deck's land ratio
/// times the hand size. Earlier candidates win ties.
fn closest_to_land_ratio(
    hand_lands: &[usize],
    hand_size: usize,
    deck_lands: usize,
    deck_size: usize,
) -> usize {
    // |lands - hand_size * deck_lands / deck_size|, scaled by deck_size to
    // stay in whole numbers.
    let distance = |lands: usize| (lands * deck_size).abs_diff(hand_size * deck_lands);
    hand_lands
        .iter()
        .enumerate()
        .min_by_key(|&(_, &lands)| distance(lands))
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
    use std::collections::BTreeSet;

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

    #[test]
    fn closest_candidate_wins_and_ties_go_to_the_first() {
        // 10 lands in 25 cards: a 7-card hand wants 2.8 lands.
        let pick = |hand_lands: &[usize]| closest_to_land_ratio(hand_lands, 7, 10, 25);
        assert_eq!(pick(&[0, 3, 5]), 1);
        assert_eq!(pick(&[6, 5, 4]), 2);
        assert_eq!(pick(&[2, 3]), 1);
        assert_eq!(pick(&[3, 3, 3]), 0);
        // 1 and 5 lands are 1.8 and 2.2 away.
        assert_eq!(pick(&[5, 1]), 1);
        // With an even split, 3 and 4 lands are equally close to 3.5.
        assert_eq!(closest_to_land_ratio(&[4, 3], 7, 10, 20), 0);
        assert_eq!(closest_to_land_ratio(&[3, 4], 7, 10, 20), 0);
        assert_eq!(pick(&[4]), 0);
    }

    /// How far opening hands are from 2.8 lands, summed over many seeds.
    fn total_distance_from_ratio(candidates: u8) -> usize {
        (0..200)
            .map(|seed| {
                let mut config = config("deck_a", "deck_c");
                config.rules.smoothing_candidates = candidates;
                let state = new_duel(config, seed).unwrap();
                PlayerId::ALL
                    .into_iter()
                    .map(|player| (hand_lands(&state, player) * 25).abs_diff(70))
                    .sum::<usize>()
            })
            .sum()
    }

    #[test]
    fn smoothing_brings_hands_closer_to_the_land_ratio() {
        let off = total_distance_from_ratio(1);
        let on = total_distance_from_ratio(3);
        let heavy = total_distance_from_ratio(8);
        assert!(on * 10 < off * 7, "3 candidates: {on}, 1 candidate: {off}");
        assert!(heavy < on, "8 candidates: {heavy}, 3 candidates: {on}");
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
