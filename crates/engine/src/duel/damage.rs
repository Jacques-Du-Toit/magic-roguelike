//! Dealing damage, changing life totals, and state-based actions.

use super::action::{GameEvent, Target};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, GameResult, WinReason, Zone};
use crate::cards::Keyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifeCause {
    Fatigue,
    Other,
}

pub(crate) fn change_life(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    change: i32,
    cause: LifeCause,
) {
    if change == 0 {
        return;
    }
    let player_state = state.player_mut(player);
    let was_alive = player_state.life > 0;
    player_state.life = player_state.life.saturating_add(change);
    if player_state.life > 0 {
        player_state.lost_to_fatigue = false;
    } else if was_alive {
        player_state.lost_to_fatigue = cause == LifeCause::Fatigue;
    }
    events.push(GameEvent::LifeChanged {
        player,
        change,
        life: player_state.life,
    });
}

/// `source` deals `amount` damage to a player or a creature. Deathtouch and
/// lifelink on the source apply. Damage to something that is no longer a
/// creature on the battlefield is not dealt.
pub(crate) fn deal_damage(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    source: ObjectId,
    target: Target,
    amount: i32,
) -> Result<(), InternalError> {
    if amount <= 0 {
        return Ok(());
    }
    let deathtouch = state.has_keyword(source, Keyword::Deathtouch)?;
    let lifelink = state.has_keyword(source, Keyword::Lifelink)?;
    let controller = state.object(source)?.owner;
    match target {
        Target::Player { player } => {
            events.push(GameEvent::DamageDealt {
                source,
                target,
                amount,
            });
            change_life(
                state,
                events,
                player,
                amount.saturating_neg(),
                LifeCause::Other,
            );
        }
        Target::Object { object } => {
            if !state.is_creature(object)? {
                return Ok(());
            }
            let creature = state.object_mut(object)?;
            creature.damage = creature.damage.saturating_add(amount);
            creature.deathtouch_damage |= deathtouch;
            events.push(GameEvent::DamageDealt {
                source,
                target,
                amount,
            });
        }
    }
    if lifelink {
        change_life(state, events, controller, amount, LifeCause::Other);
    }
    Ok(())
}

/// Moves a creature from the battlefield to its owner's graveyard.
pub(crate) fn creature_dies(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    creature: ObjectId,
) -> Result<(), InternalError> {
    let player = state.object(creature)?.owner;
    let card = state.move_object(creature, Zone::Graveyard)?;
    events.push(GameEvent::CreatureDied {
        player,
        permanent: creature,
        card: state.card_ref(card)?,
    });
    Ok(())
}

/// Checks state-based actions until nothing more happens:
///
/// - a creature with toughness 0 or less, lethal damage, or damage from a
///   source with deathtouch goes to the graveyard;
/// - a player at 0 life or less loses, and if both are, the game is a draw.
pub(crate) fn check_state_based_actions(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    while state.result.is_none() {
        let mut dying = Vec::new();
        for &id in &state.battlefield {
            if !state.is_creature(id)? {
                continue;
            }
            let toughness = state.toughness(id)?;
            let creature = state.object(id)?;
            if toughness <= 0 || creature.damage >= toughness || creature.deathtouch_damage {
                dying.push(id);
            }
        }
        for &id in &dying {
            creature_dies(state, events, id)?;
        }

        let lost = |player: PlayerId| state.player(player).life <= 0;
        let result = match (lost(PlayerId::P1), lost(PlayerId::P2)) {
            (false, false) => None,
            (true, true) => Some(GameResult::Draw),
            (true, false) => Some(win(state, PlayerId::P2)),
            (false, true) => Some(win(state, PlayerId::P1)),
        };
        if let Some(result) = result {
            state.result = Some(result);
            events.push(GameEvent::GameOver { result });
        }
        if dying.is_empty() {
            break;
        }
    }
    Ok(())
}

fn win(state: &DuelState, winner: PlayerId) -> GameResult {
    let reason = if state.player(winner.opponent()).lost_to_fatigue {
        WinReason::Fatigue
    } else {
        WinReason::LifeZero
    };
    GameResult::Win {
        player: winner,
        reason,
    }
}

/// The cleanup step: damage wears off and "until end of turn" effects end,
/// at the same time.
pub(crate) fn end_of_turn_cleanup(state: &mut DuelState) {
    for object in state.objects.values_mut() {
        object.damage = 0;
        object.deathtouch_damage = false;
        object.power_bonus = 0;
        object.toughness_bonus = 0;
    }
}
