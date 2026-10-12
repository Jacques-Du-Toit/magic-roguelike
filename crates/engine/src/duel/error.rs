//! Errors returned by the duel API.

use thiserror::Error;

use super::action::Action;
use super::ids::PlayerId;
use crate::cards::{CardDefId, EmblemError};
use crate::data::DataError;

/// The engine found its own state inconsistent. Always a bug in the engine,
/// never something a caller did.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("engine bug: {0}")]
pub struct InternalError(pub String);

impl InternalError {
    pub(crate) fn new(message: impl Into<String>) -> InternalError {
        InternalError(message.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Data(#[from] DataError),
    #[error("{player:?}'s library contains the unknown card `{card}`")]
    UnknownCard { player: PlayerId, card: CardDefId },
    #[error("{player:?}'s emblem `{name}`: {source}")]
    Emblem {
        player: PlayerId,
        name: String,
        source: EmblemError,
    },
    #[error("{player:?}'s commander `{card}` is not a legendary creature")]
    Commander { player: PlayerId, card: CardDefId },
    #[error("{0:?}'s starting life must be above zero")]
    StartingLife(PlayerId),
    #[error("smoothing_candidates must be at least 1")]
    SmoothingCandidates,
    #[error(transparent)]
    Internal(#[from] InternalError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionError {
    #[error("the game is not waiting on {0:?}")]
    NotWaitingOn(PlayerId),
    #[error("{action:?} is not a legal action for {player:?} right now")]
    IllegalAction { player: PlayerId, action: Action },
    #[error(transparent)]
    Internal(#[from] InternalError),
}
