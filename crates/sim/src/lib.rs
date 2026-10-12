//! Headless simulation: AI-vs-AI games across every pairing of the decks in
//! `data/decks/`, for finding engine errors and measuring balance.

pub mod explain;
pub mod game;
pub mod report;
pub mod runs;
pub mod timing;
pub mod versus;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread;
use std::time::Instant;

use engine::AiKind;
use engine::data;
use engine::decks::{DeckDef, DeckId};
use engine::duel::{DuelConfig, PlayerId};
use engine::run::{Loadout, duel_config, starting_deck};

use game::{GameRecord, play_game_with};
use report::{PairingStats, format_report};
use timing::Timing;
use versus::{VersusStats, format_versus};

pub const USAGE: &str = "\
Usage: sim [--games N] [--seed S] [--decks A,B,...] [--ai AI]
       sim --enemies [--games N] [--seed S] [--decks A,B,...] [--ai AI]
       sim --versus [--enemies] [--games N] [--seed S] [--decks A,B,...] [--ai AI] [--against AI]
       sim --runs [--games N] [--seed S] [--ai AI]
       sim --replay P1_DECK P2_DECK [--seed S] [--ai AI] [--against AI]
       sim --explain FILE [--last N]

Plays N AI-vs-AI games (default 100) for every pairing of the decks in
data/decks, mirror matches included, and prints a report. Game i of each
pairing uses seed S + i (S defaults to 1), so a run can be repeated exactly.
In the report's game i (counting from 0), the first-named deck is P1 when i
is even and P2 when i is odd.

--ai chooses the AI that plays: `new` (the default), the one that searches
by imagined games and that the game is played against, or `old`, the
heuristic one it is measured against. `new-knowing` and `new-guessing` are `new`
with and without knowledge of the opponent's decklist, whatever
data/balance.json says.
--decks keeps only the pairings between the decks named, for a quick sample.
With --enemies it names planeswalkers and enemies: a pairing is kept unless
its planeswalker or its enemy is left out of a list that names others.

--enemies plays each planeswalker's starting deck, with its commander and its
usual starting emblem, against every enemy deck of the run, with its emblems
and its commander if it has one, instead of the test decks.
The planeswalker is always P1, as the human is in a run.

--versus plays the --ai AI against the --against AI (default `old`) on the
same pairings: N games with --ai on the first-named deck, then N more, on
the seeds that follow, with the two AIs swapped. It reports how often --ai
wins, and how much each deck gains from being played by it.

--runs plays N whole runs with each planeswalker instead: the AI plays both
sides of every duel and a simple policy makes the run's choices. It reports
how often the act is won and where runs end.

--replay plays one game with seed S instead and prints what each player
did, e.g. to look into a game the report flagged. P1 is played by --ai and
P2 by --against (by --ai too, if --against is not given).

Every report ends with how long the AIs took to decide.

--explain replays a bug report copied from the game (Copy bug report, on
the duel or run screen, saved to FILE; `-` reads it from standard input)
and prints the AI's last N decisions (default 3) in that duel: the
position, what it chose, and everything it weighed with its score.";

/// The name an AI goes by on the command line and in the reports.
pub fn ai_name(ai: AiKind) -> &'static str {
    match ai {
        AiKind::Heuristic => "old",
        AiKind::Search => "new",
        AiKind::SearchKnowing => "new-knowing",
        AiKind::SearchGuessing => "new-guessing",
    }
}

fn parse_ai(flag: &str, text: &str) -> Result<AiKind, String> {
    match text {
        "old" => Ok(AiKind::Heuristic),
        "new" => Ok(AiKind::Search),
        "new-knowing" => Ok(AiKind::SearchKnowing),
        "new-guessing" => Ok(AiKind::SearchGuessing),
        _ => Err(format!(
            "{flag}: `{text}` is not an AI (old, new, new-knowing, new-guessing)"
        )),
    }
}

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
    /// Play `ai` against `against` on the same decks.
    pub versus: bool,
    /// The AI that plays, or in `versus` the one being tried.
    pub ai: AiKind,
    /// The other AI of `versus` and of a replay.
    pub against: Option<AiKind>,
    /// Only these decks (or planeswalkers and enemies); empty for all.
    pub decks: Vec<String>,
    /// A bug report from the game to replay and explain: a file, or `-`
    /// for standard input.
    pub explain: Option<String>,
    /// How many of the AI's last decisions `explain` shows.
    pub last: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            games: 100,
            seed: 1,
            replay: None,
            enemies: false,
            runs: false,
            versus: false,
            ai: AiKind::Search,
            against: None,
            decks: Vec::new(),
            explain: None,
            last: 3,
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
            "--versus" => options.versus = true,
            "--ai" => options.ai = parse_ai("--ai", value("--ai")?)?,
            "--against" => options.against = Some(parse_ai("--against", value("--against")?)?),
            "--decks" => {
                options.decks = value("--decks")?
                    .split(',')
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--explain" => options.explain = Some(value("--explain")?.to_string()),
            "--last" => {
                let text = value("--last")?;
                options.last = text
                    .parse()
                    .map_err(|_| format!("--last: `{text}` is not a whole number"))?;
            }
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

fn threads() -> usize {
    thread::available_parallelism().map_or(1, |cores| cores.get())
}

/// Runs `count` jobs across the machine's cores and returns their results
/// in job order, so the output doesn't depend on timing.
fn in_parallel<T: Send>(count: u32, job: impl Fn(u32) -> T + Sync) -> Vec<Option<T>> {
    let workers = threads();
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

/// Plays one game, turning a panic into an error message. The second part
/// is how long the AIs took.
fn play_safely(
    config: DuelConfig,
    seed: u64,
    ais: [AiKind; 2],
) -> (Result<GameRecord, String>, Timing) {
    let mut timing = Timing::default();
    let limits = match data::embedded() {
        Ok(data) => &data.balance.sim,
        Err(error) => return (Err(error.to_string()), timing),
    };
    let played = catch_unwind(AssertUnwindSafe(|| {
        play_game_with(config, seed, limits, ais, &mut timing)
    }));
    let result = match played {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(ToString::to_string)
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(format!("panic: {message}"))
        }
    };
    (result, timing)
}

/// Two decks that play each other.
struct Pairing {
    first: String,
    second: String,
    /// The duel's setup and the seat of the first-named deck. Game `i` uses
    /// entry `i` modulo their number: the test decks swap seats every game,
    /// so that neither is helped by any difference between the seats.
    setups: Vec<(DuelConfig, PlayerId)>,
}

/// One game of a pairing, as it turned out.
struct Played {
    seed: u64,
    first_seat: PlayerId,
    result: Result<GameRecord, String>,
}

/// Plays `options.games` games of a pairing, the first on `first_seed`,
/// with `first_ai` on the first-named deck.
fn play_games(
    pairing: &Pairing,
    options: &Options,
    first_seed: u64,
    first_ai: AiKind,
    second_ai: AiKind,
    timing: &mut Timing,
) -> Vec<Played> {
    let setup = |game: u32| {
        let index = game as usize % pairing.setups.len().max(1);
        pairing.setups.get(index)
    };
    let results = in_parallel(options.games, |game| {
        let Some((config, first_seat)) = setup(game) else {
            return (
                Err("the pairing has no setup".to_string()),
                Timing::default(),
            );
        };
        let mut ais = [second_ai; 2];
        ais[first_seat.index()] = first_ai;
        let seed = first_seed + u64::from(game);
        let (result, mut took) = play_safely(config.clone(), seed, ais);
        took.name_game(&format!(
            "{} v {} ({} v {}), seed {seed}",
            pairing.first,
            pairing.second,
            ai_name(first_ai),
            ai_name(second_ai)
        ));
        (result, took)
    });
    (0..options.games)
        .zip(results)
        .map(|(game, result)| {
            let (result, took) = result.unwrap_or_else(|| {
                (
                    Err("the game was never played".to_string()),
                    Timing::default(),
                )
            });
            timing.merge(&took);
            Played {
                seed: first_seed + u64::from(game),
                first_seat: setup(game).map_or(PlayerId::P1, |(_, seat)| *seat),
                result,
            }
        })
        .collect()
}

/// Whether `--decks` leaves `name` in, among the names of its `kind` (a
/// list that names none of them leaves all of them in).
fn wanted(options: &Options, name: &str, kind: &[&str]) -> bool {
    let named: Vec<&String> = options
        .decks
        .iter()
        .filter(|deck| kind.contains(&deck.as_str()))
        .collect();
    named.is_empty() || named.iter().any(|deck| *deck == name)
}

fn check_deck_names(options: &Options, known: &[&str]) -> Result<(), String> {
    match options
        .decks
        .iter()
        .find(|deck| !known.contains(&deck.as_str()))
    {
        Some(unknown) => Err(format!(
            "--decks: no `{unknown}` (there are: {})",
            known.join(", ")
        )),
        None => Ok(()),
    }
}

/// The pairings to play: the test decks against each other, or the
/// planeswalkers against the run's enemies.
fn pairings(options: &Options) -> Result<Vec<Pairing>, String> {
    if options.enemies {
        return enemy_pairings(options);
    }
    let data = data::embedded().map_err(|error| error.to_string())?;
    let ids: Vec<&str> = data.decks.keys().map(|id| id.0.as_str()).collect();
    check_deck_names(options, &ids)?;
    let decks: Vec<&DeckDef> = data
        .decks
        .values()
        .filter(|deck| wanted(options, &deck.id.0, &ids))
        .collect();
    let mut all = Vec::new();
    for (index, first) in decks.iter().enumerate() {
        for second in decks.iter().skip(index) {
            all.push(Pairing {
                first: first.id.0.clone(),
                second: second.id.0.clone(),
                setups: vec![
                    (
                        DuelConfig::from_decks(first, second, &data.balance.duel),
                        PlayerId::P1,
                    ),
                    (
                        DuelConfig::from_decks(second, first, &data.balance.duel),
                        PlayerId::P2,
                    ),
                ],
            });
        }
    }
    Ok(all)
}

/// Each planeswalker's starting deck against every enemy of the run, set
/// up exactly as the run sets a duel up.
fn enemy_pairings(options: &Options) -> Result<Vec<Pairing>, String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let planeswalkers: Vec<&str> = data
        .run
        .planeswalkers
        .iter()
        .map(|planeswalker| planeswalker.id.as_str())
        .collect();
    let enemies: Vec<&str> = data
        .run
        .enemies
        .iter()
        .map(|enemy| enemy.id.as_str())
        .collect();
    check_deck_names(options, &[planeswalkers.clone(), enemies.clone()].concat())?;
    let mut all = Vec::new();
    for planeswalker in &data.run.planeswalkers {
        for enemy in &data.run.enemies {
            if !wanted(options, planeswalker.id.as_str(), &planeswalkers)
                || !wanted(options, enemy.id.as_str(), &enemies)
            {
                continue;
            }
            let emblem = data
                .run
                .emblem(&planeswalker.emblem)
                .ok_or_else(|| format!("no starting emblem `{}`", planeswalker.emblem))?;
            let player = Loadout::player(
                &starting_deck(planeswalker),
                &planeswalker.colors,
                planeswalker.commander.as_ref(),
                vec![emblem],
                &data.cards,
                &data.balance,
            )
            .map_err(|error| error.to_string())?;
            let opponent = Loadout::enemy(enemy, &data.cards).map_err(|error| error.to_string())?;
            all.push(Pairing {
                first: planeswalker.id.0.clone(),
                second: enemy.id.0.clone(),
                setups: vec![(duel_config(player, opponent, &data.balance), PlayerId::P1)],
            });
        }
    }
    Ok(all)
}

/// Plays every pairing and returns its statistics, in deck order.
pub fn run(options: &Options) -> Result<Vec<PairingStats>, String> {
    run_timed(options, &mut Timing::default())
}

fn run_timed(options: &Options, timing: &mut Timing) -> Result<Vec<PairingStats>, String> {
    let mut all = Vec::new();
    for pairing in pairings(options)? {
        let mut stats = PairingStats::new(&pairing.first, &pairing.second);
        for played in play_games(
            &pairing,
            options,
            options.seed,
            options.ai,
            options.ai,
            timing,
        ) {
            match played.result {
                Ok(record) => stats.add(&record, played.first_seat),
                Err(message) => stats.add_error(played.seed, message),
            }
        }
        all.push(stats);
    }
    Ok(all)
}

/// Plays `options.ai` against `options.against` on every pairing, each way
/// round, and returns the statistics in deck order.
pub fn run_versus(options: &Options) -> Result<Vec<VersusStats>, String> {
    versus_timed(options, &mut Timing::default())
}

fn versus_timed(options: &Options, timing: &mut Timing) -> Result<Vec<VersusStats>, String> {
    let old = options.against.unwrap_or(AiKind::Heuristic);
    let mut all = Vec::new();
    for pairing in pairings(options)? {
        let mut stats = VersusStats::new(&pairing.first, &pairing.second);
        // The second way round is played on fresh seeds: on the same ones,
        // an AI against itself would win exactly half of the games, which
        // would say nothing about how much chance moves the numbers.
        let ways = [
            (options.ai, old, options.seed),
            (old, options.ai, options.seed + u64::from(options.games)),
        ];
        for (side, (first_ai, second_ai, first_seed)) in ways.into_iter().enumerate() {
            for played in play_games(&pairing, options, first_seed, first_ai, second_ai, timing) {
                let new_seat = if side == 0 {
                    played.first_seat
                } else {
                    played.first_seat.opponent()
                };
                match played.result {
                    Ok(record) => stats.sides[side].add(&record, new_seat),
                    Err(message) => stats.errors.push((played.seed, message)),
                }
            }
        }
        all.push(stats);
    }
    Ok(all)
}

/// What a run of the sim prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The same every time for the same options.
    pub report: String,
    /// How long the AIs took: different every time.
    pub timing: String,
    pub failed: bool,
}

/// Runs the simulation (or the one replayed game) and returns the report
/// and whether any game failed.
pub fn run_report(options: &Options) -> Result<(String, bool), String> {
    run_output(options).map(|output| (output.report, output.failed))
}

/// Runs the simulation (or the one replayed game).
pub fn run_output(options: &Options) -> Result<Output, String> {
    if let Some(file) = &options.explain {
        let report = if file == "-" {
            std::io::read_to_string(std::io::stdin())
        } else {
            std::fs::read_to_string(file)
        }
        .map_err(|error| format!("can't read `{file}`: {error}"))?;
        return Ok(Output {
            report: explain::explain(&report, options.last)?,
            timing: String::new(),
            failed: false,
        });
    }
    let data = data::embedded().map_err(|error| error.to_string())?;
    let limits = &data.balance.sim;
    if let Some((p1, p2)) = &options.replay {
        let deck = |id: &str| {
            data.decks
                .get(&DeckId(id.to_string()))
                .ok_or_else(|| format!("no deck `{id}` in data/decks"))
        };
        let config = DuelConfig::from_decks(deck(p1)?, deck(p2)?, &data.balance.duel);
        let ais = [options.ai, options.against.unwrap_or(options.ai)];
        let (report, failed) = game::replay(config, options.seed, limits, ais);
        return Ok(Output {
            report,
            timing: String::new(),
            failed,
        });
    }
    let started = Instant::now();
    let mut timing = Timing::default();
    let (report, failed) = if options.runs {
        run_runs(options, &mut timing)?
    } else if options.versus {
        let stats = versus_timed(options, &mut timing)?;
        let failed = stats.iter().any(|pairing| !pairing.errors.is_empty());
        let old = options.against.unwrap_or(AiKind::Heuristic);
        (
            format_versus(&stats, options.ai, old, options.games, options.seed, limits),
            failed,
        )
    } else {
        let stats = run_timed(options, &mut timing)?;
        let failed = stats.iter().any(|pairing| !pairing.errors.is_empty());
        (
            format_report(&stats, options.games, options.seed, limits),
            failed,
        )
    };
    Ok(Output {
        report,
        timing: timing.format(started.elapsed(), threads()),
        failed,
    })
}

/// Plays whole runs with every planeswalker and returns the report and
/// whether any run failed.
fn run_runs(options: &Options, timing: &mut Timing) -> Result<(String, bool), String> {
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
            let mut took = Timing::default();
            let result = match catch_unwind(AssertUnwindSafe(|| {
                runs::play_run_with(id, seed, [options.ai; 2], &mut took)
            })) {
                Ok(result) => result,
                Err(_) => Err("panic".to_string()),
            };
            (result, took)
        });
        let results: Vec<(u64, Result<runs::RunRecord, String>)> = (0..options.games)
            .zip(played)
            .map(|(game, result)| {
                let (result, took) = result.unwrap_or_else(|| {
                    (
                        Err("the run was never played".to_string()),
                        Timing::default(),
                    )
                });
                timing.merge(&took);
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
    use crate::game::{Outcome, play_game};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn arguments_have_defaults_and_can_be_set() {
        let defaults = parse_args(&[]).unwrap().unwrap();
        assert_eq!(defaults, Options::default());
        assert_eq!((defaults.games, defaults.seed), (100, 1));
        assert!(!defaults.enemies && !defaults.runs && !defaults.versus);
        assert_eq!(defaults.decks, Vec::<String>::new());
        assert_eq!(
            parse_args(&args("--games 1000 --seed 42")),
            Ok(Some(Options {
                games: 1000,
                seed: 42,
                ..Options::default()
            }))
        );
        assert_eq!(parse_args(&args("--seed 7")).unwrap().unwrap().games, 100);
        assert_eq!(parse_args(&args("--help")), Ok(None));
        assert_eq!(
            parse_args(&args("--replay deck_b deck_c --seed 9")),
            Ok(Some(Options {
                seed: 9,
                replay: Some(("deck_b".to_string(), "deck_c".to_string())),
                ..Options::default()
            }))
        );
        assert_eq!(
            parse_args(&args(
                "--versus --ai old --against old --decks deck_a,deck_c"
            )),
            Ok(Some(Options {
                versus: true,
                ai: AiKind::Heuristic,
                against: Some(AiKind::Heuristic),
                decks: vec!["deck_a".to_string(), "deck_c".to_string()],
                ..Options::default()
            }))
        );
        assert_eq!(
            parse_args(&args("--ai clever")),
            Err("--ai: `clever` is not an AI (old, new, new-knowing, new-guessing)".to_string())
        );
        assert_eq!(
            parse_args(&args("--replay deck_b")),
            Err("--replay needs a value".to_string())
        );
    }

    #[test]
    fn a_replay_tells_the_story_of_one_game() {
        let options = parse_args(&args("--replay deck_a deck_c --seed 3 --ai old"))
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

        let unknown = parse_args(&args("--replay deck_a deck_z --ai old"))
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
            ai: AiKind::Heuristic,
            ..Options::default()
        };
        assert_eq!(
            parse_args(&args("--enemies --games 2 --ai old")),
            Ok(Some(options.clone()))
        );
        let stats = run(&options).unwrap();
        // Each planeswalker against each enemy, in file order.
        let run_data = &data::embedded().unwrap().run;
        let enemies = run_data.enemies.len();
        assert_eq!(stats.len(), run_data.planeswalkers.len() * enemies);
        assert_eq!(stats[0].first_deck, "warden");
        assert_eq!(stats[0].second_deck, run_data.enemies[0].id.as_str());
        let last = stats.last().unwrap();
        assert_eq!(last.first_deck, "firebrand");
        assert_eq!(last.second_deck, "pyre_tyrant");
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
        let options = parse_args(&args("--runs --games 2 --seed 5 --ai old"))
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
    fn decks_can_be_left_out_for_a_quick_sample() {
        let options = parse_args(&args("--games 2 --decks deck_a,deck_c --ai old"))
            .unwrap()
            .unwrap();
        let pairings: Vec<(String, String)> = run(&options)
            .unwrap()
            .into_iter()
            .map(|pairing| (pairing.first_deck, pairing.second_deck))
            .collect();
        let pair = |first: &str, second: &str| (first.to_string(), second.to_string());
        assert_eq!(
            pairings,
            [
                pair("deck_a", "deck_a"),
                pair("deck_a", "deck_c"),
                pair("deck_c", "deck_c")
            ]
        );

        // With --enemies a list may name planeswalkers, enemies or both.
        let enemy = data::embedded().unwrap().run.enemies[0].id.0.clone();
        let one_enemy = Options {
            games: 1,
            enemies: true,
            ai: AiKind::Heuristic,
            decks: vec![enemy.clone()],
            ..Options::default()
        };
        let stats = run(&one_enemy).unwrap();
        assert_eq!(stats.len(), 2);
        assert!(stats.iter().all(|pairing| pairing.second_deck == enemy));
        let one_each = Options {
            decks: vec![enemy, "warden".to_string()],
            ..one_enemy
        };
        assert_eq!(run(&one_each).unwrap().len(), 1);

        let unknown = parse_args(&args("--decks deck_z --ai old"))
            .unwrap()
            .unwrap();
        let error = run(&unknown).unwrap_err();
        assert!(error.starts_with("--decks: no `deck_z`"), "{error}");
    }

    #[test]
    fn one_ai_plays_another_both_ways_round_on_fresh_seeds() {
        let options = parse_args(&args("--versus --ai old --games 4 --decks deck_a,deck_c"))
            .unwrap()
            .unwrap();
        let stats = run_versus(&options).unwrap();
        assert_eq!(stats.len(), 3);
        for pairing in &stats {
            assert_eq!(pairing.errors, []);
            assert_eq!(pairing.sides[0].games, 4);
            assert_eq!(pairing.sides[1].games, 4);
        }
        assert_eq!(run_versus(&options).unwrap(), stats);

        // The AI with the first-named deck plays seeds 1 to 4 exactly as the
        // plain report does; the other way round uses seeds 5 to 8.
        let plain = run(&Options {
            versus: false,
            ..options.clone()
        })
        .unwrap();
        assert_eq!(stats[1].sides[0].new_wins, plain[1].first_deck_wins);
        let later = run(&Options {
            versus: false,
            seed: 5,
            ..options.clone()
        })
        .unwrap();
        assert_eq!(stats[1].sides[1].new_wins, later[1].second_deck_wins);

        let output = run_output(&options).unwrap();
        assert!(!output.failed);
        assert!(
            output
                .report
                .contains("the `old` AI against the `old` AI, 4 games each way round"),
            "{}",
            output.report
        );
        assert!(output.timing.starts_with("Timing"), "{}", output.timing);
        // Three pairings, both ways round, four games each.
        assert!(output.timing.contains("\n24 games, "), "{}", output.timing);
    }

    #[test]
    fn the_new_ai_can_be_played_against_the_old() {
        let options = parse_args(&args("--versus --ai new --games 1 --decks deck_c"))
            .unwrap()
            .unwrap();
        assert_eq!(options.ai, AiKind::Search);
        let output = run_output(&options).unwrap();
        assert!(!output.failed, "{}", output.report);
        assert!(
            output.report.contains("the `new` AI against the `old` AI"),
            "{}",
            output.report
        );
        // Each of them played one side of both games.
        for ai in ["old", "new"] {
            assert!(
                output.timing.lines().any(|line| line.starts_with(ai)),
                "{}",
                output.timing
            );
        }
        assert_eq!(
            parse_args(&args("--ai new-guessing")).unwrap().unwrap().ai,
            AiKind::SearchGuessing
        );
    }

    #[test]
    fn every_report_comes_with_the_time_the_ais_took() {
        let options = parse_args(&args("--games 2 --decks deck_a --ai old"))
            .unwrap()
            .unwrap();
        let output = run_output(&options).unwrap();
        assert!(output.timing.contains("\n2 games, "), "{}", output.timing);
        assert!(output.timing.contains("old"), "{}", output.timing);

        let runs = parse_args(&args("--runs --games 1 --ai old"))
            .unwrap()
            .unwrap();
        let output = run_output(&runs).unwrap();
        assert!(output.timing.contains("old"), "{}", output.timing);
        assert!(!output.timing.contains("\n0 games"), "{}", output.timing);
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
            ai: AiKind::Heuristic,
            ..Options::default()
        };
        let stats = run(&options).unwrap();
        let pairings: Vec<(&str, &str)> = stats
            .iter()
            .map(|pairing| (pairing.first_deck.as_str(), pairing.second_deck.as_str()))
            .collect();
        // Every deck against itself and every deck after it.
        let data = data::embedded().unwrap();
        let ids: Vec<&str> = data.decks.keys().map(|id| id.0.as_str()).collect();
        let mut expected = Vec::new();
        for (index, first) in ids.iter().enumerate() {
            for second in &ids[index..] {
                expected.push((*first, *second));
            }
        }
        assert_eq!(pairings, expected);
        assert!(pairings.contains(&("deck_a", "deck_c")));
        for pairing in &stats {
            assert_eq!(pairing.games, 2);
            assert_eq!(pairing.errors, []);
        }
        let (report, failed) = run_report(&options).unwrap();
        assert!(!failed);
        assert!(report.contains("No errors."));
    }
}
