//! The balance lab: single duels and whole acts played with decks built for
//! the purpose, to measure what a card, a relic or a whole deck is worth.
//!
//! Everything here is played as the run plays it: the player's deck gets
//! its automatic basic lands and its commander, and the enemy its emblems
//! and its commander.

use std::fmt::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::Deserialize;

use engine::cards::{CardDef, CardDefId};
use engine::data::{self, GameData};
use engine::decks::DeckEntry;
use engine::duel::{CardInstance, DuelConfig, DuelRng, PlayerId};
use engine::run::{
    EnemyDef, EnemyTier, Loadout, ModifierDef, ModifierId, PlaneswalkerDef, duel_config,
    offered_spells, starting_deck,
};

use crate::game::Outcome;
use crate::runs::{self, Policy, RunRecord};
use crate::timing::Timing;
use crate::{Options, ai_name, in_parallel, play_safely};

/// The character the lab plays: the only one with a pool of its own.
const PLANESWALKER: &str = "warden";

/// The ideal decks that come with the build; `--data DIR` reads
/// `DIR/sim/ideal_decks.json` instead.
const IDEAL_DECKS: &str = include_str!("../../../data/sim/ideal_decks.json");

/// A deck built by hand to show what one package of the pool can do: the
/// spells only, as a player's deck is kept. Its basic lands are worked out
/// as the run does it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdealDeck {
    pub id: String,
    pub name: String,
    pub spells: Vec<DeckEntry>,
}

impl IdealDeck {
    pub fn cards(&self) -> Vec<CardDefId> {
        self.spells
            .iter()
            .flat_map(|entry| (0..entry.count).map(|_| entry.card.clone()))
            .collect()
    }
}

/// Reads and checks the ideal decks: every card must be one a deck may
/// hold.
pub fn ideal_decks(options: &Options, data: &GameData) -> Result<Vec<IdealDeck>, String> {
    let text = match &options.data {
        Some(dir) => {
            let file = format!("{dir}/sim/ideal_decks.json");
            std::fs::read_to_string(&file).map_err(|error| format!("can't read {file}: {error}"))?
        }
        None => IDEAL_DECKS.to_string(),
    };
    let decks: Vec<IdealDeck> =
        serde_json::from_str(&text).map_err(|error| format!("sim/ideal_decks.json: {error}"))?;
    for deck in &decks {
        for entry in &deck.spells {
            let fits = data
                .cards
                .get(&entry.card)
                .is_some_and(|card| !card.is_land() && !card.token && !card.is_legendary());
            if !fits || entry.count == 0 {
                return Err(format!(
                    "sim/ideal_decks.json: `{}` can't hold `{}`",
                    deck.id, entry.card
                ));
            }
        }
    }
    Ok(decks)
}

/// What every lab mode needs: the data, the character and the enemies to
/// play against.
struct Lab {
    data: &'static GameData,
    planeswalker: &'static PlaneswalkerDef,
    field: Vec<&'static EnemyDef>,
}

fn tier(name: &str) -> Option<EnemyTier> {
    match name {
        "easy" => Some(EnemyTier::Easy),
        "normal" => Some(EnemyTier::Normal),
        "elite" => Some(EnemyTier::Elite),
        "boss" => Some(EnemyTier::Boss),
        _ => None,
    }
}

impl Lab {
    fn new(options: &Options) -> Result<Lab, String> {
        let data = data::embedded().map_err(|error| error.to_string())?;
        let planeswalker = data
            .run
            .planeswalkers
            .iter()
            .find(|def| def.id.as_str() == PLANESWALKER)
            .ok_or_else(|| format!("no planeswalker `{PLANESWALKER}`"))?;
        // `--field` names tiers, enemies or both; without it, every enemy
        // that is not easy.
        let mut field = Vec::new();
        for enemy in &data.run.enemies {
            let named = options.field.iter().any(|name| {
                name == enemy.id.as_str() || tier(name).is_some_and(|tier| tier == enemy.tier)
            });
            if named || (options.field.is_empty() && enemy.tier != EnemyTier::Easy) {
                field.push(enemy);
            }
        }
        if let Some(unknown) = options.field.iter().find(|name| {
            tier(name).is_none()
                && !data
                    .run
                    .enemies
                    .iter()
                    .any(|enemy| enemy.id.as_str() == *name)
        }) {
            return Err(format!("--field: no tier or enemy `{unknown}`"));
        }
        if field.is_empty() {
            return Err("--field names no enemy".to_string());
        }
        Ok(Lab {
            data,
            planeswalker,
            field,
        })
    }

    /// The duel of a deck of the character's, with these relics, against
    /// an enemy.
    fn config(
        &self,
        spells: &[CardDefId],
        relics: &[&ModifierDef],
        enemy: &EnemyDef,
    ) -> Result<DuelConfig, String> {
        let deck: Vec<CardInstance> = spells.iter().cloned().map(CardInstance::new).collect();
        let player = Loadout::player(
            &deck,
            &self.planeswalker.colors,
            self.planeswalker.commander.as_ref(),
            relics.to_vec(),
            &self.data.cards,
            &self.data.balance,
        )
        .map_err(|error| error.to_string())?;
        let opponent =
            Loadout::enemy(enemy, &self.data.cards).map_err(|error| error.to_string())?;
        Ok(duel_config(player, opponent, &self.data.balance))
    }

    fn starting_spells(&self) -> Vec<CardDefId> {
        starting_deck(self.planeswalker)
            .into_iter()
            .map(|card| card.card_id)
            .collect()
    }

    fn field_names(&self) -> String {
        let names: Vec<&str> = self.field.iter().map(|enemy| enemy.id.as_str()).collect();
        names.join(", ")
    }
}

/// Wins out of games played.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Tally {
    games: u32,
    wins: u32,
}

impl Tally {
    fn add(&mut self, won: bool) {
        self.games += 1;
        self.wins += u32::from(won);
    }

    fn merge(&mut self, other: Tally) {
        self.games += other.games;
        self.wins += other.wins;
    }

    fn rate(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        f64::from(self.wins) / f64::from(self.games)
    }

    /// The variance of the measured rate.
    fn variance(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        self.rate() * (1.0 - self.rate()) / f64::from(self.games)
    }

    /// The 95% margin of the rate, in points.
    fn margin(self) -> f64 {
        196.0 * self.variance().sqrt()
    }

    fn percent(self) -> String {
        if self.games == 0 {
            return "-".to_string();
        }
        format!("{:.1}", self.rate() * 100.0)
    }
}

/// Plays one duel and says whether the player (P1) won it. A draw or a
/// game stopped at the turn cap is not a win.
fn play_duel(
    config: Result<DuelConfig, String>,
    seed: u64,
    options: &Options,
) -> (Result<bool, String>, Timing) {
    let config = match config {
        Ok(config) => config,
        Err(error) => return (Err(error), Timing::default()),
    };
    let (result, timing) = play_safely(config, seed, [options.ai; 2]);
    let won = result.map(|record| {
        matches!(
            record.outcome,
            Outcome::Win {
                seat: PlayerId::P1,
                ..
            }
        )
    });
    (won, timing)
}

/// `count` different numbers below `bound`, in rising order.
fn distinct(rng: &mut DuelRng, count: u32, bound: usize) -> Vec<usize> {
    let mut left: Vec<usize> = (0..bound).collect();
    let mut picked = Vec::new();
    for _ in 0..count {
        if left.is_empty() {
            break;
        }
        let pick = rng.below(u32::try_from(left.len()).unwrap_or(u32::MAX)) as usize;
        picked.push(left.remove(pick));
    }
    picked.sort_unstable();
    picked
}

fn errors_text(errors: &[String]) -> String {
    let mut out = String::new();
    if errors.is_empty() {
        out.push_str("No errors.\n");
    } else {
        // Writing to a String cannot fail.
        let _ = writeln!(out, "{} errors. The first few:", errors.len());
        for error in errors.iter().take(5) {
            let _ = writeln!(out, "  {error}");
        }
    }
    out
}

/// One line of a lift table: how the decks with a thing did against those
/// without it.
fn lift_line(name: &str, note: &str, with: Tally, all: Tally) -> (f64, String) {
    let without = Tally {
        games: all.games - with.games,
        wins: all.wins - with.wins,
    };
    let lift = (with.rate() - without.rate()) * 100.0;
    let margin = 196.0 * (with.variance() + without.variance()).sqrt();
    let line = format!(
        "  {name:<28} {note:<9} {:>6} {:>6}% {lift:>+6.1} {margin:>5.1}",
        with.games,
        with.percent()
    );
    (lift, line)
}

/// `--lift`: single duels of random decks (the starting deck and a few
/// cards of the pool, each as likely as the next), and for every card the
/// win rate of the decks that held it against that of those that didn't.
/// With `--relics N` each deck also has N relics at random, measured the
/// same way.
pub fn lift(options: &Options, timing: &mut Timing) -> Result<(String, bool), String> {
    let lab = Lab::new(options)?;
    let pool: Vec<&CardDef> = offered_spells(&lab.data.cards, &lab.planeswalker.colors);
    let relics = &lab.data.run.relics;
    let start = lab.starting_spells();
    let played = in_parallel(options.games, |game| {
        let mut rng = DuelRng::from_seed(options.seed.wrapping_add(u64::from(game)));
        let cards = distinct(&mut rng, options.extra, pool.len());
        let owned = distinct(&mut rng, options.relics, relics.len());
        let enemy = game as usize % lab.field.len();
        let mut spells = start.clone();
        spells.extend(
            cards
                .iter()
                .filter_map(|&index| pool.get(index))
                .map(|card| card.id.clone()),
        );
        let modifiers: Vec<&ModifierDef> = owned
            .iter()
            .filter_map(|&index| relics.get(index))
            .collect();
        let config = lab.config(&spells, &modifiers, lab.field[enemy]);
        let (won, took) = play_duel(config, rng.next_seed(), options);
        (cards, owned, enemy, won, took)
    });

    let mut all = Tally::default();
    let mut by_card = vec![Tally::default(); pool.len()];
    let mut by_relic = vec![Tally::default(); relics.len()];
    let mut by_enemy = vec![Tally::default(); lab.field.len()];
    let mut errors = Vec::new();
    for (game, result) in played.into_iter().enumerate() {
        let Some((cards, owned, enemy, won, took)) = result else {
            errors.push(format!("game {game}: never played"));
            continue;
        };
        timing.merge(&took);
        let won = match won {
            Ok(won) => won,
            Err(error) => {
                errors.push(format!("game {game}: {error}"));
                continue;
            }
        };
        all.add(won);
        by_enemy[enemy].add(won);
        for card in cards {
            by_card[card].add(won);
        }
        for relic in owned {
            by_relic[relic].add(won);
        }
    }

    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "Card lift: {} duels of the starting deck with {} random cards of the pool ({} cards) \
         and {} random relics, `{}` AI, seeds from {}.\nAgainst: {}.\n",
        options.games,
        options.extra,
        pool.len(),
        options.relics,
        ai_name(options.ai),
        options.seed,
        lab.field_names()
    );
    let _ = writeln!(
        out,
        "Won {}% of {} duels (+/- {:.1}).",
        all.percent(),
        all.games,
        all.margin()
    );
    for (enemy, tally) in lab.field.iter().zip(&by_enemy) {
        let _ = writeln!(
            out,
            "  {:<24} {:>6}%  +/- {:.1}",
            enemy.id.as_str(),
            tally.percent(),
            tally.margin()
        );
    }
    let header = format!(
        "\n  {:<28} {:<9} {:>6} {:>7} {:>6} {:>5}\n",
        "", "", "decks", "won", "lift", "+/-"
    );
    out.push_str(&header);
    let mut lines: Vec<(f64, String)> = pool
        .iter()
        .zip(&by_card)
        .map(|(card, with)| {
            let rarity = format!("{:?}", card.rarity).to_lowercase();
            lift_line(card.id.as_str(), &rarity, *with, all)
        })
        .collect();
    lines.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, line) in &lines {
        let _ = writeln!(out, "{line}");
    }
    if options.relics > 0 {
        out.push_str(&header);
        let mut lines: Vec<(f64, String)> = relics
            .iter()
            .zip(&by_relic)
            .map(|(relic, with)| lift_line(relic.id.as_str(), "relic", *with, all))
            .collect();
        lines.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, line) in &lines {
            let _ = writeln!(out, "{line}");
        }
    }
    out.push('\n');
    out.push_str(&errors_text(&errors));
    Ok((out, !errors.is_empty()))
}

/// A table of decks against the field: `options.games` duels a cell, on
/// the same seeds in every cell.
fn matchups(
    lab: &Lab,
    decks: &[(String, Vec<CardDefId>)],
    options: &Options,
    timing: &mut Timing,
) -> (String, bool) {
    let enemies = lab.field.len();
    let cells = decks.len() * enemies;
    let jobs = u32::try_from(cells)
        .unwrap_or(u32::MAX)
        .saturating_mul(options.games);
    let played = in_parallel(jobs, |job| {
        let cell = job as usize % cells.max(1);
        let game = job / u32::try_from(cells.max(1)).unwrap_or(1);
        let (_, spells) = &decks[cell / enemies];
        let config = lab.config(spells, &[], lab.field[cell % enemies]);
        let (won, took) = play_duel(config, options.seed.wrapping_add(u64::from(game)), options);
        (cell, won, took)
    });
    let mut tallies = vec![Tally::default(); cells];
    let mut errors = Vec::new();
    for (job, result) in played.into_iter().enumerate() {
        match result {
            Some((cell, Ok(won), took)) => {
                timing.merge(&took);
                tallies[cell].add(won);
            }
            Some((cell, Err(error), _)) => errors.push(format!(
                "{} v {}: {error}",
                decks[cell / enemies].0,
                lab.field[cell % enemies].id
            )),
            None => errors.push(format!("job {job}: never played")),
        }
    }

    let mut out = String::new();
    let width = decks
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0)
        .max(12);
    // Writing to a String cannot fail.
    let _ = write!(out, "  {:<width$} {:>6} {:>5}", "", "all", "+/-");
    for enemy in &lab.field {
        let short: String = enemy.id.as_str().chars().take(9).collect();
        let _ = write!(out, " {short:>9}");
    }
    out.push('\n');
    for (row, (name, _)) in decks.iter().enumerate() {
        let cells = &tallies[row * enemies..(row + 1) * enemies];
        let mut all = Tally::default();
        for cell in cells {
            all.merge(*cell);
        }
        let _ = write!(
            out,
            "  {name:<width$} {:>5}% {:>5.1}",
            all.percent(),
            all.margin()
        );
        for cell in cells {
            let _ = write!(out, " {:>8}%", cell.percent());
        }
        out.push('\n');
    }
    let cell_margin = Tally {
        games: options.games,
        wins: options.games / 2,
    }
    .margin();
    let _ = writeln!(
        out,
        "\n{} duels a cell: a cell is within about {cell_margin:.0} points.\n",
        options.games
    );
    out.push_str(&errors_text(&errors));
    (out, !errors.is_empty())
}

/// The ideal decks `--decks` leaves in: all of them when it names none.
fn chosen_decks(options: &Options, lab: &Lab) -> Result<Vec<IdealDeck>, String> {
    let all = ideal_decks(options, lab.data)?;
    if let Some(unknown) = options
        .decks
        .iter()
        .find(|name| *name != "start" && !all.iter().any(|deck| deck.id == **name))
    {
        let known: Vec<&str> = all.iter().map(|deck| deck.id.as_str()).collect();
        return Err(format!(
            "--decks: no ideal deck `{unknown}` (there are: start, {})",
            known.join(", ")
        ));
    }
    Ok(all
        .into_iter()
        .filter(|deck| options.decks.is_empty() || options.decks.contains(&deck.id))
        .collect())
}

/// `--ideal`: each ideal deck, and the starting deck, in single duels
/// against each enemy of the field. No relics.
pub fn ideal(options: &Options, timing: &mut Timing) -> Result<(String, bool), String> {
    let lab = Lab::new(options)?;
    let mut decks = Vec::new();
    if options.decks.is_empty() || options.decks.iter().any(|name| name == "start") {
        decks.push(("start".to_string(), lab.starting_spells()));
    }
    for deck in chosen_decks(options, &lab)? {
        decks.push((deck.id.clone(), deck.cards()));
    }
    let (table, failed) = matchups(&lab, &decks, options, timing);
    let report = format!(
        "Ideal decks in single duels, no relics, `{}` AI, seeds from {}.\n\n{table}",
        ai_name(options.ai),
        options.seed
    );
    Ok((report, failed))
}

/// `--leave-out DECK`: the ideal deck whole, and then once for each of its
/// cards with every copy of that card swapped for `--filler`, to show
/// which cards carry it.
pub fn leave_out(
    deck: &str,
    options: &Options,
    timing: &mut Timing,
) -> Result<(String, bool), String> {
    let lab = Lab::new(options)?;
    let all = ideal_decks(options, lab.data)?;
    let deck = all
        .iter()
        .find(|ideal| ideal.id == deck)
        .ok_or_else(|| format!("--leave-out: no ideal deck `{deck}`"))?;
    let filler = CardDefId(options.filler.clone());
    if lab.data.cards.get(&filler).is_none() {
        return Err(format!("--filler: no card `{filler}`"));
    }
    let mut variants = vec![("(whole deck)".to_string(), deck.cards())];
    for entry in &deck.spells {
        let spells = deck
            .cards()
            .into_iter()
            .map(|card| {
                if card == entry.card {
                    filler.clone()
                } else {
                    card
                }
            })
            .collect();
        variants.push((format!("-{}x {}", entry.count, entry.card), spells));
    }
    let (table, failed) = matchups(&lab, &variants, options, timing);
    let report = format!(
        "Leave one out: `{}` with each card in turn swapped for {filler}, `{}` AI, seeds from \
         {}.\nA row far below the whole deck is a card that carries it.\n\n{table}",
        deck.id,
        ai_name(options.ai),
        options.seed
    );
    Ok((report, failed))
}

/// Plays `options.games` whole runs under a policy and reports them under
/// `name`.
pub fn runs_report(
    name: &str,
    policy: &Policy,
    options: &Options,
    timing: &mut Timing,
) -> (String, bool) {
    let played = in_parallel(options.games, |game| {
        let seed = options.seed + u64::from(game);
        let mut took = Timing::default();
        let result = match catch_unwind(AssertUnwindSafe(|| {
            runs::play_run_with(PLANESWALKER, seed, [options.ai; 2], policy, &mut took)
        })) {
            Ok(result) => result,
            Err(_) => Err("panic".to_string()),
        };
        (result, took)
    });
    let results: Vec<(u64, Result<RunRecord, String>)> = (0..options.games)
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
    let failed = results.iter().any(|(_, result)| result.is_err());
    (runs::format_runs(name, &results), failed)
}

/// `--act`: each ideal deck played through whole acts as it is, taking no
/// card and no relic on the way.
pub fn act(options: &Options, timing: &mut Timing) -> Result<(String, bool), String> {
    let lab = Lab::new(options)?;
    let mut out = format!(
        "Ideal decks through whole acts, taking no cards and no relics, `{}` AI, seeds {} to \
         {}.\n\n",
        ai_name(options.ai),
        options.seed,
        options.seed + u64::from(options.games.saturating_sub(1))
    );
    let mut failed = false;
    let mut decks = Vec::new();
    if options.decks.iter().any(|name| name == "start") {
        decks.push(("start".to_string(), lab.starting_spells()));
    }
    for deck in chosen_decks(options, &lab)? {
        decks.push((deck.id.clone(), deck.cards()));
    }
    for (name, spells) in decks {
        let policy = Policy {
            deck: Some(spells),
            no_cards: true,
            no_relics: true,
            ..Policy::default()
        };
        let (report, bad) = runs_report(&name, &policy, options, timing);
        failed |= bad;
        out.push_str(&report);
        out.push('\n');
    }
    Ok((out, failed))
}

/// The relic ids a policy may name.
pub fn check_relics(ids: &[ModifierId]) -> Result<(), String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    match ids.iter().find(|id| data.run.relic(id).is_none()) {
        Some(unknown) => Err(format!("--with-relic: no relic `{unknown}`")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_args;

    fn parsed(text: &str) -> Options {
        let args: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        parse_args(&args).unwrap().unwrap()
    }

    #[test]
    fn the_ideal_decks_that_come_with_the_build_are_decks_a_player_could_have() {
        let data = data::embedded().unwrap();
        let decks = ideal_decks(&Options::default(), data).unwrap();
        assert!(!decks.is_empty());
        let warden = data
            .run
            .planeswalkers
            .iter()
            .find(|def| def.id.as_str() == PLANESWALKER)
            .unwrap();
        let pool = offered_spells(&data.cards, &warden.colors);
        for deck in &decks {
            assert!(!deck.name.is_empty(), "{}", deck.id);
            for card in deck.cards() {
                assert!(
                    pool.iter().any(|offered| offered.id == card),
                    "{}: {card} is not in the Warden's pool",
                    deck.id
                );
            }
        }
    }

    #[test]
    fn lift_plays_random_decks_and_lists_every_card_of_the_pool() {
        let options = parsed("--lift --games 6 --ai old --field boss --relics 1 --seed 3");
        let (report, failed) = lift(&options, &mut Timing::default()).unwrap();
        assert!(!failed, "{report}");
        assert!(report.contains("6 duels"), "{report}");
        assert!(report.contains("Against: "), "{report}");
        let data = data::embedded().unwrap();
        for relic in &data.run.relics {
            assert!(report.contains(relic.id.as_str()), "{report}");
        }
        assert!(report.contains("giant_growth"), "{report}");
        // The same options give the same report.
        assert_eq!(lift(&options, &mut Timing::default()).unwrap().0, report);
        assert!(lift(&parsed("--lift --field nobody"), &mut Timing::default()).is_err());
    }

    #[test]
    fn the_ideal_table_has_a_row_for_each_deck_and_a_column_for_each_enemy() {
        let options = parsed("--ideal --games 2 --ai old --field elite");
        let (report, failed) = ideal(&options, &mut Timing::default()).unwrap();
        assert!(!failed, "{report}");
        let data = data::embedded().unwrap();
        assert!(report.contains("\n  start"), "{report}");
        for deck in ideal_decks(&options, data).unwrap() {
            assert!(report.contains(&format!("\n  {}", deck.id)), "{report}");
        }
        assert!(ideal(&parsed("--ideal --decks nothing"), &mut Timing::default()).is_err());
    }

    #[test]
    fn leaving_a_card_out_gives_a_row_for_each_card_of_the_deck() {
        let data = data::embedded().unwrap();
        let deck = ideal_decks(&Options::default(), data).unwrap().remove(0);
        let options = parsed("--games 1 --ai old --field boss");
        let (report, failed) = leave_out(&deck.id, &options, &mut Timing::default()).unwrap();
        assert!(!failed, "{report}");
        assert!(report.contains("(whole deck)"), "{report}");
        for entry in &deck.spells {
            assert!(
                report.contains(&format!("-{}x {}", entry.count, entry.card)),
                "{report}"
            );
        }
        assert!(leave_out("nothing", &options, &mut Timing::default()).is_err());
    }

    #[test]
    fn an_ideal_deck_plays_whole_acts_and_ends_with_the_deck_it_began_with() {
        let options = parsed("--act --games 2 --ai old");
        let data = data::embedded().unwrap();
        let deck = ideal_decks(&options, data).unwrap().remove(0);
        let policy = Policy {
            deck: Some(deck.cards()),
            no_cards: true,
            no_relics: true,
            ..Policy::default()
        };
        let record = runs::play_run_with(
            PLANESWALKER,
            4,
            [options.ai; 2],
            &policy,
            &mut Timing::default(),
        )
        .unwrap();
        assert_eq!(record.cards_added, 0);
        assert_eq!(record.relics, 0);
        let (report, failed) = act(&options, &mut Timing::default()).unwrap();
        assert!(!failed, "{report}");
        assert!(report.contains(&format!("{}: 2 runs", deck.id)), "{report}");
    }
}
