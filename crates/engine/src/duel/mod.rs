//! The duel: state, actions and rules. See `docs/ARCHITECTURE.md` §2 and §5.

mod action;
mod config;
mod error;
mod ids;
mod mana;
mod mulligan;
mod rng;
mod setup;
mod stack;
mod state;
#[cfg(test)]
mod test_support;
mod turn;

pub use action::{Action, CardFace, CardRef, GameEvent, Target};
pub use config::{CardInstance, CardUpgrade, DuelConfig, PlayerConfig, RulesConfig};
pub use error::{ActionError, ConfigError, InternalError};
pub use ids::{CardId, ObjectId, PlayerId};
pub use mana::ManaPool;
pub use rng::DuelRng;
pub use setup::new_duel;
pub use state::{Decision, DuelState, GameObject, GameResult, PlayerState, Step, WinReason, Zone};

/// Every action `player` may take right now. Empty unless the game is
/// waiting on that player.
pub fn legal_actions(state: &DuelState, player: PlayerId) -> Vec<Action> {
    // An internal error here means the state is corrupt; no action is legal.
    try_legal_actions(state, player).unwrap_or_default()
}

fn try_legal_actions(state: &DuelState, player: PlayerId) -> Result<Vec<Action>, InternalError> {
    let Some(decision) = state.decision else {
        return Ok(Vec::new());
    };
    if decision.player() != player {
        return Ok(Vec::new());
    }
    Ok(match decision {
        Decision::Mulligan { .. } => mulligan::legal_mulligan_actions(state, player),
        Decision::BottomCards { .. } => mulligan::legal_bottom_actions(state, player),
        Decision::Priority { .. } => turn::legal_priority_actions(state, player)?,
        Decision::Discard { .. } => turn::legal_discard_actions(state, player),
    })
}

/// Applies one of `player`'s legal actions and returns what happened,
/// unredacted. On an error the state is left exactly as it was.
pub fn apply_action(
    state: &mut DuelState,
    player: PlayerId,
    action: Action,
) -> Result<Vec<GameEvent>, ActionError> {
    if state.waiting_on() != Some(player) {
        return Err(ActionError::NotWaitingOn(player));
    }
    if !try_legal_actions(state, player)?.contains(&action) {
        return Err(ActionError::IllegalAction { player, action });
    }
    // Work on a copy, so an engine bug part-way through can't leave the
    // caller with a half-updated state.
    let mut next = state.clone();
    let events = perform(&mut next, player, &action)?;
    *state = next;
    Ok(events)
}

/// The outcome of the duel, once it has ended.
pub fn result(state: &DuelState) -> Option<GameResult> {
    state.result
}

/// Carries out an action already known to be legal.
fn perform(
    state: &mut DuelState,
    player: PlayerId,
    action: &Action,
) -> Result<Vec<GameEvent>, InternalError> {
    let mut events = Vec::new();
    match (action, state.decision) {
        (Action::Keep, _) => events.extend(mulligan::keep_hand(state, player)?),
        (Action::Mulligan, _) => events.extend(mulligan::take_mulligan(state, player)?),
        (Action::BottomCard { card }, Some(Decision::BottomCards { remaining, .. })) => {
            events.extend(mulligan::bottom_card(state, player, *card, remaining)?);
        }
        (Action::PassPriority, _) => turn::pass_priority(state, &mut events, player)?,
        (Action::PlayLand { card }, _) => turn::play_land(state, &mut events, player, *card)?,
        (Action::Discard { card }, Some(Decision::Discard { remaining, .. })) => {
            turn::discard(state, &mut events, player, *card, remaining)?;
        }
        (Action::CastSpell { card, targets }, _) => {
            stack::cast_spell(state, &mut events, player, *card, targets)?;
        }
        (Action::BottomCard { .. } | Action::Discard { .. }, _) => {
            return Err(InternalError::new(format!(
                "{action:?} does not answer {:?}",
                state.decision
            )));
        }
    }
    if mulligan::mulligans_are_over(state) && state.turn == 0 {
        turn::begin_game(state, &mut events)?;
    }
    Ok(events)
}
