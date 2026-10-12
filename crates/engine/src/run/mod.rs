//! The run: the roguelike layer around the duel. See
//! `docs/ARCHITECTURE.md` §6.
//!
//! It has the same shape as the duel: a serialisable [`RunState`] with a
//! seeded RNG, [`run_legal_actions`], and [`apply_run_action`] returning
//! events. A duel inside a run is started by the run and lives in
//! `RunState::duel`; its actions go through [`apply_duel_action`], and
//! `RunAction::FinishDuel` takes its result back into the run.

mod defs;
mod flow;
mod lands;
mod loadout;
mod map;
mod rewards;
mod state;
#[cfg(test)]
mod tests;
mod view;

pub use defs::{
    EnemyDef, EnemyId, EnemyTier, EventDef, EventEffect, EventId, EventOptionDef, ModifierDef,
    ModifierEffect, ModifierId, ModifierKind, PlaneswalkerDef, PlaneswalkerId, RunData,
    RunDataError, Who, validate, validate_modifier, validate_modifier_tokens,
};
pub use flow::{apply_duel_action, apply_run_action, current_enemy, new_run, run_legal_actions};
pub use lands::{basic_land, basic_lands, color_weights, main_color};
pub use loadout::{Loadout, duel_config, library_with_lands, starting_deck};
pub use map::generate_map;
pub use state::{
    DuelKind, DuelOutcome, ENEMY, GoldReason, MapNode, NodeId, NodeKind, PLAYER, Phase, RunAction,
    RunConfig, RunError, RunEvent, RunMap, RunState, ShopItem, Ware,
};
pub use view::{RunContent, RunView, run_content, run_view};
