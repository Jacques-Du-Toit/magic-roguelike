//! The run: the roguelike layer around the duel. See
//! `docs/ARCHITECTURE.md` §6.

mod defs;
mod lands;
mod loadout;

pub use defs::{
    EnemyDef, EnemyId, EnemyTier, ModifierDef, ModifierEffect, ModifierId, ModifierKind,
    PlaneswalkerDef, PlaneswalkerId, RunData, RunDataError, Who, validate, validate_modifier,
};
pub use lands::{basic_land, basic_lands, color_weights, main_color};
pub use loadout::{Loadout, duel_config, library_with_lands, starting_deck};
