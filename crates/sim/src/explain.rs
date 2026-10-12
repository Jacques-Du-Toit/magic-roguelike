//! Replaying a bug report from the web app and saying why the AI did what
//! it did.
//!
//! "Copy bug report" copies the seed, the decks (or the run's character)
//! and everything the human did. The engine and the AI are deterministic,
//! so that is enough to play the same game again, and at each of the AI's
//! decisions to ask it what it weighed.

use std::fmt::Write;

use engine::cards::CardDefId;
use engine::data::{self, CardDatabase};
use engine::decks::DeckId;
use engine::duel::{
    Action, DuelConfig, DuelState, PlayerId, Step, Target, apply_action, new_duel, player_view,
};
use engine::run::{
    ModifierId, PlaneswalkerId, RunAction, RunConfig, RunState, apply_duel_action,
    apply_run_action, new_run,
};
use engine::{AiKind, Explanation, ai_explain};
use serde::Deserialize;

/// The human's seat in the web app; the AI has the other.
const HUMAN: PlayerId = PlayerId::P1;
const AI: PlayerId = PlayerId::P2;
/// The AI the web app plays against.
const GAME_AI: AiKind = AiKind::Search;

/// What "Copy bug report" copies: from a practice duel, or from a run.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Report {
    Run {
        planeswalker: String,
        #[serde(default)]
        emblem: Option<String>,
        seed: String,
        inputs: Vec<Input>,
    },
    Duel {
        #[serde(rename = "humanDeck")]
        human_deck: String,
        #[serde(rename = "aiDeck")]
        ai_deck: String,
        seed: String,
        actions: Vec<Action>,
    },
}

/// One thing the human did in a run.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Input {
    Run(RunAction),
    Duel(Action),
}

/// One line of the story: something the human did, or an AI decision with
/// what it weighed.
enum Entry {
    Human(String),
    Ai(String),
}

fn card_name(cards: &CardDatabase, def: &CardDefId) -> String {
    cards
        .get(def)
        .map_or_else(|| def.to_string(), |card| card.name.clone())
}

fn object_name(state: &DuelState, cards: &CardDatabase, id: engine::duel::ObjectId) -> String {
    match state.objects.get(&id) {
        Some(object) => {
            let whose = if object.owner == AI { "its" } else { "your" };
            format!("{whose} {}", card_name(cards, &object.def))
        }
        None => state
            .emblems
            .iter()
            .find(|emblem| emblem.id == id)
            .map_or_else(|| format!("{id:?}"), |emblem| emblem.def.name.clone()),
    }
}

fn target_names(state: &DuelState, cards: &CardDatabase, targets: &[Target]) -> String {
    let names: Vec<String> = targets
        .iter()
        .map(|target| match target {
            Target::Player { player } if *player == AI => "itself".to_string(),
            Target::Player { .. } => "you".to_string(),
            Target::Object { object } => object_name(state, cards, *object),
        })
        .collect();
    names.join(" and ")
}

/// An action in words, named while the objects it mentions still exist.
fn action_name(state: &DuelState, cards: &CardDatabase, action: &Action) -> String {
    let named = |id| object_name(state, cards, id);
    let aimed = |targets: &[Target]| {
        if targets.is_empty() {
            String::new()
        } else {
            format!(" at {}", target_names(state, cards, targets))
        }
    };
    match action {
        Action::Keep => "keep the hand".to_string(),
        Action::Mulligan => "mulligan".to_string(),
        Action::BottomCard { card } => format!("put {} on the bottom", named(*card)),
        Action::PassPriority => "pass".to_string(),
        Action::PlayLand { card } => format!("play {}", named(*card)),
        Action::Discard { card } => format!("discard {}", named(*card)),
        Action::CastSpell { card, targets } => format!("cast {}{}", named(*card), aimed(targets)),
        Action::ActivateAbility {
            source,
            targets,
            sacrifice,
            ..
        } => {
            let sacrificing =
                sacrifice.map_or_else(String::new, |id| format!(", sacrificing {}", named(id)));
            format!(
                "use the ability of {}{sacrificing}{}",
                named(*source),
                aimed(targets)
            )
        }
        Action::Attack { creature } => format!("attack with {}", named(*creature)),
        Action::Block { blocker, attacker } => {
            format!("block {} with {}", named(*attacker), named(*blocker))
        }
        Action::RemoveAttacker { creature } => {
            format!("take {} out of the attack", named(*creature))
        }
        Action::RemoveBlocker { blocker } => format!("take {} out of the blocks", named(*blocker)),
        Action::ResetDeclaration => "start the declaration again".to_string(),
        Action::ConfirmAttackers => "confirm the attack".to_string(),
        Action::ConfirmBlockers => "confirm the blocks".to_string(),
        Action::AssignDamage { target } => {
            format!(
                "one more damage to {}",
                target_names(state, cards, &[*target])
            )
        }
        Action::UnassignDamage { target } => {
            format!(
                "one less damage to {}",
                target_names(state, cards, &[*target])
            )
        }
        Action::ConfirmDamage => "confirm the damage".to_string(),
        Action::ChooseTargets { targets } if targets.is_empty() => {
            "decline the ability".to_string()
        }
        Action::ChooseTargets { targets } => {
            format!("aim the ability at {}", target_names(state, cards, targets))
        }
        Action::ScryKeep => "keep the card on top".to_string(),
        Action::ScryBottom => "put the card on the bottom".to_string(),
        Action::ScryGraveyard => "put the card into the graveyard".to_string(),
    }
}

/// A whole play in words: one action, or an attack or set of blocks.
fn play_name(state: &DuelState, cards: &CardDatabase, play: &[Action], holding: bool) -> String {
    // A whole attack or set of blocks ends in its confirmation, which goes
    // without saying.
    let confirms =
        |action: &&Action| matches!(action, Action::ConfirmAttackers | Action::ConfirmBlockers);
    let steps: Vec<String> = play
        .iter()
        .filter(|action| !confirms(action))
        .map(|action| action_name(state, cards, action))
        .collect();
    let mut text = match (play.last(), steps.is_empty()) {
        (Some(Action::ConfirmAttackers), true) => "no attack".to_string(),
        (Some(Action::ConfirmBlockers), true) => "no blocks".to_string(),
        _ => steps.join(", "),
    };
    if holding {
        text.push_str(", keeping mana up for your turn");
    }
    text
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Untap => "untap step",
        Step::Upkeep => "upkeep",
        Step::Draw => "draw step",
        Step::PrecombatMain => "first main phase",
        Step::BeginCombat => "beginning of combat",
        Step::DeclareAttackers => "declare-attackers step",
        Step::DeclareBlockers => "declare-blockers step",
        Step::FirstStrikeDamage => "first-strike damage step",
        Step::CombatDamage => "combat damage step",
        Step::EndCombat => "end of combat",
        Step::PostcombatMain => "second main phase",
        Step::End => "end step",
        Step::Cleanup => "cleanup step",
    }
}

/// The position as the AI sees it, in a few lines.
fn position(state: &DuelState, cards: &CardDatabase) -> Result<String, String> {
    let view = player_view(state, AI).map_err(|error| error.to_string())?;
    let mut out = String::new();
    let whose = if state.active_player == AI {
        "the AI's turn"
    } else {
        "your turn"
    };
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "Turn {}, {whose}, {}. Life: you {}, the AI {}.",
        state.turn,
        step_name(state.step),
        state.player(HUMAN).life,
        state.player(AI).life
    );
    let hand: Vec<String> = view.players[AI.index()]
        .hand
        .iter()
        .filter_map(|card| card.face.as_ref())
        .map(|face| card_name(cards, &face.def))
        .collect();
    let _ = writeln!(
        out,
        "  The AI's hand: {}. Yours: {} cards.",
        if hand.is_empty() {
            "empty".to_string()
        } else {
            hand.join(", ")
        },
        state.player(HUMAN).hand.len()
    );
    for (player, label) in [(AI, "The AI's battlefield"), (HUMAN, "Your battlefield")] {
        let mut permanents = Vec::new();
        for permanent in &view.battlefield {
            if permanent.controller != player {
                continue;
            }
            let mut text = card_name(cards, &permanent.def);
            if let (Some(power), Some(toughness)) = (permanent.power, permanent.toughness) {
                let _ = write!(text, " {power}/{toughness}");
            }
            if permanent.tapped {
                text.push_str(" (tapped)");
            }
            permanents.push(text);
        }
        // Lands and tokens repeat: say each once with a count.
        let mut counted: Vec<(String, u32)> = Vec::new();
        for permanent in permanents {
            match counted.iter_mut().find(|(name, _)| *name == permanent) {
                Some((_, count)) => *count += 1,
                None => counted.push((permanent, 1)),
            }
        }
        let listed: Vec<String> = counted
            .into_iter()
            .map(|(name, count)| {
                if count == 1 {
                    name
                } else {
                    format!("{count} x {name}")
                }
            })
            .collect();
        let _ = writeln!(
            out,
            "  {label}: {}.",
            if listed.is_empty() {
                "nothing".to_string()
            } else {
                listed.join(", ")
            }
        );
    }
    if !state.stack.is_empty() {
        let items: Vec<String> = state
            .stack
            .iter()
            .map(|item| {
                let whose = if item.controller == AI { "its" } else { "your" };
                let what = state.objects.get(&item.id).map_or_else(
                    || format!("an ability of {}", object_name(state, cards, item.source())),
                    |object| card_name(cards, &object.def),
                );
                let aimed = if item.targets.is_empty() {
                    String::new()
                } else {
                    format!(" at {}", target_names(state, cards, &item.targets))
                };
                format!("{whose} {what}{aimed}")
            })
            .collect();
        let _ = writeln!(
            out,
            "  On the stack, the last on top: {}.",
            items.join("; ")
        );
    }
    Ok(out)
}

/// One AI decision, written up.
fn decision_text(
    state: &DuelState,
    cards: &CardDatabase,
    explanation: &Explanation,
) -> Result<String, String> {
    let mut out = position(state, cards)?;
    let _ = writeln!(
        out,
        "  The AI chose to {}.",
        action_name(state, cards, &explanation.chosen)
    );
    if let Some(note) = explanation.note {
        let _ = writeln!(out, "  Why: {note}.");
    }
    if !explanation.considered.is_empty() {
        let _ = writeln!(
            out,
            "  What it weighed, as the average over {} imagined games (higher is better for it):",
            explanation.worlds
        );
        let best = explanation.considered.iter().map(|c| c.total).max();
        for considered in &explanation.considered {
            let average = considered.total as f64 / f64::from(explanation.worlds.max(1));
            let mark = if Some(considered.total) == best {
                "  <- best"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    {average:>8.2}  {}{mark}",
                play_name(state, cards, &considered.play, considered.holding)
            );
        }
    }
    Ok(out)
}

/// Lets the AI act for as long as the duel waits on it, writing up each
/// decision that was more than a pass nobody is asked about.
///
/// `game` is the duel itself or the run it is part of; `duel` finds the
/// duel in it and `apply` makes the AI's move in it.
fn let_the_ai_act<Game>(
    game: &mut Game,
    duel: impl Fn(&Game) -> Option<&DuelState>,
    apply: impl Fn(&mut Game, Action) -> Result<(), String>,
    cards: &CardDatabase,
    story: &mut Vec<Entry>,
) -> Result<(), String> {
    for _ in 0..100_000 {
        let chosen = {
            let Some(state) = duel(game) else {
                return Ok(());
            };
            if state.result.is_some() || state.waiting_on() != Some(AI) {
                return Ok(());
            }
            let explanation = ai_explain(state, AI, GAME_AI).map_err(|error| error.to_string())?;
            // Not worth telling: a pass nobody is asked about, and the
            // later steps of a declaration already explained.
            let forced = explanation.considered.is_empty()
                && matches!(
                    explanation.chosen,
                    Action::PassPriority
                        | Action::Attack { .. }
                        | Action::Block { .. }
                        | Action::ConfirmAttackers
                        | Action::ConfirmBlockers
                        | Action::ResetDeclaration
                );
            if !forced {
                story.push(Entry::Ai(decision_text(state, cards, &explanation)?));
            }
            explanation.chosen
        };
        apply(game, chosen)?;
    }
    Err("the AI kept acting without the game moving on".to_string())
}

fn act_in_duel(state: &mut DuelState, action: Action) -> Result<(), String> {
    apply_action(state, AI, action)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn act_in_run(run: &mut RunState, action: Action) -> Result<(), String> {
    apply_duel_action(run, AI, action)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn parse_seed(seed: &str) -> Result<u64, String> {
    seed.parse()
        .map_err(|_| format!("the seed `{seed}` is not a whole number"))
}

/// Replays a bug report and returns the story of its last `last` AI
/// decisions, with what you did in between.
pub fn explain(report: &str, last: usize) -> Result<String, String> {
    let report: Report = serde_json::from_str(report.trim_start_matches('\u{feff}').trim())
        .map_err(|error| format!("that is not a bug report from the game: {error}"))?;
    let data = data::embedded().map_err(|error| error.to_string())?;
    let cards = &data.cards;
    let mut story: Vec<Entry> = Vec::new();
    let mut header = String::new();

    match report {
        Report::Duel {
            human_deck,
            ai_deck,
            seed,
            actions,
        } => {
            let deck = |id: &str| {
                data.decks
                    .get(&DeckId(id.to_string()))
                    .ok_or_else(|| format!("no deck `{id}` in data/decks"))
            };
            let config =
                DuelConfig::from_decks(deck(&human_deck)?, deck(&ai_deck)?, &data.balance.duel);
            let mut state = new_duel(config, parse_seed(&seed)?).map_err(|e| e.to_string())?;
            let _ = writeln!(
                header,
                "A practice duel: you on {human_deck} against the AI on {ai_deck}, seed {seed}, \
                 {} actions of yours.",
                actions.len()
            );
            let count = actions.len();
            for (index, action) in actions.into_iter().enumerate() {
                let_the_ai_act(
                    &mut state,
                    |duel| Some(duel),
                    act_in_duel,
                    cards,
                    &mut story,
                )?;
                if action != Action::PassPriority {
                    story.push(Entry::Human(action_name(&state, cards, &action)));
                }
                apply_action(&mut state, HUMAN, action.clone()).map_err(|error| {
                    format!(
                        "your action {} of {count} ({action:?}) doesn't replay: {error}",
                        index + 1
                    )
                })?;
            }
            let_the_ai_act(
                &mut state,
                |duel| Some(duel),
                act_in_duel,
                cards,
                &mut story,
            )?;
        }
        Report::Run {
            planeswalker,
            emblem,
            seed,
            inputs,
        } => {
            let config = RunConfig {
                planeswalker: PlaneswalkerId(planeswalker.clone()),
                emblem: emblem.filter(|id| !id.is_empty()).map(ModifierId),
            };
            let mut run = new_run(config, parse_seed(&seed)?).map_err(|e| e.to_string())?;
            let _ = writeln!(
                header,
                "A run as {planeswalker}, seed {seed}, {} inputs of yours.",
                inputs.len()
            );
            let count = inputs.len();
            let mut inputs = inputs.into_iter().enumerate();
            loop {
                let_the_ai_act(
                    &mut run,
                    |run| run.duel.as_ref(),
                    act_in_run,
                    cards,
                    &mut story,
                )?;
                let Some((index, input)) = inputs.next() else {
                    break;
                };
                let fails = |error: String| {
                    format!(
                        "your input {} of {count} doesn't replay: {error}",
                        index + 1
                    )
                };
                match input {
                    Input::Run(action) => {
                        // A new duel begins a new story.
                        if matches!(action, RunAction::Travel { .. } | RunAction::StartBoss) {
                            story.clear();
                        }
                        apply_run_action(&mut run, action)
                            .map_err(|error| fails(error.to_string()))?;
                    }
                    Input::Duel(action) => {
                        if action != Action::PassPriority
                            && let Some(duel) = &run.duel
                        {
                            story.push(Entry::Human(action_name(duel, cards, &action)));
                        }
                        apply_duel_action(&mut run, HUMAN, action)
                            .map_err(|error| fails(error.to_string()))?;
                    }
                }
            }
        }
    }

    // From the `last`-th AI decision from the end, to the end.
    let ai_entries = story
        .iter()
        .filter(|entry| matches!(entry, Entry::Ai(_)))
        .count();
    let mut skip = ai_entries.saturating_sub(last);
    let mut out = header;
    let _ = writeln!(
        out,
        "The AI's last {} decisions of {ai_entries} in this duel, oldest first. Every decision \
         is replayed exactly as it was made.\n",
        last.min(ai_entries)
    );
    let mut started = false;
    for entry in &story {
        match entry {
            Entry::Ai(text) => {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                started = true;
                out.push_str(text);
                out.push('\n');
            }
            Entry::Human(text) if started => {
                let _ = writeln!(out, "You: {text}.\n");
            }
            Entry::Human(_) => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::ai_choose_action_with;
    use engine::run::run_legal_actions;

    /// A practice duel as the web app plays it: the AI answers until the
    /// human must act, and everything the human does is recorded. The
    /// human here is version 1, for `actions` actions.
    fn duel_report(human_deck: &str, ai_deck: &str, seed: u64, actions: usize) -> String {
        let data = data::embedded().unwrap();
        let deck = |id: &str| &data.decks[&DeckId(id.to_string())];
        let config = DuelConfig::from_decks(deck(human_deck), deck(ai_deck), &data.balance.duel);
        let mut state = new_duel(config, seed).unwrap();
        let mut taken: Vec<Action> = Vec::new();
        while taken.len() < actions && state.result.is_none() {
            let player = state.waiting_on().unwrap();
            let kind = if player == AI {
                GAME_AI
            } else {
                AiKind::Heuristic
            };
            let action = ai_choose_action_with(&state, player, kind).unwrap();
            if player == HUMAN {
                taken.push(action.clone());
            }
            apply_action(&mut state, player, action).unwrap();
        }
        serde_json::json!({
            "humanDeck": human_deck,
            "aiDeck": ai_deck,
            "seed": seed.to_string(),
            "error": null,
            "actions": taken,
        })
        .to_string()
    }

    #[test]
    fn a_practice_duel_is_replayed_and_the_ais_decisions_explained() {
        let report = duel_report("deck_b", "deck_a", 7, 120);
        let text = explain(&report, 4).unwrap();
        assert!(
            text.starts_with("A practice duel: you on deck_b against the AI on deck_a, seed 7")
        );
        assert_eq!(text.matches("The AI chose to ").count(), 4, "{text}");
        assert!(
            text.contains("What it weighed, as the average over"),
            "{text}"
        );
        assert!(text.contains("<- best"), "{text}");
        assert!(text.contains("The AI's battlefield: "), "{text}");
        // The same report always tells the same story.
        assert_eq!(explain(&report, 4).unwrap(), text);
        // A byte-order mark or a newline from the clipboard does no harm.
        assert_eq!(explain(&format!("\u{feff}{report}\r\n"), 4).unwrap(), text);
    }

    #[test]
    fn a_report_that_does_not_fit_the_game_is_refused() {
        assert!(
            explain("not json", 3)
                .unwrap_err()
                .starts_with("that is not a bug report from the game")
        );
        // An action the game would not have allowed at that point.
        let mut report: serde_json::Value =
            serde_json::from_str(&duel_report("deck_b", "deck_a", 7, 10)).unwrap();
        report["actions"][3] = serde_json::json!({ "type": "confirm_attackers" });
        let error = explain(&report.to_string(), 3).unwrap_err();
        assert!(error.starts_with("your action 4 of 10"), "{error}");
        let unknown = duel_report("deck_b", "deck_a", 7, 2).replace("deck_a", "deck_z");
        assert_eq!(
            explain(&unknown, 3).unwrap_err(),
            "no deck `deck_z` in data/decks"
        );
    }

    #[test]
    fn a_run_is_replayed_into_its_duel() {
        // The first duel of a run, with the human's side played by version 1.
        let config = RunConfig {
            planeswalker: PlaneswalkerId("warden".to_string()),
            emblem: None,
        };
        let mut run = new_run(config, 4).unwrap();
        let mut inputs = Vec::new();
        let mut duel_actions = 0;
        while duel_actions < 80 {
            match run.duel.as_ref().filter(|duel| duel.result.is_none()) {
                None => {
                    let action = run_legal_actions(&run)[0].clone();
                    inputs.push(serde_json::json!({ "run": action }));
                    apply_run_action(&mut run, action).unwrap();
                }
                Some(duel) => {
                    let player = duel.waiting_on().unwrap();
                    let kind = if player == AI {
                        GAME_AI
                    } else {
                        AiKind::Heuristic
                    };
                    let action = ai_choose_action_with(duel, player, kind).unwrap();
                    if player == HUMAN {
                        inputs.push(serde_json::json!({ "duel": action }));
                        duel_actions += 1;
                    }
                    apply_duel_action(&mut run, player, action).unwrap();
                }
            }
        }
        let report = serde_json::json!({
            "planeswalker": "warden",
            "emblem": "emblem_welcome",
            "seed": "4",
            "error": null,
            "inputs": inputs,
        })
        .to_string();
        let text = explain(&report, 2).unwrap();
        assert!(text.starts_with("A run as warden, seed 4"), "{text}");
        assert_eq!(text.matches("The AI chose to ").count(), 2, "{text}");
    }
}
