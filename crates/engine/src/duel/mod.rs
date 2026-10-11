//! The duel: state, actions and rules. See `docs/ARCHITECTURE.md` §2 and §5.

mod action;
mod config;
mod error;
mod ids;
mod mulligan;
mod rng;
mod setup;
mod state;
#[cfg(test)]
mod test_support;

pub use action::{Action, CardFace, EventCard, GameEvent};
pub use config::{CardInstance, CardUpgrade, DuelConfig, PlayerConfig, RulesConfig};
pub use error::{ActionError, ConfigError, InternalError};
pub use ids::{CardId, ObjectId, PlayerId};
pub use rng::DuelRng;
pub use setup::new_duel;
pub use state::{Decision, DuelState, GameObject, PlayerState, Zone};

/// Every action `player` may take right now. Empty unless the game is
/// waiting on that player.
pub fn legal_actions(state: &DuelState, player: PlayerId) -> Vec<Action> {
    match state.decision {
        Some(Decision::Mulligan { player: decider }) if decider == player => {
            mulligan::legal_mulligan_actions(state, player)
        }
        Some(Decision::BottomCards {
            player: decider, ..
        }) if decider == player => mulligan::legal_bottom_actions(state, player),
        _ => Vec::new(),
    }
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
    if !legal_actions(state, player).contains(&action) {
        return Err(ActionError::IllegalAction { player, action });
    }
    // Work on a copy, so an engine bug part-way through can't leave the
    // caller with a half-updated state.
    let mut next = state.clone();
    let events = perform(&mut next, player, &action)?;
    *state = next;
    Ok(events)
}

/// Carries out an action already known to be legal.
fn perform(
    state: &mut DuelState,
    player: PlayerId,
    action: &Action,
) -> Result<Vec<GameEvent>, InternalError> {
    match (*action).clone() {
        Action::Keep => mulligan::keep_hand(state, player),
        Action::Mulligan => mulligan::take_mulligan(state, player),
        Action::BottomCard { card } => match state.decision {
            Some(Decision::BottomCards { remaining, .. }) => {
                mulligan::bottom_card(state, player, card, remaining)
            }
            _ => Err(InternalError::new("bottoming a card with none owed")),
        },
    }
}
