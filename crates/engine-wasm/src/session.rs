//! One duel between the human and the AI, as the browser sees it. Plain
//! Rust with JSON strings in and out, so it can be tested natively; `lib.rs`
//! wraps it for wasm-bindgen.

use engine::ai_choose_action;
use engine::cards::CardDef;
use engine::data;
use engine::decks::{DeckDef, DeckId};
use engine::duel::{
    Action, DuelConfig, DuelState, GameEvent, PlayerId, PlayerView, apply_action,
    has_meaningful_action, legal_actions, new_duel, player_view, redact_events,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The human's seat. The starting player is still a seeded coin flip.
pub const HUMAN: PlayerId = PlayerId::P1;
/// The AI's seat.
pub const AI: PlayerId = PlayerId::P2;

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

impl Session {
    /// Starts a duel with the default numbers. `seed` is a `u64` in decimal.
    pub fn new(human_deck: &str, ai_deck: &str, seed: &str) -> Result<Session, String> {
        let seed: u64 = seed
            .trim()
            .parse()
            .map_err(|_| format!("the seed `{seed}` is not a whole number that fits in 64 bits"))?;
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

    /// Applies one of the human's legal actions, given as `Action` JSON.
    /// Returns a `DuelUpdate`.
    pub fn apply(&mut self, action: &str) -> Result<String, String> {
        let action: Action = serde_json::from_str(action)
            .map_err(|error| format!("`{action}` is not an action: {error}"))?;
        self.act(HUMAN, action)
    }

    /// Lets the AI take one action. Returns a `DuelUpdate`.
    pub fn ai_step(&mut self) -> Result<String, String> {
        let action = ai_choose_action(&self.state, AI).map_err(|error| error.to_string())?;
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
        let action = ai_choose_action(&session.state, HUMAN).unwrap();
        assert!(actions(session).contains(&action));
        update(&session.apply(&to_json(&action).unwrap()).unwrap())
    }

    #[test]
    fn card_definitions_and_decks_cross_as_json() {
        let cards: Vec<CardDef> = serde_json::from_str(&card_defs_json().unwrap()).unwrap();
        assert_eq!(cards.len(), 29);
        assert!(cards.iter().any(|card| card.name == "Vampire Nighthawk"));
        // Costs cross in the same text form the card files use.
        assert!(
            card_defs_json()
                .unwrap()
                .contains(r#""mana_cost":"{1}{B}{B}""#)
        );

        let decks: Vec<DeckDef> = serde_json::from_str(&decks_json().unwrap()).unwrap();
        let ids: Vec<&str> = decks.iter().map(|deck| deck.id.0.as_str()).collect();
        assert_eq!(ids, ["deck_a", "deck_b", "deck_c"]);
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
}
