//! One AI against another on the same decks: adding the games up and
//! printing the report.

use std::collections::BTreeMap;
use std::fmt::Write;

use engine::AiKind;
use engine::balance::SimBalance;
use engine::duel::PlayerId;

use crate::ai_name;
use crate::game::{GameRecord, Outcome};

/// Games counted from the side of the AI being tried (the "new" one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    pub games: u32,
    pub new_wins: u32,
    pub old_wins: u32,
    pub draws: u32,
    pub turn_caps: u32,
}

impl Tally {
    /// Counts one game in which the AI being tried sat in `new_seat`.
    pub fn add(&mut self, record: &GameRecord, new_seat: PlayerId) {
        self.games += 1;
        match record.outcome {
            Outcome::Win { seat, .. } if seat == new_seat => self.new_wins += 1,
            Outcome::Win { .. } => self.old_wins += 1,
            Outcome::Draw => self.draws += 1,
            Outcome::TurnCap => self.turn_caps += 1,
        }
    }

    pub fn merge(&mut self, other: &Tally) {
        self.games += other.games;
        self.new_wins += other.new_wins;
        self.old_wins += other.old_wins;
        self.draws += other.draws;
        self.turn_caps += other.turn_caps;
    }

    /// Games with a winner.
    pub fn decided(&self) -> u32 {
        self.new_wins + self.old_wins
    }
}

/// One pairing of decks, played both ways round.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersusStats {
    pub first_deck: String,
    pub second_deck: String,
    /// The games in which the AI being tried had the first-named deck, then
    /// those in which it had the second.
    pub sides: [Tally; 2],
    /// `(seed, what went wrong)`.
    pub errors: Vec<(u64, String)>,
}

impl VersusStats {
    pub fn new(first_deck: &str, second_deck: &str) -> VersusStats {
        VersusStats {
            first_deck: first_deck.to_string(),
            second_deck: second_deck.to_string(),
            ..VersusStats::default()
        }
    }

    pub fn total(&self) -> Tally {
        let mut total = self.sides[0];
        total.merge(&self.sides[1]);
        total
    }
}

/// `wins` as a share of `decided`, in percent.
fn share(wins: u32, decided: u32) -> Option<f64> {
    (decided > 0).then(|| f64::from(wins) * 100.0 / f64::from(decided))
}

fn percent(wins: u32, decided: u32) -> String {
    share(wins, decided).map_or("-".to_string(), |share| format!("{share:.1}"))
}

/// The 95% margin of error of a win rate over `decided` games, in points.
pub fn margin(wins: u32, decided: u32) -> Option<f64> {
    let p = share(wins, decided)? / 100.0;
    Some(1.96 * (p * (1.0 - p) / f64::from(decided)).sqrt() * 100.0)
}

fn margin_text(wins: u32, decided: u32) -> String {
    margin(wins, decided).map_or("-".to_string(), |margin| format!("{margin:.1}"))
}

/// The whole report as text.
pub fn format_versus(
    stats: &[VersusStats],
    new: AiKind,
    old: AiKind,
    games_per_side: u32,
    first_seed: u64,
    limits: &SimBalance,
) -> String {
    let (new, old) = (ai_name(new), ai_name(old));
    let mut out = String::new();
    let info = engine::engine_info();
    let last_seed = first_seed + u64::from((games_per_side * 2).saturating_sub(1));
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "{} {}: the `{new}` AI against the `{old}` AI, {games_per_side} games each way round \
         per pairing, seeds {first_seed} to {last_seed}, turn cap {}",
        info.name, info.version, limits.turn_cap
    );
    let width = stats
        .iter()
        .map(|pairing| pairing.first_deck.len() + pairing.second_deck.len() + 3)
        .max()
        .unwrap_or(0)
        .max(17);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<width$} {:>6} {:>6} {:>5} {:>7} {:>7} {:>6} {:>5} {:>7}",
        "Pairing", "Games", "Win%", "+/-", "With1%", "With2%", "Draws", "Cap", "Errors"
    );
    let mut overall = Tally::default();
    // Per deck: the games the AI being tried played with it, and the games
    // the other AI played with it.
    let mut by_deck: BTreeMap<&str, [Tally; 2]> = BTreeMap::new();
    for pairing in stats {
        let total = pairing.total();
        overall.merge(&total);
        let [with_first, with_second] = pairing.sides;
        by_deck.entry(&pairing.first_deck).or_default()[0].merge(&with_first);
        by_deck.entry(&pairing.second_deck).or_default()[0].merge(&with_second);
        by_deck.entry(&pairing.first_deck).or_default()[1].merge(&with_second);
        by_deck.entry(&pairing.second_deck).or_default()[1].merge(&with_first);
        let _ = writeln!(
            out,
            "{:<width$} {:>6} {:>6} {:>5} {:>7} {:>7} {:>6} {:>5} {:>7}",
            format!("{} v {}", pairing.first_deck, pairing.second_deck),
            total.games,
            percent(total.new_wins, total.decided()),
            margin_text(total.new_wins, total.decided()),
            percent(with_first.new_wins, with_first.decided()),
            percent(with_second.new_wins, with_second.decided()),
            total.draws,
            total.turn_caps,
            pairing.errors.len(),
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<width$} {:>6} {:>6} {:>5} {:>7} {:>7}",
        "Deck", "Games", "New%", "+/-", "Old%", "Gain"
    );
    for (deck, [with_new, with_old]) in &by_deck {
        let new_share = share(with_new.new_wins, with_new.decided());
        let old_share = share(with_old.old_wins, with_old.decided());
        let gain = match (new_share, old_share) {
            (Some(new_share), Some(old_share)) => format!("{:+.1}", new_share - old_share),
            _ => "-".to_string(),
        };
        let _ = writeln!(
            out,
            "{:<width$} {:>6} {:>6} {:>5} {:>7} {:>7}",
            deck,
            with_new.games,
            percent(with_new.new_wins, with_new.decided()),
            margin_text(with_new.new_wins, with_new.decided()),
            percent(with_old.old_wins, with_old.decided()),
            gain,
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Overall: `{new}` won {}% of {} games with a winner, +/- {} points.",
        percent(overall.new_wins, overall.decided()),
        overall.decided(),
        margin_text(overall.new_wins, overall.decided()),
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "Win%     games won by `{new}`, of games with a winner");
    let _ = writeln!(
        out,
        "+/-      the 95% margin of error of the win rate before it, in points"
    );
    let _ = writeln!(
        out,
        "With1%   `{new}`'s win rate when it has the first-named deck; With2%: the second"
    );
    let _ = writeln!(
        out,
        "New%     the deck's win rate with `{new}` playing it against `{old}`"
    );
    let _ = writeln!(
        out,
        "Old%     the deck's win rate with `{old}` playing it against `{new}`, same pairings"
    );
    let _ = writeln!(out, "Gain     New% less Old%");

    let errors: usize = stats.iter().map(|pairing| pairing.errors.len()).sum();
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

    fn record(outcome: Outcome) -> GameRecord {
        GameRecord {
            seed: 1,
            starting_player: PlayerId::P1,
            turns: 10,
            outcome,
            players: [PlayerRecord::default(), PlayerRecord::default()],
        }
    }

    fn win(seat: PlayerId) -> GameRecord {
        record(Outcome::Win {
            seat,
            by_fatigue: false,
        })
    }

    #[test]
    fn games_are_counted_from_the_new_ais_seat() {
        let mut tally = Tally::default();
        tally.add(&win(PlayerId::P1), PlayerId::P1);
        tally.add(&win(PlayerId::P1), PlayerId::P2);
        tally.add(&win(PlayerId::P2), PlayerId::P2);
        tally.add(&record(Outcome::Draw), PlayerId::P1);
        tally.add(&record(Outcome::TurnCap), PlayerId::P1);
        assert_eq!(
            tally,
            Tally {
                games: 5,
                new_wins: 2,
                old_wins: 1,
                draws: 1,
                turn_caps: 1
            }
        );
        assert_eq!(tally.decided(), 3);
    }

    #[test]
    fn the_margin_of_error_shrinks_with_more_games() {
        assert_eq!(margin(0, 0), None);
        let small = margin(50, 100).unwrap();
        let large = margin(500, 1000).unwrap();
        assert!((small - 9.8).abs() < 0.01, "{small}");
        assert!((large - 3.1).abs() < 0.01, "{large}");
    }

    #[test]
    fn the_report_shows_each_pairing_each_deck_and_the_total() {
        let limits = &engine::data::embedded().unwrap().balance.sim;
        let mut stats = VersusStats::new("deck_a", "deck_b");
        // With deck_a the new AI wins 3 of 4; with deck_b, 1 of 4.
        for seat in [PlayerId::P1, PlayerId::P1, PlayerId::P1, PlayerId::P2] {
            stats.sides[0].add(&win(seat), PlayerId::P1);
        }
        for seat in [PlayerId::P1, PlayerId::P2, PlayerId::P2, PlayerId::P2] {
            stats.sides[1].add(&win(seat), PlayerId::P1);
        }
        stats.errors.push((9, "boom".to_string()));
        let report = format_versus(&[stats], AiKind::Heuristic, AiKind::Heuristic, 4, 1, limits);
        assert!(report.contains("4 games each way round"), "{report}");
        assert!(report.contains("seeds 1 to 8"), "{report}");
        let line = |start: &str| {
            report
                .lines()
                .find(|line| {
                    line.starts_with(start) && line.contains(" v ") == start.contains(" v ")
                })
                .unwrap_or_else(|| panic!("no line for {start}: {report}"))
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        // Pairing: 8 games, 50% overall, 75% with the first deck, 25% with the second.
        let pairing = line("deck_a v deck_b");
        assert_eq!(pairing[3..5], ["8", "50.0"]);
        assert_eq!(pairing[6..8], ["75.0", "25.0"]);
        // deck_a wins 75% in the new AI's hands and 75% in the old one's.
        assert_eq!(line("deck_a ")[1..], ["4", "75.0", "42.4", "75.0", "+0.0"]);
        assert_eq!(line("deck_b ")[1..], ["4", "25.0", "42.4", "25.0", "+0.0"]);
        assert!(report.contains("won 50.0% of 8 games"), "{report}");
        assert!(report.contains("deck_a v deck_b, seed 9: boom"), "{report}");
    }
}
