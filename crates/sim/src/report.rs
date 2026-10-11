//! Adding game records up per deck pairing and printing the report.

use std::fmt::Write;

use engine::balance::SimBalance;
use engine::duel::PlayerId;

use crate::game::{GameRecord, Outcome};

/// Everything counted for one pairing of decks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PairingStats {
    pub first_deck: String,
    pub second_deck: String,
    /// Games that ran without an error.
    pub games: u32,
    pub first_deck_wins: u32,
    pub second_deck_wins: u32,
    /// Wins by the player in seat P1, whichever deck they had.
    pub p1_wins: u32,
    /// Wins by the player who took the first turn.
    pub play_wins: u32,
    pub fatigue_wins: u32,
    pub draws: u32,
    pub turn_caps: u32,
    pub turns: u64,
    /// Two per game: each player of each game.
    pub players: u32,
    pub players_who_mulliganed: u32,
    pub stuck_measured: u32,
    pub stuck: u32,
    pub flood_measured: u32,
    pub flooded: u32,
    pub turns_finished: u64,
    pub untapped_lands: u64,
    /// `(seed, what went wrong)`.
    pub errors: Vec<(u64, String)>,
}

impl PairingStats {
    pub fn new(first_deck: &str, second_deck: &str) -> PairingStats {
        PairingStats {
            first_deck: first_deck.to_string(),
            second_deck: second_deck.to_string(),
            ..PairingStats::default()
        }
    }

    /// Counts one finished game. `first_deck_seat` is the seat the
    /// first-named deck sat in.
    pub fn add(&mut self, record: &GameRecord, first_deck_seat: PlayerId) {
        self.games += 1;
        self.turns += u64::from(record.turns);
        match record.outcome {
            Outcome::Win { seat, by_fatigue } => {
                if seat == first_deck_seat {
                    self.first_deck_wins += 1;
                } else {
                    self.second_deck_wins += 1;
                }
                self.p1_wins += u32::from(seat == PlayerId::P1);
                self.play_wins += u32::from(seat == record.starting_player);
                self.fatigue_wins += u32::from(by_fatigue);
            }
            Outcome::Draw => self.draws += 1,
            Outcome::TurnCap => self.turn_caps += 1,
        }
        for player in &record.players {
            self.players += 1;
            self.players_who_mulliganed += u32::from(player.mulligans > 0);
            if let Some(stuck) = player.stuck_on_lands {
                self.stuck_measured += 1;
                self.stuck += u32::from(stuck);
            }
            if let Some(flooded) = player.flooded {
                self.flood_measured += 1;
                self.flooded += u32::from(flooded);
            }
            self.turns_finished += u64::from(player.turns_finished);
            self.untapped_lands += u64::from(player.untapped_lands);
        }
    }

    pub fn add_error(&mut self, seed: u64, message: String) {
        self.errors.push((seed, message));
    }

    fn wins(&self) -> u32 {
        self.first_deck_wins + self.second_deck_wins
    }
}

/// `part` as a percentage of `whole` with one decimal, or `-` of nothing.
fn percent(part: u32, whole: u32) -> String {
    ratio(u64::from(part) * 100, u64::from(whole))
}

/// `total / count` with one decimal, or `-` if there is nothing to average.
fn ratio(total: u64, count: u64) -> String {
    if count == 0 {
        return "-".to_string();
    }
    // Tenths, rounded to nearest, in whole numbers.
    let tenths = (total * 10 + count / 2) / count;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// The whole report as text.
pub fn format_report(
    stats: &[PairingStats],
    games_per_pairing: u32,
    first_seed: u64,
    limits: &SimBalance,
) -> String {
    let mut out = String::new();
    let info = engine::engine_info();
    let last_seed = first_seed + u64::from(games_per_pairing.saturating_sub(1));
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "{} {}: {games_per_pairing} games per pairing, seeds {first_seed} to {last_seed}, \
         turn cap {}",
        info.name, info.version, limits.turn_cap
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<17} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8} {:>5} {:>6} {:>7}",
        "Pairing",
        "Games",
        "Win%",
        "Play%",
        "P1%",
        "Turns",
        "Mull%",
        "Stuck%",
        "Flood%",
        "Idle",
        "Fatigue",
        "Cap",
        "Draws",
        "Errors"
    );
    for pairing in stats {
        let _ = writeln!(
            out,
            "{:<17} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8} {:>5} {:>6} {:>7}",
            format!("{} v {}", pairing.first_deck, pairing.second_deck),
            pairing.games,
            percent(pairing.first_deck_wins, pairing.wins()),
            percent(pairing.play_wins, pairing.wins()),
            percent(pairing.p1_wins, pairing.wins()),
            ratio(pairing.turns, u64::from(pairing.games)),
            percent(pairing.players_who_mulliganed, pairing.players),
            percent(pairing.stuck, pairing.stuck_measured),
            percent(pairing.flooded, pairing.flood_measured),
            ratio(pairing.untapped_lands, pairing.turns_finished),
            pairing.fatigue_wins,
            pairing.turn_caps,
            pairing.draws,
            pairing.errors.len(),
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Win%     games won by the first-named deck, of games with a winner"
    );
    let _ = writeln!(
        out,
        "Play%    games won by the player who took the first turn"
    );
    let _ = writeln!(
        out,
        "P1%      games won by seat P1 (the decks swap seats every game)"
    );
    let _ = writeln!(
        out,
        "Turns    average game length in turns, counting both players' turns"
    );
    let _ = writeln!(out, "Mull%    players who took at least one mulligan");
    let _ = writeln!(
        out,
        "Stuck%   players with {} or fewer lands on the battlefield at the end of their turn {}",
        limits.land_screw.max_lands, limits.land_screw.turn
    );
    let _ = writeln!(
        out,
        "Flood%   players whose drawn cards were at least {}% lands at the end of their turn {}",
        limits.land_flood.min_land_percent, limits.land_flood.turn
    );
    let _ = writeln!(
        out,
        "Idle     average untapped lands at the end of a player's own turn"
    );
    let _ = writeln!(
        out,
        "Fatigue  games won because the loser drew from an empty library"
    );
    let _ = writeln!(
        out,
        "Cap      games stopped at the turn cap; Draws: both players lost at once"
    );

    let errors: u32 = stats
        .iter()
        .map(|pairing| pairing.errors.len() as u32)
        .sum();
    let _ = writeln!(out);
    if errors == 0 {
        let _ = writeln!(out, "No errors.");
    } else {
        let _ = writeln!(out, "{errors} errors. The first few:");
        for pairing in stats {
            for (seed, message) in pairing.errors.iter().take(5) {
                let _ = writeln!(
                    out,
                    "  {} v {}, seed {seed}: {message}",
                    pairing.first_deck, pairing.second_deck
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PlayerRecord;

    fn record(outcome: Outcome, starting_player: PlayerId, turns: u32) -> GameRecord {
        GameRecord {
            seed: 1,
            starting_player,
            turns,
            outcome,
            players: [PlayerRecord::default(), PlayerRecord::default()],
        }
    }

    fn win(seat: PlayerId) -> Outcome {
        Outcome::Win {
            seat,
            by_fatigue: false,
        }
    }

    #[test]
    fn wins_are_credited_to_the_deck_in_the_winning_seat() {
        let mut stats = PairingStats::new("deck_a", "deck_b");
        // deck_a in seat P1 wins; then deck_a in seat P2 loses to P1.
        stats.add(&record(win(PlayerId::P1), PlayerId::P1, 10), PlayerId::P1);
        stats.add(&record(win(PlayerId::P1), PlayerId::P2, 12), PlayerId::P2);
        stats.add(&record(win(PlayerId::P2), PlayerId::P2, 14), PlayerId::P2);
        assert_eq!((stats.first_deck_wins, stats.second_deck_wins), (2, 1));
        assert_eq!(stats.p1_wins, 2);
        assert_eq!(stats.play_wins, 2);
        assert_eq!((stats.games, stats.turns), (3, 36));
    }

    #[test]
    fn draws_caps_and_fatigue_are_counted_apart_from_wins() {
        let mut stats = PairingStats::new("deck_a", "deck_a");
        stats.add(&record(Outcome::Draw, PlayerId::P1, 9), PlayerId::P1);
        stats.add(&record(Outcome::TurnCap, PlayerId::P1, 100), PlayerId::P1);
        let fatigue = Outcome::Win {
            seat: PlayerId::P2,
            by_fatigue: true,
        };
        stats.add(&record(fatigue, PlayerId::P1, 50), PlayerId::P1);
        assert_eq!(
            (stats.draws, stats.turn_caps, stats.fatigue_wins),
            (1, 1, 1)
        );
        assert_eq!(stats.wins(), 1);
        assert_eq!(stats.games, 3);
    }

    #[test]
    fn per_player_numbers_are_measured_only_where_known() {
        let mut stats = PairingStats::new("deck_a", "deck_c");
        let mut game = record(win(PlayerId::P1), PlayerId::P1, 9);
        game.players[0] = PlayerRecord {
            mulligans: 1,
            stuck_on_lands: Some(true),
            flooded: None,
            turns_finished: 4,
            untapped_lands: 6,
        };
        game.players[1] = PlayerRecord {
            mulligans: 0,
            stuck_on_lands: Some(false),
            flooded: Some(false),
            turns_finished: 4,
            untapped_lands: 2,
        };
        stats.add(&game, PlayerId::P1);
        assert_eq!((stats.players, stats.players_who_mulliganed), (2, 1));
        assert_eq!((stats.stuck_measured, stats.stuck), (2, 1));
        assert_eq!((stats.flood_measured, stats.flooded), (1, 0));
        assert_eq!((stats.turns_finished, stats.untapped_lands), (8, 8));
    }

    #[test]
    fn numbers_are_shown_to_one_decimal() {
        assert_eq!(percent(1, 3), "33.3");
        assert_eq!(percent(2, 3), "66.7");
        assert_eq!(percent(5, 5), "100.0");
        assert_eq!(percent(0, 7), "0.0");
        assert_eq!(percent(0, 0), "-");
        assert_eq!(ratio(43, 3), "14.3");
        assert_eq!(ratio(0, 0), "-");
    }

    #[test]
    fn the_report_lists_pairings_and_errors() {
        let limits = &engine::data::embedded().unwrap().balance.sim;
        let mut clean = PairingStats::new("deck_a", "deck_b");
        clean.add(&record(win(PlayerId::P1), PlayerId::P1, 11), PlayerId::P1);
        let report = format_report(std::slice::from_ref(&clean), 1, 5, limits);
        assert!(report.contains("1 games per pairing, seeds 5 to 5, turn cap 100"));
        assert!(report.contains("deck_a v deck_b"));
        assert!(report.contains("No errors."));

        let mut broken = PairingStats::new("deck_b", "deck_c");
        broken.add_error(42, "something broke".to_string());
        let report = format_report(&[clean, broken], 1, 5, limits);
        assert!(report.contains("1 errors."));
        assert!(report.contains("deck_b v deck_c, seed 42: something broke"));
    }
}
