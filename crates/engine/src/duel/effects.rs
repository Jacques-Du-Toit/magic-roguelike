//! Carrying out effect primitives.

use super::action::{GameEvent, Target};
use super::damage::{LifeCause, change_life, deal_damage, destroy_creature, note_commander_return};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::stack::permanent_entered;
use super::state::{Decision, DuelState, StackItemKind, Zone};
use super::turn::{discard_card, draw_card, mill_card};
use crate::cards::{Count, Effect, PlayerRef, WhichCreatures};

/// Whether an effect was carried out in full, or has stopped for a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Applied {
    Done,
    /// `state.decision` now asks a player for something. The resolution
    /// carries on once they have answered.
    Waiting,
}

/// Carries out one effect of a resolving spell or ability.
///
/// `source` is the spell or the permanent the effect comes from, and
/// `targets` are the effect's targets, already known to be legal, in the
/// order `Effect::targets` lists them.
pub(crate) fn apply_effect(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    effect: &Effect,
    controller: PlayerId,
    source: ObjectId,
    targets: &[Target],
) -> Result<Applied, InternalError> {
    match *effect {
        Effect::AddCounters { amount, .. } => {
            let permanent = object(effect, targets, 0)?;
            put_counters(state, events, controller, permanent, amount)?;
        }
        Effect::AddCountersToYourCreatures { amount, which } => {
            for creature in state.creatures_of(controller)? {
                let reached = match which {
                    WhichCreatures::All => true,
                    WhichCreatures::Others => creature != source,
                    WhichCreatures::WithCounter => state.object(creature)?.plus_counters > 0,
                };
                if reached {
                    put_counters(state, events, controller, creature, amount)?;
                }
            }
        }
        Effect::DoubleCounters { .. } => {
            let permanent = object(effect, targets, 0)?;
            if state.is_on_battlefield(permanent) {
                let counters = state.object(permanent)?.plus_counters;
                put_counters(state, events, controller, permanent, counters)?;
            }
        }
        Effect::DoubleCountersOnYourCreatures {} => {
            for creature in state.creatures_of(controller)? {
                let counters = state.object(creature)?.plus_counters;
                put_counters(state, events, controller, creature, counters)?;
            }
        }
        Effect::CreateToken {
            ref token,
            count,
            for_each,
        } => {
            let count = count.saturating_mul(times(state, controller, for_each)?);
            for _ in 0..count {
                let id = state.create_card(controller, token.clone(), Zone::Battlefield);
                events.push(GameEvent::TokenCreated {
                    player: controller,
                    card: state.card_ref(id)?,
                });
                permanent_entered(state, id)?;
            }
        }
        Effect::DealDamage { amount, .. } => {
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            deal_damage(state, events, source, needed(effect, targets, 0)?, amount)?;
        }
        Effect::DealPowerDamage { .. } => {
            let biter = object(effect, targets, 0)?;
            if state.is_creature(biter)? {
                let amount = state.power(biter)?;
                deal_damage(state, events, biter, needed(effect, targets, 1)?, amount)?;
            }
        }
        Effect::Destroy { .. } => {
            let permanent = object(effect, targets, 0)?;
            if state.is_creature(permanent)? {
                destroy_creature(state, events, permanent)?;
            }
        }
        Effect::CounterSpell { .. } => counter_spell(state, events, object(effect, targets, 0)?)?,
        Effect::Discard {
            player,
            amount,
            random,
        } => {
            let player = resolve_player(effect, player, controller, targets)?;
            let hand = state.player(player).hand.clone();
            let amount = usize::try_from(amount).unwrap_or(usize::MAX);
            if hand.len() <= amount {
                // No choice to make: the whole hand goes.
                for card in hand {
                    discard_card(state, events, player, card)?;
                }
            } else if random {
                for _ in 0..amount {
                    let cards = state.player(player).hand.len();
                    let pick = state.rng.below(u32::try_from(cards).unwrap_or(u32::MAX));
                    let card = state.player(player).hand[pick as usize];
                    discard_card(state, events, player, card)?;
                }
            } else {
                state.decision = Some(Decision::Discard {
                    player,
                    remaining: u8::try_from(amount).unwrap_or(u8::MAX),
                });
                return Ok(Applied::Waiting);
            }
        }
        Effect::DrawCards {
            player,
            amount,
            for_each,
        } => {
            let amount = amount.saturating_mul(times(state, controller, for_each)?);
            let player = resolve_player(effect, player, controller, targets)?;
            for _ in 0..amount {
                draw_card(state, events, player)?;
            }
        }
        Effect::GainLife { player, amount } => {
            let player = resolve_player(effect, player, controller, targets)?;
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            change_life(state, events, player, amount, LifeCause::Other)?;
        }
        Effect::GrantKeywordUntilEot { keyword, .. } => {
            let permanent = object(effect, targets, 0)?;
            if state.is_creature(permanent)? {
                let granted = &mut state.object_mut(permanent)?.granted_keywords;
                if !granted.contains(&keyword) {
                    granted.push(keyword);
                }
                events.push(GameEvent::KeywordGranted { permanent, keyword });
            }
        }
        Effect::LoseLife { player, amount } => {
            let player = resolve_player(effect, player, controller, targets)?;
            let amount = i32::try_from(amount).unwrap_or(i32::MAX);
            change_life(
                state,
                events,
                player,
                amount.saturating_neg(),
                LifeCause::Other,
            )?;
        }
        Effect::ModifyPtUntilEot {
            power,
            toughness,
            for_each,
            ..
        } => {
            let permanent = object(effect, targets, 0)?;
            let times = i32::try_from(times(state, controller, for_each)?).unwrap_or(i32::MAX);
            let (power, toughness) = (power.saturating_mul(times), toughness.saturating_mul(times));
            let creature = state.object_mut(permanent)?;
            creature.power_bonus += power;
            creature.toughness_bonus += toughness;
            events.push(GameEvent::PowerToughnessChanged {
                permanent,
                power,
                toughness,
            });
        }
        Effect::Mill { player, amount } => {
            let player = resolve_player(effect, player, controller, targets)?;
            for _ in 0..amount {
                let Some(&top) = state.player(player).library.last() else {
                    break;
                };
                mill_card(state, events, player, top)?;
            }
        }
        Effect::ReturnToHand { .. } => {
            let permanent = object(effect, targets, 0)?;
            if state.objects.get(&permanent).map(|card| card.zone) == Some(Zone::Graveyard) {
                let player = state.object(permanent)?.owner;
                let card = state.move_object(permanent, Zone::Hand)?;
                events.push(GameEvent::CardReturnedToHand {
                    player,
                    card: state.card_ref(card)?,
                });
            } else if state.is_on_battlefield(permanent) {
                let player = state.object(permanent)?.owner;
                let card = state.move_object(permanent, Zone::Hand)?;
                events.push(GameEvent::PermanentReturnedToHand {
                    player,
                    permanent,
                    card: state.card_ref(card)?,
                });
            }
        }
        Effect::Scry { amount } => {
            let library = state.player(controller).library.len();
            let amount = usize::try_from(amount).unwrap_or(usize::MAX).min(library);
            if amount > 0 {
                state.decision = Some(Decision::Scry {
                    player: controller,
                    kept: 0,
                    remaining: u8::try_from(amount).unwrap_or(u8::MAX),
                    surveil: false,
                });
                return Ok(Applied::Waiting);
            }
        }
        Effect::Surveil { amount } => {
            let library = state.player(controller).library.len();
            let amount = usize::try_from(amount).unwrap_or(usize::MAX).min(library);
            if amount > 0 {
                state.decision = Some(Decision::Scry {
                    player: controller,
                    kept: 0,
                    remaining: u8::try_from(amount).unwrap_or(u8::MAX),
                    surveil: true,
                });
                return Ok(Applied::Waiting);
            }
        }
        Effect::YourCreaturesGetUntilEot {
            power,
            toughness,
            keyword,
        } => {
            for permanent in state.creatures_of(controller)? {
                let creature = state.object_mut(permanent)?;
                if power != 0 || toughness != 0 {
                    creature.power_bonus += power;
                    creature.toughness_bonus += toughness;
                    events.push(GameEvent::PowerToughnessChanged {
                        permanent,
                        power,
                        toughness,
                    });
                }
                if let Some(keyword) = keyword {
                    if !creature.granted_keywords.contains(&keyword) {
                        creature.granted_keywords.push(keyword);
                    }
                    events.push(GameEvent::KeywordGranted { permanent, keyword });
                }
            }
        }
        Effect::Tap {
            skip_next_untap, ..
        } => {
            let permanent = object(effect, targets, 0)?;
            if state.is_on_battlefield(permanent) {
                let tapped = state.object_mut(permanent)?;
                let was_tapped = tapped.tapped;
                tapped.tapped = true;
                tapped.skip_untap |= skip_next_untap;
                if !was_tapped {
                    events.push(GameEvent::PermanentTapped { permanent });
                }
            }
        }
    }
    Ok(Applied::Done)
}

/// How many times an effect with `for_each` happens: once without it.
fn times(
    state: &DuelState,
    controller: PlayerId,
    for_each: Option<Count>,
) -> Result<u32, InternalError> {
    let creatures = |with_counter: bool| -> Result<u32, InternalError> {
        let mut count = 0;
        for creature in state.creatures_of(controller)? {
            if !with_counter || state.object(creature)?.plus_counters > 0 {
                count += 1;
            }
        }
        Ok(count)
    };
    Ok(match for_each {
        None => 1,
        Some(Count::CreatureYouControl) => creatures(false)?,
        Some(Count::CreatureYouControlWithCounter) => creatures(true)?,
        Some(Count::CounterPutThisTurn) => state.player(controller).counters_put_this_turn,
    })
}

/// Puts +1/+1 counters on a creature for `controller`. Every effect that
/// adds counters comes through here.
fn put_counters(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    controller: PlayerId,
    permanent: ObjectId,
    amount: u32,
) -> Result<(), InternalError> {
    if amount == 0 || !state.is_on_battlefield(permanent) || !state.is_creature(permanent)? {
        return Ok(());
    }
    let creature = state.object_mut(permanent)?;
    creature.plus_counters = creature.plus_counters.saturating_add(amount);
    let (counters, owner) = (creature.plus_counters, creature.owner);
    events.push(GameEvent::CountersAdded {
        permanent,
        amount,
        counters,
    });
    if owner == controller {
        let put = &mut state.players[controller.index()].counters_put_this_turn;
        *put = put.saturating_add(amount);
    }
    Ok(())
}

/// The player an effect names: the controller, their opponent, or the
/// player it targets (its first target).
fn resolve_player(
    effect: &Effect,
    player: PlayerRef,
    controller: PlayerId,
    targets: &[Target],
) -> Result<PlayerId, InternalError> {
    match player {
        PlayerRef::Controller => Ok(controller),
        PlayerRef::Opponent => Ok(controller.opponent()),
        PlayerRef::TargetPlayer | PlayerRef::TargetOpponent => match needed(effect, targets, 0)? {
            Target::Player { player } => Ok(player),
            Target::Object { .. } => Err(InternalError::new(format!(
                "`{}` is aimed at an object, not a player",
                effect.name()
            ))),
        },
    }
}

fn needed(effect: &Effect, targets: &[Target], index: usize) -> Result<Target, InternalError> {
    targets
        .get(index)
        .copied()
        .ok_or_else(|| InternalError::new(format!("`{}` is missing target {index}", effect.name())))
}

fn object(effect: &Effect, targets: &[Target], index: usize) -> Result<ObjectId, InternalError> {
    match needed(effect, targets, index)? {
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
    note_commander_return(state, events, card)
}
