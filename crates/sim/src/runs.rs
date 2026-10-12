//! Playing whole runs headlessly: the AI plays both sides of every duel
//! and a simple policy makes the run's choices. It measures whether an act
//! is winnable, which single duels can't show.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Instant;

use engine::data;
use engine::duel::{DuelRng, GameResult};
use engine::run::{
    DuelKind, DuelOutcome, NodeKind, PLAYER, Phase, PlaneswalkerId, RunAction, RunConfig, RunEvent,
    RunState, apply_duel_action, apply_run_action, new_run, run_legal_actions,
};
use engine::{AiKind, ai_choose_action_with};

use crate::timing::Timing;

/// What happened in one run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunRecord {
    pub won: bool,
    pub reached_boss: bool,
    /// Wounds carried on arriving at the boss.
    pub wounds_at_boss: u32,
    /// `[normal, elite, boss]`: duels played and duels won.
    pub duels: [u32; 3],
    pub wins: [u32; 3],
    pub cards_added: u32,
    pub relics: u32,
    pub gold_left: u32,
    /// The points the feats picked scored, over the duels won that had any.
    pub feat_points: u32,
    /// For each feat the character can be offered, in how many of the
    /// duels won (the boss's aside) the duel met its first, second and
    /// third tier, whether or not the feat was picked.
    pub feats_met: BTreeMap<String, [u32; 3]>,
}

/// The run choice a sensible but simple player makes: rest and shop when
/// they are on offer, take the first feats and the first card of a reward,
/// heal at a rest site, buy what can be afforded, take a treasure's first
/// relic, and take from an event what leaves no wound. `choices` breaks
/// ties between map nodes.
fn choose(state: &RunState, actions: &[RunAction], choices: &mut DuelRng) -> RunAction {
    let first = |wanted: fn(&RunAction) -> bool| actions.iter().find(|action| wanted(action));
    let pick = match &state.phase {
        Phase::Map => {
            let kind = |action: &RunAction| match action {
                RunAction::Travel { node } => state.map.node(*node).map(|node| node.kind),
                _ => None,
            };
            // Elites only with a wound to spare; never when one loss ends
            // the run.
            let last_legs = data::embedded().is_ok_and(|data| {
                state.wounds.len() + 1 >= usize::from(data.balance.run.max_wounds)
            });
            let safe: Vec<&RunAction> = actions
                .iter()
                .filter(|action| kind(action) != Some(NodeKind::Elite) || !last_legs)
                .collect();
            let pool = if safe.is_empty() {
                actions.iter().collect()
            } else {
                safe
            };
            let index = choices.below(u32::try_from(pool.len()).unwrap_or(1)) as usize;
            pool.get(index).copied()
        }
        // The first two on offer, which is as good as two at random: the
        // AI doesn't play for them.
        Phase::Feats { .. } => first(|action| matches!(action, RunAction::ChooseFeats { .. })),
        Phase::Reward { .. } => first(|action| matches!(action, RunAction::TakeReward { .. })),
        Phase::Draft { .. } => first(|action| matches!(action, RunAction::TakeDraft { .. })),
        // The first relic; the gold, last on offer, when there is none.
        Phase::Treasure { .. } => first(|action| matches!(action, RunAction::TakeTreasure { .. })),
        // The first thing an event offers that doesn't leave a wound, found
        // by trying it on a copy of the run.
        Phase::Event { .. } => actions.iter().find(|action| {
            let mut after = state.clone();
            matches!(action, RunAction::TakeEvent { .. })
                && apply_run_action(&mut after, (*action).clone()).is_ok()
                && after.wounds.len() <= state.wounds.len()
        }),
        Phase::Rest => first(|action| matches!(action, RunAction::HealWound { .. })),
        Phase::Shop { .. } => first(|action| matches!(action, RunAction::Buy { .. })),
        Phase::Duel { .. } => first(|action| matches!(action, RunAction::FinishDuel)),
        Phase::Over { .. } => None,
    };
    // The last action on offer always moves on: leave, or skip.
    pick.or(actions.last()).cloned().unwrap_or(RunAction::Leave)
}

/// Plays one run to its end, with the old AI on both sides of every duel.
pub fn play_run(planeswalker: &str, seed: u64) -> Result<RunRecord, String> {
    play_run_with(
        planeswalker,
        seed,
        [AiKind::Heuristic; 2],
        &mut Timing::default(),
    )
}

/// Plays one run to its end. `ais` play the player's side and the enemy's
/// side of every duel; how long they took is added to `timing`.
pub fn play_run_with(
    planeswalker: &str,
    seed: u64,
    ais: [AiKind; 2],
    timing: &mut Timing,
) -> Result<RunRecord, String> {
    let started = Instant::now();
    let result = play(planeswalker, seed, ais, timing);
    timing.played(started.elapsed());
    result
}

/// Adds to `record` which tiers of each feat the duel just won met. Every
/// feat the character can be offered is counted, picked or not, since the
/// duel's stats don't depend on the pick.
fn note_feats_met(state: &RunState, record: &mut RunRecord) -> Result<(), String> {
    let run = &data::embedded().map_err(|error| error.to_string())?.run;
    let Some(stats) = &state.duel_stats else {
        return Ok(());
    };
    for feat in run.feats_for(&state.planeswalker) {
        let met = record.feats_met.entry(feat.id.0.clone()).or_default();
        for tier in 0..usize::from(feat.score(stats)).min(met.len()) {
            met[tier] += 1;
        }
    }
    Ok(())
}

fn play(
    planeswalker: &str,
    seed: u64,
    ais: [AiKind; 2],
    timing: &mut Timing,
) -> Result<RunRecord, String> {
    let config = RunConfig {
        planeswalker: PlaneswalkerId(planeswalker.to_string()),
        emblem: None,
    };
    let mut state = new_run(config, seed).map_err(|error| error.to_string())?;
    // The policy's own dice, so they don't disturb the run's.
    let mut choices = DuelRng::from_seed(seed ^ 0x5EED);
    let mut record = RunRecord::default();
    for _ in 0..1_000_000 {
        if let Phase::Over { won } = state.phase {
            record.won = won;
            record.relics = u32::try_from(state.relics.len()).unwrap_or(u32::MAX);
            record.gold_left = state.gold;
            return Ok(record);
        }
        if let Some(duel) = state.duel.as_ref().filter(|duel| duel.result.is_none()) {
            let player = duel
                .waiting_on()
                .ok_or("a duel has no result and waits on nobody")?;
            let ai = ais[player.index()];
            let thinking = Instant::now();
            let choice = ai_choose_action_with(duel, player, ai);
            timing.decision(ai, thinking.elapsed(), || {
                format!(
                    "{planeswalker} seed {seed}, duel turn {} ({:?}), {:?}",
                    duel.turn, duel.step, duel.decision
                )
            });
            let action = choice.map_err(|error| format!("AI: {error}"))?;
            apply_duel_action(&mut state, player, action).map_err(|error| error.to_string())?;
            continue;
        }
        let actions = run_legal_actions(&state);
        if actions.is_empty() {
            return Err(format!("nothing to do in {:?}", state.phase));
        }
        let action = choose(&state, &actions, &mut choices);
        // A duel won, the boss's aside: what it would have scored on every
        // feat, read before the run takes the duel away.
        let won = matches!(
            state.duel.as_ref().and_then(|duel| duel.result),
            Some(GameResult::Win { player, .. }) if player == PLAYER
        );
        let boss = matches!(
            state.phase,
            Phase::Duel {
                kind: DuelKind::Boss,
                ..
            }
        );
        if action == RunAction::FinishDuel && won && !boss {
            note_feats_met(&state, &mut record)?;
        }
        let events = apply_run_action(&mut state, action).map_err(|error| error.to_string())?;
        for event in events {
            match event {
                RunEvent::DuelStarted {
                    kind: DuelKind::Boss,
                    ..
                } if !record.reached_boss => {
                    record.reached_boss = true;
                    record.wounds_at_boss = u32::try_from(state.wounds.len()).unwrap_or(u32::MAX);
                }
                RunEvent::DuelEnded { kind, outcome, .. } => {
                    let index = match kind {
                        DuelKind::Normal => 0,
                        DuelKind::Elite => 1,
                        DuelKind::Boss => 2,
                    };
                    timing.game(ais);
                    record.duels[index] += 1;
                    record.wins[index] += u32::from(outcome == DuelOutcome::Won);
                }
                RunEvent::FeatsScored { points, .. } => record.feat_points += points,
                RunEvent::CardAdded { .. } => record.cards_added += 1,
                _ => {}
            }
        }
    }
    Err("the run did not end".to_string())
}

fn percent(part: u32, whole: u32) -> String {
    if whole == 0 {
        return "-".to_string();
    }
    let tenths = (u64::from(part) * 1000 + u64::from(whole) / 2) / u64::from(whole);
    format!("{}.{}", tenths / 10, tenths % 10)
}

fn average(total: u32, count: u32) -> String {
    if count == 0 {
        return "-".to_string();
    }
    let tenths = (u64::from(total) * 10 + u64::from(count) / 2) / u64::from(count);
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// The report for one planeswalker's runs. `results` has one entry per
/// run: its seed, and its record or what went wrong.
pub fn format_runs(planeswalker: &str, results: &[(u64, Result<RunRecord, String>)]) -> String {
    let mut out = String::new();
    let records: Vec<&RunRecord> = results
        .iter()
        .filter_map(|(_, result)| result.as_ref().ok())
        .collect();
    let runs = u32::try_from(records.len()).unwrap_or(u32::MAX);
    let count = |wanted: fn(&RunRecord) -> bool| {
        u32::try_from(records.iter().filter(|record| wanted(record)).count()).unwrap_or(u32::MAX)
    };
    let sum =
        |value: fn(&RunRecord) -> u32| records.iter().map(|record| value(record)).sum::<u32>();
    let at_boss = count(|record| record.reached_boss);
    // Writing to a String cannot fail.
    let _ = writeln!(out, "{planeswalker}: {runs} runs");
    let _ = writeln!(
        out,
        "  Won the act          {:>5}%",
        percent(count(|record| record.won), runs)
    );
    let _ = writeln!(
        out,
        "  Reached the boss     {:>5}%   with {} wounds on average",
        percent(at_boss, runs),
        average(sum(|record| record.wounds_at_boss), at_boss)
    );
    let _ = writeln!(
        out,
        "  Beat the boss        {:>5}%   of those who reached it",
        percent(sum(|record| record.wins[2]), at_boss)
    );
    for (index, name) in ["Normal duels", "Elite duels"].into_iter().enumerate() {
        let played: u32 = records.iter().map(|record| record.duels[index]).sum();
        let won: u32 = records.iter().map(|record| record.wins[index]).sum();
        let _ = writeln!(
            out,
            "  {name:<20} {:>5}%   won, {} played per run",
            percent(won, played),
            average(played, runs)
        );
    }
    let _ = writeln!(
        out,
        "  Per run: {} cards added, {} relics, {} gold unspent",
        average(sum(|record| record.cards_added), runs),
        average(sum(|record| record.relics), runs),
        average(sum(|record| record.gold_left), runs)
    );
    out.push_str(&format_feats(&records));
    let errors: Vec<&(u64, Result<RunRecord, String>)> = results
        .iter()
        .filter(|(_, result)| result.is_err())
        .collect();
    if errors.is_empty() {
        let _ = writeln!(out, "  No errors.");
    } else {
        let _ = writeln!(out, "  {} errors. The first few:", errors.len());
        for (seed, result) in errors.iter().take(5) {
            if let Err(message) = result {
                let _ = writeln!(out, "    seed {seed}: {message}");
            }
        }
    }
    out
}

/// The feats part of the report: how often a duel that was won met each
/// tier of each feat, and what the feats picked scored.
fn format_feats(records: &[&RunRecord]) -> String {
    let mut out = String::new();
    let won: u32 = records
        .iter()
        .map(|record| record.wins[0] + record.wins[1])
        .sum();
    let mut met: BTreeMap<&str, [u32; 3]> = BTreeMap::new();
    for record in records {
        for (feat, tiers) in &record.feats_met {
            let total = met.entry(feat.as_str()).or_default();
            for (sum, tier) in total.iter_mut().zip(tiers) {
                *sum += tier;
            }
        }
    }
    if won == 0 || met.is_empty() {
        return out;
    }
    let _ = writeln!(
        out,
        "  Feats met by accident (the AI doesn't play for them), of {won} duels won:"
    );
    let _ = writeln!(
        out,
        "    {:<12} {:>7} {:>7} {:>7}",
        "", "tier 1", "tier 2", "tier 3"
    );
    // In the order of the data, which is how the game shows them.
    let order: Vec<&str> = match data::embedded() {
        Ok(data) => data.run.feats.iter().map(|feat| feat.id.as_str()).collect(),
        Err(_) => Vec::new(),
    };
    let mut feats: Vec<(&str, [u32; 3])> = met.into_iter().collect();
    feats.sort_by_key(|(feat, _)| order.iter().position(|id| id == feat));
    for (feat, tiers) in feats {
        let _ = writeln!(
            out,
            "    {feat:<12} {:>6}% {:>6}% {:>6}%",
            percent(tiers[0], won),
            percent(tiers[1], won),
            percent(tiers[2], won)
        );
    }
    let points: u32 = records.iter().map(|record| record.feat_points).sum();
    let hundredths = (u64::from(points) * 100 + u64::from(won) / 2) / u64::from(won);
    let _ = writeln!(
        out,
        "    Two picked blindly scored {}.{:02} points a duel won, of 6",
        hundredths / 100,
        hundredths % 100
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_played_to_its_end_and_recorded() {
        let record = play_run("warden", 3).unwrap();
        let duels: u32 = record.duels.iter().sum();
        assert!(duels >= 3, "{record:?}");
        assert!(
            record
                .wins
                .iter()
                .zip(record.duels)
                .all(|(won, played)| *won <= played)
        );
        assert_eq!(record.won, record.wins[2] == 1);
        assert_eq!(record.reached_boss, record.duels[2] >= 1);
        // Every feat of the Warden's is counted for every duel won, and a
        // tier is never met more often than the one below it.
        let won = record.wins[0] + record.wins[1];
        assert!(won >= 1, "{record:?}");
        assert_eq!(record.feats_met.len(), 8);
        for tiers in record.feats_met.values() {
            assert!(tiers[0] <= won && tiers[1] <= tiers[0] && tiers[2] <= tiers[1]);
        }
        assert!(record.feat_points <= won * 6);
        assert_eq!(play_run("warden", 3), Ok(record));
        assert!(play_run("nobody", 3).is_err());
    }

    #[test]
    fn the_report_adds_runs_up() {
        let won = RunRecord {
            won: true,
            reached_boss: true,
            wounds_at_boss: 2,
            duels: [4, 1, 1],
            wins: [3, 1, 1],
            cards_added: 5,
            relics: 1,
            gold_left: 40,
            feat_points: 5,
            feats_met: BTreeMap::from([
                ("swift".to_string(), [2, 1, 0]),
                ("unbroken".to_string(), [4, 3, 1]),
            ]),
        };
        let lost = RunRecord {
            duels: [3, 0, 0],
            ..RunRecord::default()
        };
        let results = vec![(1, Ok(won)), (2, Ok(lost)), (3, Err("boom".to_string()))];
        let report = format_runs("warden", &results);
        assert!(report.contains("warden: 2 runs"), "{report}");
        assert!(report.contains("Won the act           50.0%"), "{report}");
        assert!(report.contains("with 2.0 wounds on average"), "{report}");
        assert!(report.contains("Beat the boss        100.0%"), "{report}");
        assert!(report.contains("Normal duels          42.9%"), "{report}");
        assert!(report.contains("3.5 played per run"), "{report}");
        // Four duels won, the boss's aside; Unbroken comes first in the data.
        assert!(report.contains("of 4 duels won"), "{report}");
        let unbroken = report.find("unbroken      100.0%   75.0%   25.0%");
        let swift = report.find("swift          50.0%   25.0%    0.0%");
        assert!(unbroken.is_some() && unbroken < swift, "{report}");
        assert!(report.contains("scored 1.25 points a duel won"), "{report}");
        assert!(report.contains("1 errors."), "{report}");
        assert!(report.contains("seed 3: boom"), "{report}");
    }
}
