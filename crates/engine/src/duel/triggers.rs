//! Noticing when triggered abilities trigger.
//!
//! Whatever happens in the game that an ability could be waiting for is
//! reported here as a `Happening`. Every permanent and emblem is asked
//! whether one of its abilities triggers on it, and those that do wait in
//! `DuelState::pending_triggers` until a player would next get priority.

use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::state::{AbilityRef, DuelState, PendingTrigger, TriggeredAbility};
use crate::cards::{AbilityDef, CastKind, Effect, Keyword, Stat, StatOf, Trigger, TriggerSubject};

/// Something abilities can trigger on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Happening {
    /// A permanent has entered the battlefield.
    Entered(ObjectId),
    /// A creature is about to die. It is still on the battlefield, so that
    /// its own abilities and those of creatures dying with it see it go.
    Dies(ObjectId),
    /// A creature has been declared as an attacker.
    Attacked(ObjectId),
    /// A permanent has dealt damage to an opponent of its controller.
    DealtDamageToOpponent(ObjectId),
    GainedLife(PlayerId),
    /// A permanent is about to be sacrificed. Like a dying creature, it is
    /// still on the battlefield.
    Sacrificed(ObjectId),
    /// A player has cast the spell `spell`, now on the stack.
    CastSpell {
        player: PlayerId,
        spell: ObjectId,
    },
    /// A permanent is about to leave the battlefield.
    Left(ObjectId),
    /// A player's upkeep has begun.
    Upkeep(PlayerId),
    /// Combat has begun on a player's turn.
    BeginCombat(PlayerId),
    /// A player's end step has begun.
    EndStep(PlayerId),
}

/// What an ability is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holder {
    Permanent(ObjectId),
    Emblem,
}

/// Notes every triggered ability that triggers on `happening`.
pub(crate) fn fire(state: &mut DuelState, happening: Happening) -> Result<(), InternalError> {
    // (the ability, whether it triggers only once each turn)
    let mut triggered: Vec<(PendingTrigger, bool)> = Vec::new();
    for &id in &state.battlefield {
        let object = state.object(id)?;
        let controller = object.owner;
        for (index, ability) in state.def(id)?.abilities.iter().enumerate() {
            let AbilityDef::Triggered {
                trigger,
                effects,
                once_each_turn,
                condition,
                ..
            } = ability
            else {
                continue;
            };
            if condition
                .is_some_and(|condition| !state.condition_holds(controller, Some(id), condition))
            {
                continue;
            }
            let holder = Holder::Permanent(id);
            if let Some(subject) = triggers_on(state, trigger, happening, holder, controller)? {
                let amount = amount_now(state, effects, id, subject)?;
                let ability = AbilityRef {
                    source: id,
                    def: object.def.clone(),
                    index,
                };
                triggered.push((
                    PendingTrigger {
                        ability: TriggeredAbility::Card { ability },
                        controller,
                        subject,
                        amount,
                    },
                    *once_each_turn,
                ));
            }
        }
        // Prowess is a rule, not something each card spells out.
        if let Happening::CastSpell { player, spell } = happening
            && player == controller
            && !state.def(spell)?.is_creature()
            && state.is_creature(id)?
            && state.has_keyword(id, Keyword::Prowess)?
        {
            triggered.push((
                PendingTrigger {
                    ability: TriggeredAbility::Keyword {
                        source: id,
                        def: object.def.clone(),
                        keyword: Keyword::Prowess,
                    },
                    controller,
                    subject: None,
                    amount: None,
                },
                false,
            ));
        }
    }
    for emblem in &state.emblems {
        for (index, ability) in emblem.def.abilities.iter().enumerate() {
            let AbilityDef::Triggered {
                trigger,
                effects,
                once_each_turn,
                condition,
                ..
            } = ability
            else {
                continue;
            };
            let controller = emblem.controller;
            if condition
                .is_some_and(|condition| !state.condition_holds(controller, None, condition))
            {
                continue;
            }
            if let Some(subject) =
                triggers_on(state, trigger, happening, Holder::Emblem, controller)?
            {
                triggered.push((
                    PendingTrigger {
                        ability: TriggeredAbility::Emblem {
                            emblem: emblem.id,
                            index,
                        },
                        controller,
                        subject,
                        amount: amount_now(state, effects, emblem.id, subject)?,
                    },
                    *once_each_turn,
                ));
            }
        }
    }
    for (trigger, once_each_turn) in triggered {
        if once_each_turn {
            if state.triggered_this_turn.contains(&trigger.ability) {
                continue;
            }
            state.triggered_this_turn.push(trigger.ability.clone());
        }
        state.pending_triggers.push(trigger);
    }
    Ok(())
}

/// The power or toughness a "gain life equal to..." effect among `effects`
/// reads, as it is at this moment, if there is such an effect and the
/// creature is on the battlefield.
fn amount_now(
    state: &DuelState,
    effects: &[Effect],
    source: ObjectId,
    subject: Option<ObjectId>,
) -> Result<Option<i32>, InternalError> {
    for effect in effects {
        if let Effect::GainLifeEqualTo { stat, of } = *effect {
            let object = match of {
                StatOf::This => Some(source),
                StatOf::Triggering => subject,
            };
            return stat_of(state, stat, object);
        }
    }
    Ok(None)
}

/// A creature's power or toughness, while it is on the battlefield.
pub(crate) fn stat_of(
    state: &DuelState,
    stat: Stat,
    object: Option<ObjectId>,
) -> Result<Option<i32>, InternalError> {
    let Some(object) = object.filter(|object| state.is_on_battlefield(*object)) else {
        return Ok(None);
    };
    Ok(Some(match stat {
        Stat::Power => state.power(object)?,
        Stat::Toughness => state.toughness(object)?,
    }))
}

/// Whether an ability with `trigger`, on `holder` and controlled by
/// `controller`, triggers on `happening`. `Some` if it does, holding the
/// object that set it off when that is something other than its own
/// permanent.
fn triggers_on(
    state: &DuelState,
    trigger: &Trigger,
    happening: Happening,
    holder: Holder,
    controller: PlayerId,
) -> Result<Option<Option<ObjectId>>, InternalError> {
    let is_holder = |object: ObjectId| holder == Holder::Permanent(object);
    let own = Some(None);
    // A creature of the controller's, whichever it is.
    let creature_of_controller = |object: ObjectId| -> Result<bool, InternalError> {
        Ok(state.is_creature(object)? && state.object(object)?.owner == controller)
    };
    let by_subject = |subject: TriggerSubject, object: ObjectId| -> Result<_, InternalError> {
        Ok(match subject {
            TriggerSubject::This => is_holder(object).then_some(None),
            TriggerSubject::CreatureYouControl => {
                creature_of_controller(object)?.then_some(Some(object))
            }
            TriggerSubject::AnotherCreatureYouControl => {
                (creature_of_controller(object)? && !is_holder(object)).then_some(Some(object))
            }
            TriggerSubject::TokenYouControl => (state.def(object)?.token
                && state.object(object)?.owner == controller)
                .then_some(Some(object)),
        })
    };
    Ok(match (*trigger, happening) {
        (Trigger::EntersBattlefield { subject, min_power }, Happening::Entered(object)) => {
            let big_enough = match min_power {
                Some(min_power) => state.is_creature(object)? && state.power(object)? >= min_power,
                None => true,
            };
            if big_enough {
                by_subject(subject, object)?
            } else {
                None
            }
        }
        (Trigger::Dies { subject }, Happening::Dies(object)) => by_subject(subject, object)?,
        (Trigger::LeavesBattlefield {}, Happening::Left(object)) => {
            is_holder(object).then_some(None)
        }
        (Trigger::Attacks {}, Happening::Attacked(object))
        | (Trigger::DealsDamageToOpponent {}, Happening::DealtDamageToOpponent(object)) => {
            is_holder(object).then_some(None)
        }
        (Trigger::Sacrifice {}, Happening::Sacrificed(_)) => own,
        (Trigger::GainLife {}, Happening::GainedLife(player))
        | (Trigger::BeginningOfUpkeep {}, Happening::Upkeep(player))
        | (Trigger::BeginningOfCombat {}, Happening::BeginCombat(player)) => {
            (player == controller).then_some(None)
        }
        (Trigger::BeginningOfEndStep { each_player }, Happening::EndStep(player)) => {
            (each_player || player == controller).then_some(None)
        }
        (Trigger::CastSpell { kind }, Happening::CastSpell { player, spell }) => {
            let def = state.def(spell)?;
            let fits = match kind {
                CastKind::InstantOrSorcery => def.is_instant_or_sorcery(),
                CastKind::Noncreature => !def.is_creature(),
            };
            if player == controller && fits {
                own
            } else {
                None
            }
        }
        _ => None,
    })
}
