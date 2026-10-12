//! The duel: state, actions and rules. See `docs/ARCHITECTURE.md` §2 and §5.

mod abilities;
#[cfg(test)]
mod ability_tests;
mod action;
#[cfg(test)]
mod aura_tests;
#[cfg(test)]
mod choice_tests;
mod combat;
#[cfg(test)]
mod combat_tests;
#[cfg(test)]
mod commander_tests;
mod config;
#[cfg(test)]
mod counter_tests;
mod damage;
mod effects;
#[cfg(test)]
mod emblem_tests;
mod error;
#[cfg(test)]
mod graveyard_tests;
mod ids;
#[cfg(test)]
mod keyword_tests;
#[cfg(test)]
mod lifegain_tests;
mod mana;
mod mulligan;
mod rng;
mod setup;
#[cfg(test)]
mod spell_tests;
mod stack;
mod state;
#[cfg(test)]
mod target_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod token_tests;
#[cfg(test)]
mod trigger_tests;
mod triggers;
mod turn;
mod view;
#[cfg(test)]
mod view_tests;

pub use action::{Action, BlockDeclared, CardFace, CardRef, GameEvent, Target};
pub use config::{CardInstance, CardUpgrade, DuelConfig, PlayerConfig, RulesConfig};
pub use error::{ActionError, ConfigError, InternalError};
pub use ids::{CardId, ObjectId, PlayerId};
pub use mana::ManaPool;
pub use rng::DuelRng;
pub use setup::new_duel;
pub use state::{
    AbilityRef, Attacker, Combat, DamageAssignment, DamageShare, Decision, DuelState, EffectStep,
    Emblem, GameObject, GameResult, PendingTrigger, PlayerState, Resolving, StackItem,
    StackItemKind, Step, TriggeredAbility, WinReason, Zone,
};
pub(crate) use turn::scry_card;
pub use view::{
    CommanderView, EmblemView, PermanentView, PlayerSummary, PlayerView, StackItemView,
    StackItemViewKind, has_meaningful_action, player_view, redact_events, should_auto_pass,
};

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
        Decision::DeclareAttackers { .. } => combat::legal_attack_actions(state)?,
        Decision::DeclareBlockers { .. } => combat::legal_block_actions(state)?,
        Decision::AssignCombatDamage { .. } => combat::legal_assignment_actions(state)?,
        Decision::ChooseTargets { .. } => stack::legal_target_choices(state)?,
        Decision::Scry { surveil: false, .. } => vec![Action::ScryKeep, Action::ScryBottom],
        Decision::Scry { surveil: true, .. } => vec![Action::ScryKeep, Action::ScryGraveyard],
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

/// Applies an action the caller already knows to be legal (one taken from
/// `legal_actions`), straight onto `state`. For the AI's search, which
/// plays thousands of imagined games on copies it throws away: it skips the
/// legality check and the safety copy that `apply_action` makes. After an
/// error the state is not to be used.
pub(crate) fn apply_trusted(
    state: &mut DuelState,
    player: PlayerId,
    action: &Action,
) -> Result<Vec<GameEvent>, InternalError> {
    perform(state, player, action)
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
            turn::open_priority_for(state, &mut events, player)?;
        }
        (
            Action::ActivateAbility {
                source,
                ability,
                targets,
                sacrifice,
            },
            _,
        ) => {
            abilities::activate(
                state,
                &mut events,
                player,
                *source,
                *ability,
                targets,
                *sacrifice,
            )?;
            // Like a spell's caster, the player keeps priority.
            turn::open_priority_for(state, &mut events, player)?;
        }
        (Action::Attack { creature }, _) => combat::declare_attacker(state, *creature)?,
        (Action::Block { blocker, attacker }, _) => {
            combat::declare_blocker(state, *blocker, *attacker)?;
        }
        (Action::RemoveAttacker { creature }, _) => combat::remove_attacker(state, *creature)?,
        (Action::RemoveBlocker { blocker }, _) => combat::remove_blocker(state, *blocker)?,
        (Action::ResetDeclaration, Some(Decision::DeclareAttackers { .. })) => {
            combat::reset_attackers(state)?;
        }
        (Action::ResetDeclaration, Some(Decision::DeclareBlockers { .. })) => {
            combat::reset_blockers(state)?;
        }
        (Action::ConfirmAttackers, _) => turn::confirm_attackers(state, &mut events)?,
        (Action::ConfirmBlockers, _) => turn::confirm_blockers(state, &mut events)?,
        (Action::AssignDamage { target }, _) => combat::change_assignment(state, *target, 1)?,
        (Action::UnassignDamage { target }, _) => combat::change_assignment(state, *target, -1)?,
        (Action::ConfirmDamage, _) => turn::confirm_damage(state, &mut events)?,
        (Action::ChooseTargets { targets }, _) => {
            turn::choose_trigger_targets(state, &mut events, targets)?;
        }
        (
            Action::ScryKeep | Action::ScryBottom | Action::ScryGraveyard,
            Some(Decision::Scry {
                kept, remaining, ..
            }),
        ) => {
            let to = match action {
                Action::ScryBottom => turn::ScryTo::Bottom,
                Action::ScryGraveyard => turn::ScryTo::Graveyard,
                _ => turn::ScryTo::Top,
            };
            turn::scry(state, &mut events, player, kept, remaining, to)?;
        }
        (
            Action::BottomCard { .. }
            | Action::Discard { .. }
            | Action::ResetDeclaration
            | Action::ScryKeep
            | Action::ScryBottom
            | Action::ScryGraveyard,
            _,
        ) => {
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
