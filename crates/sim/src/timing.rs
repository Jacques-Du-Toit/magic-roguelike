//! How long the AIs take to decide, measured while the games are played.
//! It is kept apart from the game records and the reports built on them,
//! which must come out the same every time.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Duration;

use engine::AiKind;

use crate::ai_name;

/// One AI's thinking, added up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiTiming {
    /// Every action it was asked for, a forced pass included.
    pub decisions: u64,
    pub thinking: Duration,
    /// Its slowest single decision.
    pub worst: Duration,
    /// How many sides of a game it played: two in a game against itself.
    pub seats: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Timing {
    pub ais: BTreeMap<AiKind, AiTiming>,
    pub games: u64,
    /// Time spent playing games, thinking included, summed over the threads.
    pub playing: Duration,
}

impl Timing {
    pub fn decision(&mut self, ai: AiKind, took: Duration) {
        let entry = self.ais.entry(ai).or_default();
        entry.decisions += 1;
        entry.thinking += took;
        entry.worst = entry.worst.max(took);
    }

    /// Counts one game played by `ais`, one per seat.
    pub fn game(&mut self, ais: [AiKind; 2]) {
        self.games += 1;
        for ai in ais {
            self.ais.entry(ai).or_default().seats += 1;
        }
    }

    pub fn played(&mut self, took: Duration) {
        self.playing += took;
    }

    pub fn merge(&mut self, other: &Timing) {
        self.games += other.games;
        self.playing += other.playing;
        for (ai, theirs) in &other.ais {
            let entry = self.ais.entry(*ai).or_default();
            entry.decisions += theirs.decisions;
            entry.thinking += theirs.thinking;
            entry.worst = entry.worst.max(theirs.worst);
            entry.seats += theirs.seats;
        }
    }

    /// The timing table. `wall` is how long the whole run took on `threads`
    /// threads.
    pub fn format(&self, wall: Duration, threads: usize) -> String {
        let ms = |time: Duration| time.as_secs_f64() * 1000.0;
        let per = |time: Duration, count: u64| {
            if count == 0 {
                "-".to_string()
            } else {
                format!("{:.3}", ms(time) / count as f64)
            }
        };
        let mut out = String::new();
        // Writing to a String cannot fail.
        let _ = writeln!(out, "Timing");
        let _ = writeln!(
            out,
            "{:<6} {:>11} {:>12} {:>12} {:>14}",
            "AI", "Decisions", "Average ms", "Worst ms", "ms per game"
        );
        for (ai, timing) in &self.ais {
            let _ = writeln!(
                out,
                "{:<6} {:>11} {:>12} {:>12.3} {:>14}",
                ai_name(*ai),
                timing.decisions,
                per(timing.thinking, timing.decisions),
                ms(timing.worst),
                per(timing.thinking, timing.seats),
            );
        }
        let _ = writeln!(
            out,
            "ms per game is one AI's thinking for its side of one game."
        );
        let _ = writeln!(
            out,
            "{} games, {} ms each on one core. The whole run took {:.1} s on {threads} threads.",
            self.games,
            per(self.playing, self.games),
            wall.as_secs_f64()
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_and_games_add_up_per_ai() {
        let mut timing = Timing::default();
        timing.decision(AiKind::Heuristic, Duration::from_millis(2));
        timing.decision(AiKind::Heuristic, Duration::from_millis(6));
        timing.game([AiKind::Heuristic; 2]);
        timing.played(Duration::from_millis(10));
        let mut total = Timing::default();
        total.merge(&timing);
        total.merge(&timing);
        let old = total.ais[&AiKind::Heuristic];
        assert_eq!((old.decisions, old.seats), (4, 4));
        assert_eq!(old.thinking, Duration::from_millis(16));
        assert_eq!(old.worst, Duration::from_millis(6));
        assert_eq!((total.games, total.playing), (2, Duration::from_millis(20)));

        let text = total.format(Duration::from_secs(3), 8);
        assert!(text.contains("old"), "{text}");
        // 16 ms over 4 decisions, and over 4 seats.
        assert!(text.contains("4.000"), "{text}");
        assert!(text.contains("6.000"), "{text}");
        assert!(
            text.contains("2 games, 10.000 ms each on one core"),
            "{text}"
        );
        assert!(text.contains("3.0 s on 8 threads"), "{text}");
    }
}
