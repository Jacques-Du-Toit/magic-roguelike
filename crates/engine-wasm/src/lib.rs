//! Thin wasm-bindgen wrapper around the engine. It only serialises in and
//! out; it contains no game logic. Data crosses the boundary as JSON strings
//! (see `docs/ARCHITECTURE.md` §8).

pub mod session;

use wasm_bindgen::prelude::*;

use session::{RunSession, Session};

fn js_error(message: String) -> JsError {
    JsError::new(&message)
}

/// Returns the engine's `EngineInfo` as a JSON string.
#[wasm_bindgen]
pub fn engine_info() -> Result<String, JsError> {
    engine_info_json().map_err(|e| JsError::new(&e.to_string()))
}

fn engine_info_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&engine::engine_info())
}

/// Every card definition, as a JSON array of `CardDef`.
#[wasm_bindgen]
pub fn card_defs() -> Result<String, JsError> {
    session::card_defs_json().map_err(js_error)
}

/// Every decklist, as a JSON array of `DeckDef`.
#[wasm_bindgen]
pub fn decks() -> Result<String, JsError> {
    session::decks_json().map_err(js_error)
}

/// A duel between the human (P1) and the AI (P2). The `DuelState` lives in
/// wasm memory; JavaScript only holds this handle.
#[wasm_bindgen]
pub struct Duel {
    session: Session,
}

#[wasm_bindgen]
impl Duel {
    /// `seed` is a `u64` as a decimal string: a JS number would lose
    /// precision.
    #[wasm_bindgen(constructor)]
    pub fn new(human_deck: &str, ai_deck: &str, seed: &str) -> Result<Duel, JsError> {
        Session::new(human_deck, ai_deck, seed)
            .map(|session| Duel { session })
            .map_err(js_error)
    }

    pub fn seed(&self) -> String {
        self.session.seed()
    }

    /// The human's `PlayerView` as JSON.
    pub fn view(&self) -> Result<String, JsError> {
        self.session.view().map_err(js_error)
    }

    /// The human's legal actions, as a JSON array of `Action`.
    pub fn legal_actions(&self) -> Result<String, JsError> {
        self.session.legal_actions().map_err(js_error)
    }

    pub fn has_meaningful_action(&self) -> bool {
        self.session.has_meaningful_action()
    }

    /// Whether to pass priority for the human without asking them, given
    /// their hold-priority toggle. The rule is the engine's.
    pub fn should_auto_pass(&self, hold_priority: bool) -> bool {
        self.session.should_auto_pass(hold_priority)
    }

    /// Applies one of the human's legal actions (`Action` JSON) and returns
    /// a `DuelUpdate` as JSON.
    pub fn apply(&mut self, action: &str) -> Result<String, JsError> {
        self.session.apply(action).map_err(js_error)
    }

    /// Lets the AI take one action and returns a `DuelUpdate` as JSON.
    pub fn ai_step(&mut self) -> Result<String, JsError> {
        self.session.ai_step().map_err(js_error)
    }
}

/// Everything a run can contain, as `RunContent` JSON.
#[wasm_bindgen]
pub fn run_content() -> Result<String, JsError> {
    session::run_content_json().map_err(js_error)
}

/// A run, with whatever duel it is in. The `RunState` lives in wasm memory;
/// JavaScript only holds this handle.
#[wasm_bindgen]
pub struct Run {
    session: RunSession,
}

#[wasm_bindgen]
impl Run {
    /// `emblem` is the starting emblem's id, or empty for the planeswalker's
    /// usual one. `seed` is a `u64` as a decimal string.
    #[wasm_bindgen(constructor)]
    pub fn new(planeswalker: &str, emblem: &str, seed: &str) -> Result<Run, JsError> {
        RunSession::new(planeswalker, emblem, seed)
            .map(|session| Run { session })
            .map_err(js_error)
    }

    /// The `RunView` as JSON.
    pub fn view(&self) -> Result<String, JsError> {
        self.session.view().map_err(js_error)
    }

    /// Applies one of the legal run actions (`RunAction` JSON) and returns
    /// a `RunUpdate` as JSON.
    pub fn apply(&mut self, action: &str) -> Result<String, JsError> {
        self.session.apply(action).map_err(js_error)
    }

    /// The human's `PlayerView` of the duel in progress, as JSON.
    pub fn duel_view(&self) -> Result<String, JsError> {
        self.session.duel_view().map_err(js_error)
    }

    /// The human's legal actions in the duel, as a JSON array of `Action`.
    pub fn duel_legal_actions(&self) -> Result<String, JsError> {
        self.session.duel_legal_actions().map_err(js_error)
    }

    pub fn duel_should_auto_pass(&self, hold_priority: bool) -> bool {
        self.session.duel_should_auto_pass(hold_priority)
    }

    /// How the feats picked for the duel stand, as a JSON array of
    /// `FeatScore`.
    pub fn duel_feats(&self) -> Result<String, JsError> {
        self.session.duel_feats().map_err(js_error)
    }

    /// Applies one of the human's legal duel actions and returns a
    /// `DuelUpdate` as JSON.
    pub fn duel_apply(&mut self, action: &str) -> Result<String, JsError> {
        self.session.duel_apply(action).map_err(js_error)
    }

    /// The action the AI would take in the human's place, as `Action`
    /// JSON.
    pub fn duel_suggest(&self) -> Result<String, JsError> {
        self.session.duel_suggest().map_err(js_error)
    }

    /// Lets the enemy take one action in the duel and returns a
    /// `DuelUpdate` as JSON.
    pub fn duel_ai_step(&mut self) -> Result<String, JsError> {
        self.session.duel_ai_step().map_err(js_error)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_info_json_round_trips() {
        let json = engine_info_json().unwrap();
        let info: engine::EngineInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, engine::engine_info());
    }
}
