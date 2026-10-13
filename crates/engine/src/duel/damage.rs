//! Dealing damage, changing life totals, and state-based actions.

use super::action::{GameEvent, Target};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::stack::fits_target_spec;
use super::state::{DuelState, GameResult, WinReason, Zone};
use super::triggers::{Happening, fire};
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
) -> Result<(), InternalError> {
    if change == 0 {
        return Ok(());
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
    if change > 0 {
        let gained = &mut state.player_mut(player).life_gained_this_turn;
        *gained = gained.saturating_add(change.unsigned_abs());
        fire(state, Happening::GainedLife(player))?;
    }
    Ok(())
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
    // The source may be gone by now (a creature whose "when this dies"
    // ability deals the damage), or be an emblem. Then it has no keywords.
    let (deathtouch, lifelink, controller, is_permanent) = match state.objects.get(&source) {
        Some(object) => (
            state.has_keyword(source, Keyword::Deathtouch)?,
            state.has_keyword(source, Keyword::Lifelink)?,
            Some(object.owner),
            object.zone == Zone::Battlefield,
        ),
        None => (false, false, None, false),
    };
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
            )?;
            if is_permanent && controller.is_some_and(|controller| controller != player) {
                fire(state, Happening::DealtDamageToOpponent(source))?;
            }
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
    if let (true, Some(controller)) = (lifelink, controller) {
        change_life(state, events, controller, amount, LifeCause::Other)?;
    }
    Ok(())
}

/// Moves a creature from the battlefield to its owner's graveyard.
pub(crate) fn creature_dies(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    creature: ObjectId,
) -> Result<(), InternalError> {
    creatures_die(state, events, &[creature])
}

/// Moves creatures that die at the same time to their owners' graveyards.
/// Abilities that trigger on a creature dying see all of them go, their own
/// permanent included, before any has left.
pub(crate) fn creatures_die(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    creatures: &[ObjectId],
) -> Result<(), InternalError> {
    for &creature in creatures {
        fire(state, Happening::Dies(creature))?;
    }
    for &creature in creatures {
        let player = state.object(creature)?.owner;
        let card = state.move_object(creature, Zone::Graveyard)?;
        events.push(GameEvent::CreatureDied {
            player,
            permanent: creature,
            card: state.card_ref(card)?,
        });
        note_commander_return(state, events, card)?;
    }
    Ok(())
}

/// Reports a commander that was on its way to a graveyard and went to its
/// owner's command zone instead (see `DuelState::move_object`). `card` is
/// the object the move produced.
pub(crate) fn note_commander_return(
    state: &DuelState,
    events: &mut Vec<GameEvent>,
    card: ObjectId,
) -> Result<(), InternalError> {
    let object = state.object(card)?;
    if object.zone == Zone::Command {
        events.push(GameEvent::CommanderReturned {
            player: object.owner,
            card: state.card_ref(card)?,
        });
    }
    Ok(())
}

/// Destroys a creature: it dies unless it is indestructible.
pub(crate) fn destroy_creature(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    creature: ObjectId,
) -> Result<(), InternalError> {
    if state.has_keyword(creature, Keyword::Indestructible)? {
        return Ok(());
    }
    creature_dies(state, events, creature)
}

/// Checks state-based actions until nothing more happens:
///
/// - a creature with toughness 0 or less goes to the graveyard, and so does
///   one with lethal damage or damage from a source with deathtouch unless
///   it is indestructible;
/// - an Aura that isn't attached to something it can enchant goes to the
///   graveyard;
/// - a token that has left the battlefield ceases to exist;
/// - a player at 0 life or less loses, and if both are, the game is a draw.
pub(crate) fn check_state_based_actions(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    while state.result.is_none() {
        state.remove_stray_tokens();
        let mut dying = Vec::new();
        let mut falling_off = Vec::new();
        for &id in &state.battlefield {
            let def = state.def(id)?;
            let permanent = state.object(id)?;
            if def.is_creature() {
                let toughness = state.toughness(id)?;
                let destroyed = (permanent.damage >= toughness || permanent.deathtouch_damage)
                    && !state.has_keyword(id, Keyword::Indestructible)?;
                if toughness <= 0 || destroyed {
                    dying.push(id);
                }
            } else if let Some(enchant) = def.enchant {
                let attached_legally = match permanent.attached_to {
                    Some(object) => {
                        let target = Target::Object { object };
                        // "With another Aura attached to it": another than
                        // this one.
                        let alone = matches!(enchant, crate::cards::TargetSpec::Creature(filter) if filter.enchanted)
                            && state.auras_on(object)?.iter().all(|aura| *aura == id);
                        !alone && fits_target_spec(state, permanent.owner, enchant, target)?
                    }
                    None => false,
                };
                if !attached_legally {
                    falling_off.push(id);
                }
            }
        }
        creatures_die(state, events, &dying)?;
        for &id in &falling_off {
            let player = state.object(id)?.owner;
            let card = state.move_object(id, Zone::Graveyard)?;
            events.push(GameEvent::PermanentPutIntoGraveyard {
                player,
                permanent: id,
                card: state.card_ref(card)?,
            });
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
        if dying.is_empty() && falling_off.is_empty() {
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
        object.granted_keywords.clear();
    }
}
