//! One duel between the human and the AI, as the browser sees it. Plain
//! Rust with JSON strings in and out, so it can be tested natively; `lib.rs`
//! wraps it for wasm-bindgen.

use engine::cards::CardDef;
use engine::data;
use engine::decks::{DeckDef, DeckId};
use engine::duel::{
    Action, DuelConfig, DuelState, GameEvent, PlayerId, PlayerView, apply_action,
    has_meaningful_action, legal_actions, new_duel, player_view, redact_events, should_auto_pass,
};
use engine::run::{
    ENEMY, ModifierId, PLAYER, PlaneswalkerId, RunAction, RunConfig, RunEvent, RunState, RunView,
    apply_duel_action, apply_run_action, feat_scores, new_run, run_content, run_view,
};
use engine::{AiKind, ai_choose_action_with};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The human's seat. The starting player is still a seeded coin flip.
pub const HUMAN: PlayerId = PlayerId::P1;
/// The AI's seat.
pub const AI: PlayerId = PlayerId::P2;
/// The AI the game is played against, and the one behind `duel_suggest`:
/// the searching one. (Version 1 stays in the engine as its baseline.)
pub const GAME_AI: AiKind = AiKind::Search;

/// What one action did: the events, already redacted for the human, and the
/// human's view of the duel afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DuelUpdate {
    pub events: Vec<GameEvent>,
    pub view: PlayerView,
}

/// A duel in progress. The state stays on this side of the boundary.
#[derive(Debug, Clone)]
pub struct Session {
    state: DuelState,
    seed: u64,
}

fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

/// Every card definition, in id order, as a JSON array of `CardDef`.
pub fn card_defs_json() -> Result<String, String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let cards: Vec<&CardDef> = data.cards.iter().collect();
    to_json(&cards)
}

/// Every decklist, in id order, as a JSON array of `DeckDef`.
pub fn decks_json() -> Result<String, String> {
    let data = data::embedded().map_err(|error| error.to_string())?;
    let decks: Vec<&DeckDef> = data.decks.values().collect();
    to_json(&decks)
}

/// Reads a seed: a `u64` in decimal.
fn parse_seed(seed: &str) -> Result<u64, String> {
    seed.trim()
        .parse()
        .map_err(|_| format!("the seed `{seed}` is not a whole number that fits in 64 bits"))
}

impl Session {
    /// Starts a duel with the default numbers. `seed` is a `u64` in decimal.
    pub fn new(human_deck: &str, ai_deck: &str, seed: &str) -> Result<Session, String> {
        let seed = parse_seed(seed)?;
        let data = data::embedded().map_err(|error| error.to_string())?;
        let deck = |id: &str| {
            data.decks
                .get(&DeckId(id.to_string()))
                .ok_or_else(|| format!("no deck `{id}`"))
        };
        // Indexed like `PlayerId`: the human is P1.
        let config = DuelConfig::from_decks(deck(human_deck)?, deck(ai_deck)?, &data.balance.duel);
        let state = new_duel(config, seed).map_err(|error| error.to_string())?;
        Ok(Session { state, seed })
    }

    pub fn seed(&self) -> String {
        self.seed.to_string()
    }

    /// The human's `PlayerView`.
    pub fn view(&self) -> Result<String, String> {
        to_json(&self.human_view()?)
    }

    /// The human's legal actions, as a JSON array of `Action`. Empty unless
    /// the game is waiting on the human.
    pub fn legal_actions(&self) -> Result<String, String> {
        to_json(&legal_actions(&self.state, HUMAN))
    }

    pub fn has_meaningful_action(&self) -> bool {
        has_meaningful_action(&self.state, HUMAN)
    }

    /// Whether to pass priority for the human without asking them.
    /// `hold_priority` is their toggle.
    pub fn should_auto_pass(&self, hold_priority: bool) -> bool {
        should_auto_pass(&self.state, HUMAN, hold_priority)
    }

    /// Applies one of the human's legal actions, given as `Action` JSON.
    /// Returns a `DuelUpdate`.
    pub fn apply(&mut self, action: &str) -> Result<String, String> {
        let action: Action = serde_json::from_str(action)
            .map_err(|error| format!("`{action}` is not an action: {error}"))?;
        self.act(HUMAN, action)
    }

    /// Lets the AI take one action. Returns a `DuelUpdate`.
    pub fn ai_step(&mut self) -> Result<String, String> {
        let action =
            ai_choose_action_with(&self.state, AI, GAME_AI).map_err(|error| error.to_string())?;
        self.act(AI, action)
    }

    fn act(&mut self, player: PlayerId, action: Action) -> Result<String, String> {
        let events =
            apply_action(&mut self.state, player, action).map_err(|error| error.to_string())?;
        to_json(&DuelUpdate {
            events: redact_events(&events, HUMAN),
            view: self.human_view()?,
        })
    }

    fn human_view(&self) -> Result<PlayerView, String> {
        player_view(&self.state, HUMAN).map_err(|error| error.to_string())
    }
}

/// What one run action did: the events, and the run as it now stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunUpdate {
    pub events: Vec<RunEvent>,
    pub view: RunView,
}

/// Everything a run can contain, as `RunContent` JSON.
pub fn run_content_json() -> Result<String, String> {
    to_json(&run_content().map_err(|error| error.to_string())?)
}

/// A run in progress, with whatever duel it is in. The state stays on this
/// side of the boundary. In a run's duels the human is P1, as in `Session`.
#[derive(Debug, Clone)]
pub struct RunSession {
    state: RunState,
}

impl RunSession {
    /// Starts a run. `emblem` is the starting emblem's id, or empty for the
    /// planeswalker's usual one. `seed` is a `u64` in decimal.
    pub fn new(planeswalker: &str, emblem: &str, seed: &str) -> Result<RunSession, String> {
        let config = RunConfig {
            planeswalker: PlaneswalkerId(planeswalker.to_string()),
            emblem: (!emblem.is_empty()).then(|| ModifierId(emblem.to_string())),
        };
        let state = new_run(config, parse_seed(seed)?).map_err(|error| error.to_string())?;
        Ok(RunSession { state })
    }

    fn run_view(&self) -> Result<RunView, String> {
        run_view(&self.state).map_err(|error| error.to_string())
    }

    /// The `RunView`: the whole run and the legal run actions.
    pub fn view(&self) -> Result<String, String> {
        to_json(&self.run_view()?)
    }

    /// Applies one of the legal run actions, given as `RunAction` JSON.
    /// Returns a `RunUpdate`.
    pub fn apply(&mut self, action: &str) -> Result<String, String> {
        let action: RunAction = serde_json::from_str(action)
            .map_err(|error| format!("`{action}` is not a run action: {error}"))?;
        let events =
            apply_run_action(&mut self.state, action).map_err(|error| error.to_string())?;
        to_json(&RunUpdate {
            events,
            view: self.run_view()?,
        })
    }

    fn duel(&self) -> Result<&DuelState, String> {
        self.state
            .duel
            .as_ref()
            .ok_or_else(|| "no duel is being played".to_string())
    }

    fn duel_player_view(&self) -> Result<PlayerView, String> {
        player_view(self.duel()?, PLAYER).map_err(|error| error.to_string())
    }

    /// The human's `PlayerView` of the duel in progress.
    pub fn duel_view(&self) -> Result<String, String> {
        to_json(&self.duel_player_view()?)
    }

    /// The human's legal actions in the duel, as a JSON array of `Action`.
    pub fn duel_legal_actions(&self) -> Result<String, String> {
        to_json(&legal_actions(self.duel()?, PLAYER))
    }

    /// Whether to pass priority for the human without asking them.
    pub fn duel_should_auto_pass(&self, hold_priority: bool) -> bool {
        self.duel()
            .is_ok_and(|duel| should_auto_pass(duel, PLAYER, hold_priority))
    }

    /// How the feats picked for the duel stand, as a JSON array of
    /// `FeatScore`: what each would score if the duel were won now.
    pub fn duel_feats(&self) -> Result<String, String> {
        self.duel()?;
        to_json(&feat_scores(&self.state).map_err(|error| error.to_string())?)
    }

    /// Applies one of the human's legal duel actions. Returns a
    /// `DuelUpdate`.
    pub fn duel_apply(&mut self, action: &str) -> Result<String, String> {
        let action: Action = serde_json::from_str(action)
            .map_err(|error| format!("`{action}` is not an action: {error}"))?;
        self.duel_act(PLAYER, action)
    }

    /// The action the AI would take in the human's place, as `Action` JSON.
    /// It looks only at what the human may see. For hints and for tests
    /// that click through whole runs.
    pub fn duel_suggest(&self) -> Result<String, String> {
        let action = ai_choose_action_with(self.duel()?, PLAYER, GAME_AI)
            .map_err(|error| error.to_string())?;
        to_json(&action)
    }

    /// Lets the enemy take one action in the duel. Returns a `DuelUpdate`.
    pub fn duel_ai_step(&mut self) -> Result<String, String> {
        let action = ai_choose_action_with(self.duel()?, ENEMY, GAME_AI)
            .map_err(|error| error.to_string())?;
        self.duel_act(ENEMY, action)
    }

    fn duel_act(&mut self, player: PlayerId, action: Action) -> Result<String, String> {
        let events = apply_duel_action(&mut self.state, player, action)
            .map_err(|error| error.to_string())?;
        to_json(&DuelUpdate {
            events: redact_events(&events, PLAYER),
            view: self.duel_player_view()?,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use engine::duel::{CardRef, Decision, GameResult};

    fn view(session: &Session) -> PlayerView {
        serde_json::from_str(&session.view().unwrap()).unwrap()
    }

    fn actions(session: &Session) -> Vec<Action> {
        serde_json::from_str(&session.legal_actions().unwrap()).unwrap()
    }

    fn update(json: &str) -> DuelUpdate {
        serde_json::from_str(json).unwrap()
    }

    /// Takes the human's turn with the AI's brain, through the JSON API.
    fn human_step(session: &mut Session) -> DuelUpdate {
        let action = ai_choose_action_with(&session.state, HUMAN, AiKind::Heuristic).unwrap();
        assert!(actions(session).contains(&action));
        update(&session.apply(&to_json(&action).unwrap()).unwrap())
    }

    #[test]
    fn card_definitions_and_decks_cross_as_json() {
        let cards: Vec<CardDef> = serde_json::from_str(&card_defs_json().unwrap()).unwrap();
        assert_eq!(cards.len(), data::embedded().unwrap().cards.iter().count());
        assert!(cards.iter().any(|card| card.name == "Vampire Nighthawk"));
        // Costs cross in the same text form the card files use.
        assert!(
            card_defs_json()
                .unwrap()
                .contains(r#""mana_cost":"{1}{B}{B}""#)
        );

        let decks: Vec<DeckDef> = serde_json::from_str(&decks_json().unwrap()).unwrap();
        let ids: Vec<&str> = decks.iter().map(|deck| deck.id.0.as_str()).collect();
        assert!(ids.starts_with(&["deck_a", "deck_b", "deck_c"]), "{ids:?}");
    }

    #[test]
    fn a_new_duel_shows_the_human_their_own_hand_only() {
        let session = Session::new("deck_b", "deck_a", "7").unwrap();
        assert_eq!(session.seed(), "7");
        let view = view(&session);
        assert_eq!(view.viewer, HUMAN);
        let face_up = |hand: &[CardRef]| hand.iter().filter(|card| card.face.is_some()).count();
        assert_eq!(face_up(&view.players[HUMAN.index()].hand), 7);
        assert_eq!(view.players[AI.index()].hand.len(), 7);
        assert_eq!(face_up(&view.players[AI.index()].hand), 0);
        assert!(matches!(view.decision, Some(Decision::Mulligan { .. })));
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(Session::new("deck_b", "deck_a", "-1").is_err());
        assert!(Session::new("deck_b", "deck_a", "18446744073709551616").is_err());
        assert!(Session::new("deck_b", "deck_a", "seven").is_err());
        assert_eq!(
            Session::new("deck_z", "deck_a", "1").unwrap_err(),
            "no deck `deck_z`"
        );
        // The largest seed fits.
        assert!(Session::new("deck_b", "deck_a", "18446744073709551615").is_ok());

        let mut session = Session::new("deck_b", "deck_a", "1").unwrap();
        let before = session.view().unwrap();
        assert!(session.apply("not json").is_err());
        assert!(session.apply(r#"{"type":"confirm_attackers"}"#).is_err());
        assert_eq!(session.view().unwrap(), before);
    }

    #[test]
    fn only_the_player_being_waited_on_can_act() {
        for seed in 1..=4 {
            let mut session = Session::new("deck_b", "deck_a", &seed.to_string()).unwrap();
            if session.state.waiting_on() == Some(HUMAN) {
                assert!(!actions(&session).is_empty());
                assert!(session.ai_step().is_err());
            } else {
                assert_eq!(actions(&session), []);
                assert!(!session.has_meaningful_action());
                assert!(!session.should_auto_pass(false));
                assert!(session.apply(r#"{"type":"keep"}"#).is_err());
            }
        }
    }

    #[test]
    fn a_whole_game_plays_through_the_handle_without_leaking_hidden_cards() {
        for (ai_deck, seed) in [("deck_a", 3u64), ("deck_c", 4)] {
            let mut session = Session::new("deck_b", ai_deck, &seed.to_string()).unwrap();
            let mut last = None;
            for _ in 0..20_000 {
                let update = match session.state.waiting_on() {
                    Some(HUMAN) => human_step(&mut session),
                    Some(_) => update(&session.ai_step().unwrap()),
                    None => break,
                };
                for event in &update.events {
                    if let GameEvent::CardDrawn { player, card }
                    | GameEvent::CardPutOnBottom { player, card } = event
                    {
                        assert_eq!(card.face.is_some(), *player == HUMAN);
                    }
                }
                assert_eq!(update.view, view(&session));
                last = Some(update);
            }
            let last = last.unwrap();
            assert!(matches!(last.view.result, Some(GameResult::Win { .. })));
            assert!(matches!(
                last.events.last(),
                Some(GameEvent::GameOver { .. })
            ));
            assert_eq!(actions(&session), []);
            assert!(session.ai_step().is_err());
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_game() {
        let play = |seed: &str| {
            let mut session = Session::new("deck_b", "deck_c", seed).unwrap();
            let mut log = Vec::new();
            while let Some(player) = session.state.waiting_on() {
                log.push(if player == HUMAN {
                    to_json(&human_step(&mut session)).unwrap()
                } else {
                    session.ai_step().unwrap()
                });
            }
            log
        };
        assert_eq!(play("11"), play("11"));
        assert_ne!(play("11"), play("12"));
    }

    // ---- Runs ----

    fn run_view_of(session: &RunSession) -> RunView {
        serde_json::from_str(&session.view().unwrap()).unwrap()
    }

    fn run_update(json: &str) -> RunUpdate {
        serde_json::from_str(json).unwrap()
    }

    fn run_act(session: &mut RunSession, action: &RunAction) -> RunUpdate {
        run_update(&session.apply(&to_json(action).unwrap()).unwrap())
    }

    #[test]
    fn the_run_content_crosses_as_json() {
        let content: engine::run::RunContent =
            serde_json::from_str(&run_content_json().unwrap()).unwrap();
        let names: Vec<&str> = content
            .planeswalkers
            .iter()
            .map(|planeswalker| planeswalker.name.as_str())
            .collect();
        assert_eq!(names, ["The Warden", "The Firebrand"]);
        assert_eq!(content.relics.len(), 15);
    }

    #[test]
    fn a_run_starts_on_the_map_and_bad_input_is_an_error() {
        let mut session = RunSession::new("warden", "", "7").unwrap();
        let view = run_view_of(&session);
        assert_eq!(view.seed, "7");
        assert_eq!(view.phase, engine::run::Phase::Map);
        assert_eq!(view.actions.len(), 3);
        // No duel yet.
        assert!(session.duel_view().is_err());
        assert!(session.duel_ai_step().is_err());
        assert!(session.duel_feats().is_err());
        assert!(!session.duel_should_auto_pass(false));

        assert_eq!(
            RunSession::new("nobody", "", "7").unwrap_err(),
            "there is no planeswalker `nobody`"
        );
        assert!(RunSession::new("warden", "", "seven").is_err());
        let before = session.view().unwrap();
        assert!(session.apply("not json").is_err());
        assert!(session.apply(r#"{"type":"leave"}"#).is_err());
        assert_eq!(session.view().unwrap(), before);
    }

    /// Plays the duel in progress through the JSON API, the human with the
    /// AI's brain.
    fn play_run_duel(session: &mut RunSession) {
        for _ in 0..50_000 {
            let duel = session.state.duel.as_ref().unwrap();
            match duel.waiting_on() {
                None => return,
                Some(player) if player == PLAYER => {
                    let action = ai_choose_action_with(duel, PLAYER, GAME_AI).unwrap();
                    assert_eq!(session.duel_suggest().unwrap(), to_json(&action).unwrap());
                    let legal: Vec<Action> =
                        serde_json::from_str(&session.duel_legal_actions().unwrap()).unwrap();
                    assert!(legal.contains(&action));
                    let json = session.duel_apply(&to_json(&action).unwrap()).unwrap();
                    let update = update(&json);
                    for event in &update.events {
                        if let GameEvent::CardDrawn { player, card } = event {
                            assert_eq!(card.face.is_some(), *player == PLAYER);
                        }
                    }
                }
                Some(_) => {
                    assert!(session.duel_apply(r#"{"type":"pass_priority"}"#).is_err());
                    session.duel_ai_step().unwrap();
                }
            }
        }
        panic!("the duel did not end");
    }

    #[test]
    fn a_whole_run_plays_through_the_handle() {
        let mut session = RunSession::new("firebrand", "", "5").unwrap();
        let mut log = Vec::new();
        for _ in 0..500 {
            let view = run_view_of(&session);
            if matches!(view.phase, engine::run::Phase::Over { .. }) {
                assert!(view.actions.is_empty());
                break;
            }
            if view.actions == [RunAction::Concede] {
                let duel: PlayerView = serde_json::from_str(&session.duel_view().unwrap()).unwrap();
                assert_eq!(duel.viewer, PLAYER);
                // The feats picked for it, as the run's view has them.
                let feats: Vec<engine::run::FeatScore> =
                    serde_json::from_str(&session.duel_feats().unwrap()).unwrap();
                assert_eq!(feats, view.feats);
                play_run_duel(&mut session);
                continue;
            }
            // Take the last action on offer: it picks feats, finishes
            // duels, skips rewards, and leaves shops and rest sites.
            let action = view.actions.last().unwrap().clone();
            let update = run_act(&mut session, &action);
            assert_eq!(update.view, run_view_of(&session));
            log.extend(update.events);
        }
        assert!(matches!(log.last(), Some(RunEvent::RunEnded { .. })));
        let duels = log
            .iter()
            .filter(|event| matches!(event, RunEvent::DuelStarted { .. }))
            .count();
        assert!(duels >= 2, "only {duels} duels");
    }
}
