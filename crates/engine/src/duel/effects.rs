//! Carrying out effect primitives.

use super::action::{GameEvent, Target};
use super::damage::{LifeCause, change_life, creature_dies, deal_damage};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::state::{DuelState, StackItemKind, Zone};
use super::turn::draw_card;
use crate::cards::{Effect, PlayerRef};

/// Carries out one effect of a resolving spell or ability.
///
/// `source` is the spell or the permanent the effect comes from, and
/// `target` is the effect's target, already known to be legal, if it has
/// one.
pub(crate) fn apply_effect(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    effect: &Effect,
    controller: PlayerId,
    source: ObjectId,
    target: Option<Target>,
) -> Result<(), InternalError> {
    match *effect {
        Effect::DealDamage { amount, .. } => {
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            deal_damage(state, events, source, needed(effect, target)?, amount)
        }
        Effect::Destroy { .. } => {
            let permanent = object(effect, target)?;
            if state.is_creature(permanent)? {
                creature_dies(state, events, permanent)?;
            }
            Ok(())
        }
        Effect::CounterSpell { .. } => counter_spell(state, events, object(effect, target)?),
        Effect::DrawCards { player, amount } => {
            let player = resolve_player(player, controller);
            for _ in 0..amount {
                draw_card(state, events, player)?;
            }
            Ok(())
        }
        Effect::GainLife { player, amount } => {
            let player = resolve_player(player, controller);
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            change_life(state, events, player, amount, LifeCause::Other);
            Ok(())
        }
        Effect::LoseLife { player, amount } => {
            let player = resolve_player(player, controller);
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            change_life(
                state,
                events,
                player,
                amount.saturating_neg(),
                LifeCause::Other,
            );
            Ok(())
        }
        Effect::ModifyPtUntilEot {
            power, toughness, ..
        } => {
            let permanent = object(effect, target)?;
            let creature = state.object_mut(permanent)?;
            creature.power_bonus += power;
            creature.toughness_bonus += toughness;
            events.push(GameEvent::PowerToughnessChanged {
                permanent,
                power,
                toughness,
            });
            Ok(())
        }
    }
}

fn resolve_player(player: PlayerRef, controller: PlayerId) -> PlayerId {
    match player {
        PlayerRef::Controller => controller,
        PlayerRef::Opponent => controller.opponent(),
    }
}

fn needed(effect: &Effect, target: Option<Target>) -> Result<Target, InternalError> {
    target.ok_or_else(|| InternalError::new(format!("`{}` has no target", effect.name())))
}

fn object(effect: &Effect, target: Option<Target>) -> Result<ObjectId, InternalError> {
    match needed(effect, target)? {
        Target::Object { object } => Ok(object),
        Target::Player { .. } => Err(InternalError::new(format!(
            "`{}` is aimed at a player",
            effect.name()
        ))),
    }
}

/// Counters a spell: it leaves the stack for its owner's graveyard.
fn counter_spell(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    spell: ObjectId,
) -> Result<(), InternalError> {
    let player = state
        .stack
        .iter()
        .find(|item| item.id == spell && item.kind == StackItemKind::Spell)
        .map(|item| item.controller)
        .ok_or_else(|| InternalError::new(format!("{spell:?} is not a spell on the stack")))?;
    let card = state.move_object(spell, Zone::Graveyard)?;
    events.push(GameEvent::SpellCountered {
        player,
        card: state.card_ref(card)?,
    });
    Ok(())
}
