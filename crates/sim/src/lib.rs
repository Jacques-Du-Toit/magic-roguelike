//! Headless simulation: AI-vs-AI games across every pairing of the decks in
//! `data/decks/`, for finding engine errors and measuring balance.

pub mod game;
pub mod report;
pub mod runs;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread;

use engine::data;
use engine::decks::{DeckDef, DeckId};
use engine::duel::{DuelConfig, PlayerId};
use engine::run::{Loadout, duel_config, starting_deck};

use game::{GameRecord, play_game};
use report::{PairingStats, format_report};

pub const USAGE: &str = "\
Usage: sim [--games N] [--seed S]
       sim --enemies [--games N] [--seed S]
       sim --runs [--games N] [--seed S]
       sim --replay P1_DECK P2_DECK [--seed S]

Plays N AI-vs-AI games (default 100) for every pairing of the decks in
data/decks, mirror matches included, and prints a report. Game i of each
pairing uses seed S + i (S defaults to 1), so a run can be repeated exactly.
In the report's game i (counting from 0), the first-named deck is P1 when i
is even and P2 when i is odd.

--enemies plays each planeswalker's starting deck, with its passive, against
every enemy deck of the run, with its emblems, instead of the test decks.
The planeswalker is always P1, as the human is in a run.

--runs plays N whole runs with each planeswalker instead: the AI plays both
sides of every duel and a simple policy makes the run's choices. It reports
how often the act is won and where runs end.

--replay plays one game with seed S instead and prints what each player
did, e.g. to look into a game the report flagged.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Games per pairing.
    pub games: u32,
    /// The seed of each pairing's first game, or of the replayed game.
    pub seed: u64,
    /// The decks of P1 and P2 for a single replayed game.
    pub replay: Option<(String, String)>,
    /// Play the run's starting decks against its enemy decks.
    pub enemies: bool,
    /// Play whole runs.
    pub runs: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            games: 100,
            seed: 1,
            replay: None,
            enemies: false,
            runs: false,
        }
    }
}

/// Reads the command line. `Ok(None)` means help was asked for.
pub fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value"))
                .map(String::as_str)
        };
        match flag.as_str() {
            "--help" | "-h" => return Ok(None),
            "--games" => {
                let text = value("--games")?;
                options.games = text
                    .parse()
                    .map_err(|_| format!("--games: `{text}` is not a whole number"))?;
            }
            "--seed" => {
                let text = value("--seed")?;
                options.seed = text
                    .parse()
                    .map_err(|_| format!("--seed: `{text}` is not a whole number"))?;
            }
            "--enemies" => options.enemies = true,
            "--runs" => options.runs = true,
            "--replay" => {
                let p1 = value("--replay")?.to_string();
                let p2 = value("--replay")?.to_string();
                options.replay = Some((p1, p2));
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(Some(options))
}

/// Runs `count` jobs across the machine's cores and returns their results
/// in job order, so the output doesn't depend on timing.
fn in_parallel<T: Send>(count: u32, job: impl Fn(u32) -> T + Sync) -> Vec<Option<T>> {
    let workers = thread::available_parallelism().map_or(1, |cores| cores.get());
    let job = &job;
    let mut results: Vec<Option<T>> = (0..count).map(|_| None).collect();
    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    (0..count)
                        .skip(worker)
                        .step_by(workers)
                        .map(|index| (index, job(index)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            // A worker can only fail by panicking outside a game, which
            // leaves its games as `None` for the caller to report.
            for (index, result) in handle.join().unwrap_or_default() {
                if let Some(slot) = results.get_mut(index as usize) {
                    *slot = Some(result);
                }
            }
        }
    });
    results
}

/// Plays one game, turning a panic into an error message.
fn play_safely(config: DuelConfig, seed: u64) -> Result<GameRecord, String> {
    let limits = &data::embedded()
        .map_err(|error| error.to_string())?
        .balance
        .sim;
    match catch_unwind(AssertUnwindSafe(|| play_game(config, seed, limits))) {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(ToString::to_string)
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(format!("panic: {message}"))
        }
    }
}

/// Plays the games of one pairing. `setup` gives each game its config and
/// the seat of the first-named deck.
fn play_pairing(
    stats: &mut PairingStats,
    options: &Options,
    setup: impl Fn(u32) -> (DuelConfig, PlayerId) + Sync,
) {
    let results = in_parallel(options.games, |game| {
        play_safely(setup(game).0, options.seed + u64::from(game))
    });
    for (game, result) in (0..options.games).zip(results) {
        let seed = options.seed + u64::from(game);
        match result {
            Some(Ok(record)) => stats.add(&record, setup(game).1),
            Some(Err(message)) => stats.add_error(seed, message),
            None => stats.add_error(seed, "the game was never played".to_string()),
        }
    }
}

/// Plays every pairing and returns its statistics, in deck order.
pub fn run(options: &Options) -> Result<Vec<PairingStats>, String> {
    if options.enemies {
        return run_enemies(options);
    }
    let data = data::embedded().map_err(|error| error.to_string())?;
    let decks: Vec<&DeckDef> = data.decks.values().collect();
    let mut all = Vec::new();
    for (index, first) in decks.iter().enumerate() {
        for second in &decks[index..] {
            let mut stats = PairingStats::new(&first.id.0, &second.id.0);
            // The decks swap seats every game, so neither is helped by any
            // difference between the seats.
            play_pairing(&mut stats, options, |game| {
                if game.is_multiple_of(2) {
                    (
                        DuelConfig::from_decks(first, second, &data.balance.duel),
                        PlayerId::P1,
                    )
                } else {
                    (
                        DuelConfig::from_decks(second, first, &data.balance.duel),
                        PlayerId::P2,
                    )
                }
            });
            all.push(stats);
        }
    }
    Ok(all)
}

/// Plays each planeswalker's starting deck against every enemy of the run,
/// set up exactly as the run sets a duel up.
fn run_enemies(options: &Options) -> Result<Vec<PairingStats>, String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let mut all = Vec::new();
    for planeswalker in &data.run.planeswalkers {
        for enemy in &data.run.enemies {
            let player = Loadout::player(
                &starting_deck(planeswalker),
                &planeswalker.colors,
                vec![&planeswalker.passive],
                &data.cards,
                &data.balance,
            )
            .map_err(|error| error.to_string())?;
            let opponent = Loadout::enemy(enemy, &data.cards).map_err(|error| error.to_string())?;
            let config = duel_config(player, opponent, &data.balance);
            let mut stats = PairingStats::new(&planeswalker.id.0, &enemy.id.0);
            play_pairing(&mut stats, options, |_| (config.clone(), PlayerId::P1));
            all.push(stats);
        }
    }
    Ok(all)
}
/// Runs the simulation (or the one replayed game) and returns the text to
/// print and whether any game failed.
pub fn run_report(options: &Options) -> Result<(String, bool), String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let limits = &data.balance.sim;
    if let Some((p1, p2)) = &options.replay {
        let deck = |id: &str| {
            data.decks
                .get(&DeckId(id.to_string()))
                .ok_or_else(|| format!("no deck `{id}` in data/decks"))
        };
        let config = DuelConfig::from_decks(deck(p1)?, deck(p2)?, &data.balance.duel);
        return Ok(game::replay(config, options.seed, limits));
    }
    if options.runs {
        return run_runs(options);
    }
    let stats = run(options)?;
    let failed = stats.iter().any(|pairing| !pairing.errors.is_empty());
    Ok((
        format_report(&stats, options.games, options.seed, limits),
        failed,
    ))
}

/// Plays whole runs with every planeswalker and returns the report and
/// whether any run failed.
fn run_runs(options: &Options) -> Result<(String, bool), String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let mut text = format!(
        "Whole runs, seeds {} to {}.\n\n",
        options.seed,
        options.seed + u64::from(options.games.saturating_sub(1))
    );
    let mut failed = false;
    for planeswalker in &data.run.planeswalkers {
        let id = planeswalker.id.as_str();
        let played = in_parallel(options.games, |game| {
            let seed = options.seed + u64::from(game);
            match catch_unwind(AssertUnwindSafe(|| runs::play_run(id, seed))) {
                Ok(result) => result,
                Err(_) => Err("panic".to_string()),
            }
        });
        let results: Vec<(u64, Result<runs::RunRecord, String>)> = (0..options.games)
            .zip(played)
            .map(|(game, result)| {
                let result = result.unwrap_or_else(|| Err("the run was never played".to_string()));
                (options.seed + u64::from(game), result)
            })
            .collect();
        failed |= results.iter().any(|(_, result)| result.is_err());
        text.push_str(&runs::format_runs(id, &results));
        text.push('\n');
    }
    Ok((text, failed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Outcome;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn arguments_have_defaults_and_can_be_set() {
        assert_eq!(
            parse_args(&[]),
            Ok(Some(Options {
                games: 100,
                seed: 1,
                replay: None,
                enemies: false,
                runs: false
            }))
        );
        assert_eq!(
            parse_args(&args("--games 1000 --seed 42")),
            Ok(Some(Options {
                games: 1000,
                seed: 42,
                replay: None,
                enemies: false,
                runs: false
            }))
        );
        assert_eq!(parse_args(&args("--seed 7")).unwrap().unwrap().games, 100);
        assert_eq!(parse_args(&args("--help")), Ok(None));
        assert_eq!(
            parse_args(&args("--replay deck_b deck_c --seed 9")),
            Ok(Some(Options {
                games: 100,
                seed: 9,
                replay: Some(("deck_b".to_string(), "deck_c".to_string())),
                enemies: false,
                runs: false
            }))
        );
        assert_eq!(
            parse_args(&args("--replay deck_b")),
            Err("--replay needs a value".to_string())
        );
    }

    #[test]
    fn a_replay_tells_the_story_of_one_game() {
        let options = parse_args(&args("--replay deck_a deck_c --seed 3"))
            .unwrap()
            .unwrap();
        let (text, failed) = run_report(&options).unwrap();
        assert!(!failed);
        assert!(text.starts_with("Replay, seed 3."), "{text}");
        assert!(text.contains("plays Forest") || text.contains("plays Plains"));
        assert!(text.contains("casts "));
        assert!(text.contains("attacks with "));
        assert!(text.trim_end().lines().last().unwrap().contains("wins"));
        assert_eq!(run_report(&options).unwrap().0, text);

        let unknown = parse_args(&args("--replay deck_a deck_z"))
            .unwrap()
            .unwrap();
        assert_eq!(
            run_report(&unknown),
            Err("no deck `deck_z` in data/decks".to_string())
        );
    }

    #[test]
    fn bad_arguments_are_explained() {
        assert_eq!(
            parse_args(&args("--games")),
            Err("--games needs a value".to_string())
        );
        assert_eq!(
            parse_args(&args("--games lots")),
            Err("--games: `lots` is not a whole number".to_string())
        );
        assert_eq!(
            parse_args(&args("--seed -1")),
            Err("--seed: `-1` is not a whole number".to_string())
        );
        assert_eq!(
            parse_args(&args("--fast")),
            Err("unknown argument `--fast`".to_string())
        );
    }

    #[test]
    fn the_starting_decks_can_be_played_against_every_enemy() {
        let options = Options {
            games: 2,
            enemies: true,
            ..Options::default()
        };
        assert_eq!(
            parse_args(&args("--enemies --games 2")),
            Ok(Some(options.clone()))
        );
        let stats = run(&options).unwrap();
        assert_eq!(stats.len(), 2 * 10);
        assert_eq!(stats[0].first_deck, "warden");
        assert_eq!(stats[0].second_deck, "goblin_raid");
        assert_eq!(stats[19].first_deck, "firebrand");
        assert_eq!(stats[19].second_deck, "pyre_tyrant");
        for pairing in &stats {
            assert_eq!(
                pairing.games, 2,
                "{} v {}",
                pairing.first_deck, pairing.second_deck
            );
            assert_eq!(pairing.errors, []);
        }
    }

    #[test]
    fn whole_runs_can_be_played_and_reported() {
        let options = parse_args(&args("--runs --games 2 --seed 5"))
            .unwrap()
            .unwrap();
        assert!(options.runs);
        let (report, failed) = run_report(&options).unwrap();
        assert!(!failed, "{report}");
        assert!(report.starts_with("Whole runs, seeds 5 to 6."), "{report}");
        assert!(report.contains("warden: 2 runs") && report.contains("firebrand: 2 runs"));
        assert_eq!(run_report(&options).unwrap().0, report);
    }

    #[test]
    fn jobs_come_back_in_order() {
        let results = in_parallel(50, |index| index * 2);
        let expected: Vec<Option<u32>> = (0..50).map(|index| Some(index * 2)).collect();
        assert_eq!(results, expected);
        assert_eq!(in_parallel(0, |index| index), []);
    }

    fn config() -> DuelConfig {
        let data = data::embedded().unwrap();
        let decks: Vec<&DeckDef> = data.decks.values().collect();
        DuelConfig::from_decks(decks[0], decks[2], &data.balance.duel)
    }

    #[test]
    fn a_game_is_played_to_a_result_and_recorded() {
        let limits = &data::embedded().unwrap().balance.sim;
        let record = play_game(config(), 3, limits).unwrap();
        assert_eq!(record.seed, 3);
        assert!(matches!(record.outcome, Outcome::Win { .. }));
        assert!(record.turns >= 4 && record.turns <= limits.turn_cap);
        for player in &record.players {
            assert!(player.turns_finished >= 1);
            assert!(player.turns_finished <= record.turns.div_ceil(2));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_record() {
        let limits = &data::embedded().unwrap().balance.sim;
        assert_eq!(
            play_game(config(), 9, limits),
            play_game(config(), 9, limits)
        );
    }

    #[test]
    fn a_game_past_the_turn_cap_is_stopped() {
        let mut limits = data::embedded().unwrap().balance.sim.clone();
        limits.turn_cap = 3;
        let record = play_game(config(), 3, &limits).unwrap();
        assert_eq!(record.outcome, Outcome::TurnCap);
        assert_eq!(record.turns, 3);
        // Too short to say anything about lands.
        assert!(
            record
                .players
                .iter()
                .all(|player| player.stuck_on_lands.is_none())
        );
    }

    #[test]
    fn land_measurements_are_taken_at_the_configured_turns() {
        let mut limits = data::embedded().unwrap().balance.sim.clone();
        // Nobody has more than 1 land at the end of their first turn, and
        // nobody's first seven or eight cards are all lands.
        limits.land_screw.turn = 1;
        limits.land_screw.max_lands = 1;
        limits.land_flood.turn = 1;
        limits.land_flood.min_land_percent = 100;
        let record = play_game(config(), 3, &limits).unwrap();
        for player in &record.players {
            assert_eq!(player.stuck_on_lands, Some(true));
            assert_eq!(player.flooded, Some(false));
        }
    }

    #[test]
    fn a_small_run_covers_every_pairing_without_errors() {
        let options = Options {
            games: 2,
            ..Options::default()
        };
        let stats = run(&options).unwrap();
        let pairings: Vec<(&str, &str)> = stats
            .iter()
            .map(|pairing| (pairing.first_deck.as_str(), pairing.second_deck.as_str()))
            .collect();
        assert_eq!(
            pairings,
            [
                ("deck_a", "deck_a"),
                ("deck_a", "deck_b"),
                ("deck_a", "deck_c"),
                ("deck_a", "deck_d"),
                ("deck_b", "deck_b"),
                ("deck_b", "deck_c"),
                ("deck_b", "deck_d"),
                ("deck_c", "deck_c"),
                ("deck_c", "deck_d"),
                ("deck_d", "deck_d"),
            ]
        );
        for pairing in &stats {
            assert_eq!(pairing.games, 2);
            assert_eq!(pairing.errors, []);
        }
        let (report, failed) = run_report(&options).unwrap();
        assert!(!failed);
        assert!(report.contains("No errors."));
    }
}
