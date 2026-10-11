//! Combat: declaring attackers and blockers, and combat damage.

use super::action::{Action, BlockDeclared, GameEvent, Target};
use super::damage::deal_damage;
use super::error::InternalError;
use super::ids::ObjectId;
use super::state::{Attacker, Combat, DuelState};
use crate::cards::Keyword;

fn combat(state: &DuelState) -> Result<&Combat, InternalError> {
    state
        .combat
        .as_ref()
        .ok_or_else(|| InternalError::new("there is no combat"))
}

fn combat_mut(state: &mut DuelState) -> Result<&mut Combat, InternalError> {
    state
        .combat
        .as_mut()
        .ok_or_else(|| InternalError::new("there is no combat"))
}

/// The active player's creatures that could still be declared as attackers.
pub(crate) fn possible_attackers(state: &DuelState) -> Result<Vec<ObjectId>, InternalError> {
    let combat = combat(state)?;
    let mut attackers = Vec::new();
    for creature in state.creatures_of(state.active_player)? {
        let permanent = state.object(creature)?;
        let ready = !permanent.summoning_sick || state.has_keyword(creature, Keyword::Haste)?;
        if ready && !permanent.tapped && !combat.is_attacking(creature) {
            attackers.push(creature);
        }
    }
    Ok(attackers)
}

pub(crate) fn legal_attack_actions(state: &DuelState) -> Result<Vec<Action>, InternalError> {
    let mut actions: Vec<Action> = possible_attackers(state)?
        .into_iter()
        .map(|creature| Action::Attack { creature })
        .collect();
    if !combat(state)?.attackers.is_empty() {
        actions.push(Action::ResetDeclaration);
    }
    actions.push(Action::ConfirmAttackers);
    Ok(actions)
}

pub(crate) fn declare_attacker(
    state: &mut DuelState,
    creature: ObjectId,
) -> Result<(), InternalError> {
    combat_mut(state)?.attackers.push(Attacker {
        attacker: creature,
        blockers: Vec::new(),
        blocked: false,
    });
    Ok(())
}

/// Clears the attackers chosen so far.
pub(crate) fn reset_attackers(state: &mut DuelState) -> Result<(), InternalError> {
    combat_mut(state)?.attackers.clear();
    Ok(())
}

/// Clears the blocks chosen so far.
pub(crate) fn reset_blockers(state: &mut DuelState) -> Result<(), InternalError> {
    for attacker in &mut combat_mut(state)?.attackers {
        attacker.blockers.clear();
    }
    Ok(())
}

/// Makes the declared attack happen: attackers without vigilance tap.
pub(crate) fn confirm_attackers(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    let attackers: Vec<ObjectId> = combat(state)?
        .attackers
        .iter()
        .map(|a| a.attacker)
        .collect();
    if attackers.is_empty() {
        return Ok(());
    }
    events.push(GameEvent::AttackersDeclared {
        player: state.active_player,
        attackers: attackers.clone(),
    });
    for attacker in attackers {
        if !state.has_keyword(attacker, Keyword::Vigilance)? {
            state.object_mut(attacker)?.tapped = true;
            events.push(GameEvent::PermanentTapped {
                permanent: attacker,
            });
        }
    }
    Ok(())
}

/// Whether `blocker` could be declared as a blocker of `attacker` now.
fn can_block(
    state: &DuelState,
    blocker: ObjectId,
    attacker: ObjectId,
) -> Result<bool, InternalError> {
    let combat = combat(state)?;
    if !state.is_creature(attacker)? || !combat.is_attacking(attacker) {
        return Ok(false);
    }
    if state.object(blocker)?.tapped || combat.is_blocking(blocker) {
        return Ok(false);
    }
    if state.has_keyword(attacker, Keyword::Flying)? {
        let reaches = state.has_keyword(blocker, Keyword::Flying)?
            || state.has_keyword(blocker, Keyword::Reach)?;
        if !reaches {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every block the defending player could add to their declaration.
fn possible_blocks(state: &DuelState) -> Result<Vec<(ObjectId, ObjectId)>, InternalError> {
    let attackers: Vec<ObjectId> = combat(state)?
        .attackers
        .iter()
        .map(|a| a.attacker)
        .collect();
    let mut blocks = Vec::new();
    for blocker in state.creatures_of(state.active_player.opponent())? {
        for &attacker in &attackers {
            if can_block(state, blocker, attacker)? {
                blocks.push((blocker, attacker));
            }
        }
    }
    Ok(blocks)
}

pub(crate) fn any_block_is_possible(state: &DuelState) -> Result<bool, InternalError> {
    Ok(!possible_blocks(state)?.is_empty())
}

/// A creature with menace can't be blocked by exactly one creature.
fn blocks_are_legal(state: &DuelState) -> Result<bool, InternalError> {
    for attacker in &combat(state)?.attackers {
        if attacker.blockers.len() == 1
            && state.is_creature(attacker.attacker)?
            && state.has_keyword(attacker.attacker, Keyword::Menace)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn legal_block_actions(state: &DuelState) -> Result<Vec<Action>, InternalError> {
    let mut actions: Vec<Action> = possible_blocks(state)?
        .into_iter()
        .map(|(blocker, attacker)| Action::Block { blocker, attacker })
        .collect();
    if combat(state)?
        .attackers
        .iter()
        .any(|a| !a.blockers.is_empty())
    {
        actions.push(Action::ResetDeclaration);
    }
    if blocks_are_legal(state)? {
        actions.push(Action::ConfirmBlockers);
    }
    Ok(actions)
}

pub(crate) fn declare_blocker(
    state: &mut DuelState,
    blocker: ObjectId,
    attacker: ObjectId,
) -> Result<(), InternalError> {
    combat_mut(state)?
        .attackers
        .iter_mut()
        .find(|a| a.attacker == attacker)
        .ok_or_else(|| InternalError::new(format!("{attacker:?} is not attacking")))?
        .blockers
        .push(blocker);
    Ok(())
}

/// Makes the declared blocks happen.
pub(crate) fn confirm_blockers(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    let player = state.active_player.opponent();
    let mut blocks = Vec::new();
    for attacker in &mut combat_mut(state)?.attackers {
        attacker.blocked = !attacker.blockers.is_empty();
        if attacker.blocked {
            blocks.push(BlockDeclared {
                attacker: attacker.attacker,
                blockers: attacker.blockers.clone(),
            });
        }
    }
    events.push(GameEvent::BlockersDeclared { player, blocks });
    Ok(())
}

/// The attackers and blockers still on the battlefield.
fn creatures_in_combat(state: &DuelState) -> Result<Vec<ObjectId>, InternalError> {
    let mut creatures = Vec::new();
    for attacker in &combat(state)?.attackers {
        creatures.push(attacker.attacker);
        creatures.extend(&attacker.blockers);
    }
    let mut present = Vec::new();
    for creature in creatures {
        if state.is_creature(creature)? {
            present.push(creature);
        }
    }
    Ok(present)
}

pub(crate) fn has_attackers(state: &DuelState) -> bool {
    state
        .combat
        .as_ref()
        .is_some_and(|combat| !combat.attackers.is_empty())
}

/// Whether the first-strike damage step is needed.
pub(crate) fn any_first_strike(state: &DuelState) -> Result<bool, InternalError> {
    for creature in creatures_in_combat(state)? {
        if state.has_keyword(creature, Keyword::FirstStrike)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Deals one step's combat damage, all at once.
///
/// In the first-strike step only creatures with first strike deal damage; in
/// the regular step, the creatures that didn't deal it then.
///
/// Damage is assigned automatically. An unblocked attacker hits the
/// defending player. A blocked attacker assigns lethal damage to each
/// blocker in the order they were declared, and what is left over goes to
/// the last blocker, or to the player if the attacker has trample.
pub(crate) fn deal_combat_damage(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    first_strike_step: bool,
) -> Result<(), InternalError> {
    let combat = combat(state)?.clone();
    let defender = Target::Player {
        player: state.active_player.opponent(),
    };

    let mut dealers = Vec::new();
    for creature in creatures_in_combat(state)? {
        let deals = if first_strike_step {
            state.has_keyword(creature, Keyword::FirstStrike)?
        } else {
            !combat.dealt_first_strike_damage.contains(&creature)
        };
        if deals {
            dealers.push(creature);
        }
    }

    // (source, target, amount), worked out before any of it is dealt.
    let mut assignments: Vec<(ObjectId, Target, i32)> = Vec::new();
    for attacker in &combat.attackers {
        let id = attacker.attacker;
        if !state.is_creature(id)? {
            continue;
        }
        let mut blockers = Vec::new();
        for &blocker in &attacker.blockers {
            if state.is_creature(blocker)? {
                blockers.push(blocker);
            }
        }

        if dealers.contains(&id) {
            let mut left = state.power(id)?.max(0);
            if !attacker.blocked {
                assignments.push((id, defender, left));
            } else {
                let deathtouch = state.has_keyword(id, Keyword::Deathtouch)?;
                let trample = state.has_keyword(id, Keyword::Trample)?;
                for (index, &blocker) in blockers.iter().enumerate() {
                    let is_last = index + 1 == blockers.len();
                    let amount = if is_last && !trample {
                        left
                    } else {
                        left.min(lethal_damage(state, blocker, deathtouch)?)
                    };
                    assignments.push((id, Target::Object { object: blocker }, amount));
                    left -= amount;
                }
                if trample {
                    assignments.push((id, defender, left));
                }
            }
        }
        for blocker in blockers {
            if dealers.contains(&blocker) {
                let power = state.power(blocker)?;
                assignments.push((blocker, Target::Object { object: id }, power));
            }
        }
    }

    for (source, target, amount) in assignments {
        deal_damage(state, events, source, target, amount)?;
    }
    if first_strike_step {
        combat_mut(state)?.dealt_first_strike_damage = dealers;
    }
    Ok(())
}

/// How much more damage a creature needs to be destroyed.
fn lethal_damage(
    state: &DuelState,
    creature: ObjectId,
    from_deathtouch: bool,
) -> Result<i32, InternalError> {
    let permanent = state.object(creature)?;
    if permanent.deathtouch_damage {
        return Ok(0);
    }
    let needed = (state.toughness(creature)? - permanent.damage).max(0);
    Ok(if from_deathtouch {
        needed.min(1)
    } else {
        needed
    })
}
