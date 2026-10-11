//! Pure rules engine. All game logic lives here; it knows nothing about
//! rendering, wasm or IO.
//!
//! So far it loads and validates the game data (cards, decklists, balance
//! numbers) and can start a duel and run its mulligans. [`engine_info`] is
//! what the web app calls to prove the wasm pipeline.

pub mod ai;
pub mod balance;
pub mod cards;
pub mod data;
pub mod decks;
pub mod duel;

pub use ai::ai_choose_action;
pub use duel::{
    apply_action, has_meaningful_action, legal_actions, new_duel, player_view, redact_events,
    result,
};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Identifies the engine build. Exported to TypeScript with `ts-rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
}

/// Returns the name and version of this engine build.
pub fn engine_info() -> EngineInfo {
    EngineInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_info_reports_crate_name_and_version() {
        let info = engine_info();
        assert_eq!(info.name, "engine");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
