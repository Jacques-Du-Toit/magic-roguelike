//! Combat: declaring attackers and blockers, and combat damage.

use super::action::{Action, BlockDeclared, GameEvent, Target};
use super::damage::deal_damage;
use super::error::InternalError;
use super::ids::ObjectId;
use super::state::{Attacker, Combat, DamageAssignment, DamageShare, DuelState};
use super::triggers::{Happening, fire};
use crate::cards::{Keyword, Restricted};

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
        if ready
            && !state.has_keyword(creature, Keyword::Defender)?
            && !permanent.tapped
            && !combat.is_attacking(creature)
            && !state.is_restricted(creature, Restricted::Attack)?
        {
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
    let declared = &combat(state)?.attackers;
    actions.extend(declared.iter().map(|attacker| Action::RemoveAttacker {
        creature: attacker.attacker,
    }));
    if !declared.is_empty() {
        actions.push(Action::ResetDeclaration);
    }
    actions.push(Action::ConfirmAttackers);
    Ok(actions)
}

/// Takes one creature back out of the attack being declared.
pub(crate) fn remove_attacker(
    state: &mut DuelState,
    creature: ObjectId,
) -> Result<(), InternalError> {
    combat_mut(state)?
        .attackers
        .retain(|attacker| attacker.attacker != creature);
    Ok(())
}

/// Takes one creature back out of the blocks being declared.
pub(crate) fn remove_blocker(
    state: &mut DuelState,
    blocker: ObjectId,
) -> Result<(), InternalError> {
    for attacker in &mut combat_mut(state)?.attackers {
        attacker.blockers.retain(|declared| *declared != blocker);
    }
    Ok(())
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
    for &attacker in &attackers {
        if !state.has_keyword(attacker, Keyword::Vigilance)? {
            state.object_mut(attacker)?.tapped = true;
            events.push(GameEvent::PermanentTapped {
                permanent: attacker,
            });
        }
    }
    for attacker in attackers {
        fire(state, Happening::Attacked(attacker))?;
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
    if state.object(blocker)?.tapped
        || combat.is_blocking(blocker)
        || state.is_restricted(blocker, Restricted::Block)?
    {
        return Ok(false);
    }
    if state.has_keyword(attacker, Keyword::Flying)? {
        let reaches = state.has_keyword(blocker, Keyword::Flying)?
            || state.has_keyword(blocker, Keyword::Reach)?;
        if !reaches {
            return Ok(false);
        }
    }
    state.can_be_blocked_by(attacker, blocker)
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
    let declared: Vec<ObjectId> = combat(state)?
        .attackers
        .iter()
        .flat_map(|a| a.blockers.iter().copied())
        .collect();
    actions.extend(
        declared
            .iter()
            .map(|&blocker| Action::RemoveBlocker { blocker }),
    );
    if !declared.is_empty() {
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

/// Whether the creature deals its damage in the first-strike step.
fn strikes_first(state: &DuelState, creature: ObjectId) -> Result<bool, InternalError> {
    Ok(state.has_keyword(creature, Keyword::FirstStrike)?
        || state.has_keyword(creature, Keyword::DoubleStrike)?)
}

/// Whether the first-strike damage step is needed.
pub(crate) fn any_first_strike(state: &DuelState) -> Result<bool, InternalError> {
    for creature in creatures_in_combat(state)? {
        if strikes_first(state, creature)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The creatures that deal damage in this damage step: in the first-strike
/// step those with first strike or double strike, in the regular step those
/// that didn't deal it then and those with double strike.
fn dealers(state: &DuelState, first_strike_step: bool) -> Result<Vec<ObjectId>, InternalError> {
    let combat = combat(state)?;
    let mut dealers = Vec::new();
    for creature in creatures_in_combat(state)? {
        let deals = if first_strike_step {
            strikes_first(state, creature)?
        } else {
            !combat.dealt_first_strike_damage.contains(&creature)
                || state.has_keyword(creature, Keyword::DoubleStrike)?
        };
        if deals {
            dealers.push(creature);
        }
    }
    Ok(dealers)
}

/// The blockers of an attacker that are still on the battlefield.
fn live_blockers(state: &DuelState, attacker: &Attacker) -> Result<Vec<ObjectId>, InternalError> {
    let mut blockers = Vec::new();
    for &blocker in &attacker.blockers {
        if state.is_creature(blocker)? {
            blockers.push(blocker);
        }
    }
    Ok(blockers)
}

/// How a blocked attacker's damage is divided when nobody says otherwise:
/// lethal damage to each blocker in the order they were declared, and what
/// is left over to the last blocker, or to the player if the attacker has
/// trample.
fn automatic_assignment(
    state: &DuelState,
    attacker: &Attacker,
) -> Result<DamageAssignment, InternalError> {
    let id = attacker.attacker;
    let power = state.power(id)?.max(0);
    let deathtouch = state.has_keyword(id, Keyword::Deathtouch)?;
    let trample = state.has_keyword(id, Keyword::Trample)?;
    let blockers = live_blockers(state, attacker)?;
    let mut left = power;
    let mut shares = Vec::new();
    for (index, &blocker) in blockers.iter().enumerate() {
        let lethal = lethal_damage(state, blocker, deathtouch)?;
        let is_last = index + 1 == blockers.len();
        let amount = if is_last && !trample {
            left
        } else {
            left.min(lethal)
        };
        shares.push(DamageShare {
            target: Target::Object { object: blocker },
            amount,
            lethal,
        });
        left -= amount;
    }
    if trample {
        shares.push(DamageShare {
            target: Target::Player {
                player: state.active_player.opponent(),
            },
            amount: left,
            lethal: 0,
        });
    }
    Ok(DamageAssignment {
        attacker: id,
        power,
        shares,
        confirmed: false,
    })
}

/// Sets up the damage divisions the attacking player gets to choose in this
/// damage step: one for each attacker that deals damage now and has two or
/// more blockers. With fewer blockers there is nothing worth choosing.
/// Returns whether there are any.
pub(crate) fn prepare_assignments(
    state: &mut DuelState,
    first_strike_step: bool,
) -> Result<bool, InternalError> {
    let dealers = dealers(state, first_strike_step)?;
    let mut assignments = Vec::new();
    for attacker in &combat(state)?.attackers {
        if !dealers.contains(&attacker.attacker) || !state.is_creature(attacker.attacker)? {
            continue;
        }
        if live_blockers(state, attacker)?.len() < 2 {
            continue;
        }
        let assignment = automatic_assignment(state, attacker)?;
        if assignment.power > 0 {
            assignments.push(assignment);
        }
    }
    let any = !assignments.is_empty();
    combat_mut(state)?.assignments = assignments;
    Ok(any)
}

/// The attacker whose damage is being divided, if any is left to confirm.
pub(crate) fn pending_assignment(state: &DuelState) -> Option<ObjectId> {
    state
        .combat
        .as_ref()?
        .assignments
        .iter()
        .find(|assignment| !assignment.confirmed)
        .map(|assignment| assignment.attacker)
}

fn current_assignment(state: &DuelState) -> Result<&DamageAssignment, InternalError> {
    combat(state)?
        .assignments
        .iter()
        .find(|assignment| !assignment.confirmed)
        .ok_or_else(|| InternalError::new("no damage is being divided"))
}

fn current_assignment_mut(state: &mut DuelState) -> Result<&mut DamageAssignment, InternalError> {
    combat_mut(state)?
        .assignments
        .iter_mut()
        .find(|assignment| !assignment.confirmed)
        .ok_or_else(|| InternalError::new("no damage is being divided"))
}

pub(crate) fn legal_assignment_actions(state: &DuelState) -> Result<Vec<Action>, InternalError> {
    let assignment = current_assignment(state)?;
    let unassigned = assignment.unassigned();
    let mut actions = Vec::new();
    for share in &assignment.shares {
        // Damage may go to the player only once every blocker has lethal.
        let allowed =
            matches!(share.target, Target::Object { .. }) || assignment.blockers_have_lethal();
        if unassigned > 0 && allowed {
            actions.push(Action::AssignDamage {
                target: share.target,
            });
        }
    }
    for share in &assignment.shares {
        if share.amount > 0 {
            actions.push(Action::UnassignDamage {
                target: share.target,
            });
        }
    }
    if assignment.is_legal() {
        actions.push(Action::ConfirmDamage);
    }
    Ok(actions)
}

/// Moves one damage to (`change` 1) or from (`change` -1) a recipient of
/// the division being made.
pub(crate) fn change_assignment(
    state: &mut DuelState,
    target: Target,
    change: i32,
) -> Result<(), InternalError> {
    let share = current_assignment_mut(state)?
        .shares
        .iter_mut()
        .find(|share| share.target == target)
        .ok_or_else(|| InternalError::new(format!("{target:?} takes none of this damage")))?;
    share.amount += change;
    Ok(())
}

pub(crate) fn confirm_assignment(state: &mut DuelState) -> Result<(), InternalError> {
    current_assignment_mut(state)?.confirmed = true;
    Ok(())
}

/// Deals one step's combat damage, all at once.
///
/// An unblocked attacker hits the defending player. A blocked attacker's
/// damage is divided as its controller chose, if they were asked (see
/// `prepare_assignments`), and automatically otherwise.
pub(crate) fn deal_combat_damage(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    first_strike_step: bool,
) -> Result<(), InternalError> {
    let combat = combat(state)?.clone();
    let defender = Target::Player {
        player: state.active_player.opponent(),
    };
    let dealers = dealers(state, first_strike_step)?;

    // (source, target, amount), worked out before any of it is dealt.
    let mut assignments: Vec<(ObjectId, Target, i32)> = Vec::new();
    for attacker in &combat.attackers {
        let id = attacker.attacker;
        if !state.is_creature(id)? {
            continue;
        }
        if dealers.contains(&id) {
            if !attacker.blocked {
                assignments.push((id, defender, state.power(id)?.max(0)));
            } else {
                let chosen = combat
                    .assignments
                    .iter()
                    .find(|assignment| assignment.attacker == id && assignment.confirmed);
                let division = match chosen {
                    Some(chosen) => chosen.clone(),
                    None => automatic_assignment(state, attacker)?,
                };
                for share in division.shares {
                    assignments.push((id, share.target, share.amount));
                }
            }
        }
        for blocker in live_blockers(state, attacker)? {
            if dealers.contains(&blocker) {
                let power = state.power(blocker)?;
                assignments.push((blocker, Target::Object { object: id }, power));
            }
        }
    }

    for (source, target, amount) in assignments {
        deal_damage(state, events, source, target, amount)?;
    }
    let combat = combat_mut(state)?;
    combat.assignments.clear();
    if first_strike_step {
        combat.dealt_first_strike_damage = dealers;
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
