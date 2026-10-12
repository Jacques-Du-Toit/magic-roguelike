//! The searching AI's attacks and blocks.
//!
//! An attack or a set of blocks is a whole plan, declared a creature at a
//! time. There are too many plans to try them all, so the search starts
//! from a few (version 1's, everything, nothing) and then changes the best
//! one a creature at a time for as long as the imagined games come out
//! better, up to a fixed number of plans.
//!
//! Each call works the plan out for the board as it was before anything
//! was declared, so every step of a declaration heads for the same plan.
//! The last plan is remembered between those steps; that only saves the
//! work of finding it again.

use std::cell::Cell;

use super::combat;
use super::distinct;
use super::evaluate;
use super::search::{Imagined, fnv1a};
use super::{VERSION_1, settle};
use crate::balance::AiSearchBalance;
use crate::duel::{
    Action, ActionError, Decision, DuelState, InternalError, ObjectId, PlayerId, Target,
    apply_trusted, legal_actions, player_view,
};

/// Every order of `0..count`, the natural one first. Beyond four it is
/// only the natural one: more blockers than that on one attacker is rare,
/// and the orders multiply.
fn orders(count: usize) -> Vec<Vec<usize>> {
    let natural: Vec<usize> = (0..count).collect();
    if count > 4 {
        return vec![natural];
    }
    let mut all = vec![Vec::new()];
    for _ in 0..count {
        let mut longer = Vec::new();
        for order in &all {
            for &next in &natural {
                if !order.contains(&next) {
                    let mut order: Vec<usize> = order.clone();
                    order.push(next);
                    longer.push(order);
                }
            }
        }
        all = longer;
    }
    all
}

/// Dividing an attacker's damage among its blockers: the division that
/// leaves the board best. Nothing is hidden about this, so no games are
/// imagined: each way of ordering the blockers (lethal damage to each in
/// turn, the rest to the last, or to the player for a trampler) is tried
/// on a copy and the result is judged as it stands.
pub(super) fn assign_damage(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
) -> Result<Action, InternalError> {
    let assignment = state
        .combat
        .as_ref()
        .and_then(|combat| combat.assignments.iter().find(|a| !a.confirmed))
        .ok_or_else(|| InternalError::new("no damage is being divided"))?;
    let is_blocker =
        |share: &crate::duel::DamageShare| matches!(share.target, Target::Object { .. });
    let blockers: Vec<usize> = (0..assignment.shares.len())
        .filter(|&index| assignment.shares.get(index).is_some_and(is_blocker))
        .collect();
    let to_player = assignment
        .shares
        .iter()
        .position(|share| !is_blocker(share));

    let mut best: Option<(i32, Vec<i32>)> = None;
    for order in orders(blockers.len()) {
        // The amounts this order gives, share by share.
        let mut amounts = vec![0; assignment.shares.len()];
        let mut left = assignment.power;
        for (place, blocker) in order.iter().enumerate() {
            let Some((&index, share)) = blockers
                .get(*blocker)
                .and_then(|index| Some((index, assignment.shares.get(*index)?)))
            else {
                continue;
            };
            let last = place + 1 == order.len() && to_player.is_none();
            let amount = if last { left } else { left.min(share.lethal) };
            if let Some(slot) = amounts.get_mut(index) {
                *slot = amount;
            }
            left -= amount;
        }
        if let Some(slot) = to_player.and_then(|index| amounts.get_mut(index)) {
            *slot = left;
        }

        let mut game = state.clone();
        let dividing = game
            .combat
            .as_mut()
            .and_then(|combat| combat.assignments.iter_mut().find(|a| !a.confirmed))
            .ok_or_else(|| InternalError::new("no damage is being divided"))?;
        for (share, &amount) in dividing.shares.iter_mut().zip(&amounts) {
            share.amount = amount;
        }
        if !dividing.is_legal() {
            continue;
        }
        // Any other attacker's damage is divided the automatic way.
        for _ in 0..64 {
            if !matches!(game.decision, Some(Decision::AssignCombatDamage { .. })) {
                break;
            }
            apply_trusted(&mut game, player, &Action::ConfirmDamage)?;
        }
        let settled = match settle(&mut game, VERSION_1) {
            Ok(()) => evaluate::position(&game, player)?,
            Err(ActionError::Internal(error)) => return Err(error),
            Err(error) => return Err(InternalError::new(error.to_string())),
        };
        if best.as_ref().is_none_or(|(score, _)| settled > *score) {
            best = Some((settled, amounts));
        }
    }
    let Some((_, goal)) = best else {
        return Ok(Action::ConfirmDamage);
    };

    // One step towards the division: take damage back first, then give it,
    // to the blockers before the player.
    for (share, &wanted) in assignment.shares.iter().zip(&goal) {
        if share.amount > wanted {
            return Ok(Action::UnassignDamage {
                target: share.target,
            });
        }
    }
    for (share, &wanted) in assignment.shares.iter().zip(&goal) {
        let step = Action::AssignDamage {
            target: share.target,
        };
        if share.amount < wanted && actions.contains(&step) {
            return Ok(step);
        }
    }
    Ok(Action::ConfirmDamage)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// The creatures to attack with.
    Attack(Vec<ObjectId>),
    /// `(blocker, attacker)` pairs, in the order to declare them.
    Block(Vec<(ObjectId, ObjectId)>),
}

thread_local! {
    /// The plan last worked out, with the position it was for.
    static REMEMBERED: Cell<Option<(u64, Plan)>> = const { Cell::new(None) };
}

/// A number that stands for everything a plan depends on: what the player
/// can see, and how the search is set up.
fn position_key(
    state: &DuelState,
    player: PlayerId,
    settings: &AiSearchBalance,
) -> Result<u64, InternalError> {
    let to_error = |error: serde_json::Error| InternalError::new(error.to_string());
    let mut bytes = serde_json::to_vec(&player_view(state, player)?).map_err(to_error)?;
    bytes.extend(serde_json::to_vec(settings).map_err(to_error)?);
    Ok(fnv1a(&bytes))
}

/// The plan for the position, from memory if it was the last one worked
/// out and still fits (`fits` checks it against what is legal).
fn remembered_or(
    key: u64,
    fits: impl Fn(&Plan) -> bool,
    work_out: impl FnOnce() -> Result<Plan, InternalError>,
) -> Result<Plan, InternalError> {
    let remembered = REMEMBERED.with(Cell::take);
    let plan = match remembered {
        Some((remembered_key, plan)) if remembered_key == key && fits(&plan) => plan,
        _ => work_out()?,
    };
    REMEMBERED.with(|cell| cell.set(Some((key, plan.clone()))));
    Ok(plan)
}

/// The position with the declaration under way taken back.
fn before_declaring(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
) -> Result<DuelState, InternalError> {
    let mut before = state.clone();
    if actions.contains(&Action::ResetDeclaration) {
        apply_trusted(&mut before, player, &Action::ResetDeclaration)?;
    }
    Ok(before)
}

pub(super) fn declare_attackers(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    let before = before_declaring(state, player, actions)?;
    let mut able: Vec<ObjectId> = legal_actions(&before, player)
        .iter()
        .filter_map(|action| match action {
            Action::Attack { creature } => Some(*creature),
            _ => None,
        })
        .collect();
    able.sort_unstable();
    let plan = remembered_or(
        position_key(&before, player, settings)?,
        |plan| matches!(plan, Plan::Attack(attackers) if attackers.iter().all(|a| able.contains(a))),
        || attack_plan(&before, player, &able, settings).map(Plan::Attack),
    )?;
    let Plan::Attack(plan) = plan else {
        return Err(InternalError::new("expected a plan of attack"));
    };

    let declared = combat::declared_attackers(state);
    if declared.iter().any(|creature| !plan.contains(creature)) {
        return Ok(Action::ResetDeclaration);
    }
    Ok(plan
        .into_iter()
        .find(|creature| !declared.contains(creature))
        .map_or(Action::ConfirmAttackers, |creature| Action::Attack {
            creature,
        }))
}

/// The attack that does best in the imagined games, of those tried.
fn attack_plan(
    before: &DuelState,
    player: PlayerId,
    able: &[ObjectId],
    settings: &AiSearchBalance,
) -> Result<Vec<ObjectId>, InternalError> {
    if able.is_empty() {
        return Ok(Vec::new());
    }
    let classes = distinct::classes(before, able)?;
    // Attacking with any one of several interchangeable creatures is the
    // same attack, so an attack is written with the first of each kind.
    let tidy = |plan: &[ObjectId]| -> Vec<ObjectId> {
        let mut wanted: Vec<usize> = vec![0; able.len()];
        for (creature, class) in able.iter().zip(&classes) {
            if plan.contains(creature) {
                wanted[*class] += 1;
            }
        }
        let mut tidied = Vec::new();
        for (creature, class) in able.iter().zip(&classes) {
            if wanted[*class] > 0 {
                wanted[*class] -= 1;
                tidied.push(*creature);
            }
        }
        tidied
    };
    let imagined = Imagined::new(before, player, settings)?;
    let score = |plan: &[ObjectId]| {
        imagined.total(player, settings, false, |game| {
            for &creature in plan {
                apply_trusted(game, player, &Action::Attack { creature })?;
            }
            apply_trusted(game, player, &Action::ConfirmAttackers).map(|_| ())
        })
    };

    let preferred = tidy(&combat::attack_plan(before, player, able)?);
    let mut tried: Vec<Vec<ObjectId>> = vec![preferred.clone()];
    let mut best = (score(&preferred)?, preferred);
    let budget = settings.attack_plans as usize;
    // Everything, and nothing.
    for plan in [able.to_vec(), Vec::new()] {
        if tried.len() < budget && !tried.contains(&plan) {
            let total = score(&plan)?;
            tried.push(plan.clone());
            if total > best.0 {
                best = (total, plan);
            }
        }
    }
    // One creature more or one fewer, for as long as that helps.
    let mut improved = true;
    while improved {
        improved = false;
        for class in 0..able.len() {
            for add in [true, false] {
                let mut plan = best.1.clone();
                let member = able
                    .iter()
                    .zip(&classes)
                    .find(|(creature, of)| **of == class && plan.contains(creature) != add)
                    .map(|(creature, _)| *creature);
                let Some(creature) = member else {
                    continue;
                };
                if add {
                    plan.push(creature);
                } else {
                    plan.retain(|other| *other != creature);
                }
                let plan = tidy(&plan);
                if tried.len() >= budget {
                    return Ok(best.1);
                }
                if tried.contains(&plan) {
                    continue;
                }
                let total = score(&plan)?;
                tried.push(plan.clone());
                if total > best.0 {
                    best = (total, plan);
                    improved = true;
                }
            }
        }
    }
    Ok(best.1)
}

/// The blocks that could be declared before any has been: `(blocker,
/// attacker)`.
fn possible_blocks(before: &DuelState, player: PlayerId) -> Vec<(ObjectId, ObjectId)> {
    legal_actions(before, player)
        .iter()
        .filter_map(|action| match action {
            Action::Block { blocker, attacker } => Some((*blocker, *attacker)),
            _ => None,
        })
        .collect()
}

pub(super) fn declare_blockers(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    let before = before_declaring(state, player, actions)?;
    let possible = possible_blocks(&before, player);
    let plan = remembered_or(
        position_key(&before, player, settings)?,
        |plan| matches!(plan, Plan::Block(blocks) if blocks.iter().all(|b| possible.contains(b))),
        || block_plan(&before, player, &possible, settings).map(Plan::Block),
    )?;
    let Plan::Block(plan) = plan else {
        return Err(InternalError::new("expected a plan of blocks"));
    };

    let mut declared = Vec::new();
    if let Some(combat) = &state.combat {
        for attacker in &combat.attackers {
            for &blocker in &attacker.blockers {
                declared.push((blocker, attacker.attacker));
            }
        }
    }
    if declared.iter().any(|block| !plan.contains(block)) {
        return Ok(Action::ResetDeclaration);
    }
    for &(blocker, attacker) in &plan {
        if !declared.contains(&(blocker, attacker)) {
            return Ok(Action::Block { blocker, attacker });
        }
    }
    Ok(if actions.contains(&Action::ConfirmBlockers) {
        Action::ConfirmBlockers
    } else {
        Action::ResetDeclaration
    })
}

/// Declares `plan` on a copy of the position. Returns whether the blocks
/// may be confirmed (a creature with menace can't have just one blocker).
fn declare_blocks(
    game: &mut DuelState,
    player: PlayerId,
    plan: &[(ObjectId, ObjectId)],
) -> Result<bool, InternalError> {
    for &(blocker, attacker) in plan {
        apply_trusted(game, player, &Action::Block { blocker, attacker })?;
    }
    Ok(legal_actions(game, player).contains(&Action::ConfirmBlockers))
}

/// The blocks that do best in the imagined games, of those tried.
fn block_plan(
    before: &DuelState,
    player: PlayerId,
    possible: &[(ObjectId, ObjectId)],
    settings: &AiSearchBalance,
) -> Result<Vec<(ObjectId, ObjectId)>, InternalError> {
    if possible.is_empty() {
        return Ok(Vec::new());
    }
    let is_legal = |plan: &[(ObjectId, ObjectId)]| {
        let fits = plan.iter().all(|block| possible.contains(block));
        Ok::<bool, InternalError>(fits && declare_blocks(&mut before.clone(), player, plan)?)
    };
    let imagined = Imagined::new(before, player, settings)?;
    let score = |plan: &[(ObjectId, ObjectId)]| {
        imagined.total(player, settings, false, |game| {
            declare_blocks(game, player, plan)?;
            apply_trusted(game, player, &Action::ConfirmBlockers).map(|_| ())
        })
    };

    let mut blockers: Vec<ObjectId> = possible.iter().map(|(blocker, _)| *blocker).collect();
    blockers.dedup();
    let classes = distinct::classes(before, &blockers)?;
    // The biggest attackers first.
    let mut attackers: Vec<(i32, ObjectId)> = Vec::new();
    for attacker in combat::declared_attackers(before) {
        if possible.iter().any(|(_, blocked)| *blocked == attacker) {
            attackers.push((before.power(attacker)?, attacker));
        }
    }
    attackers.sort_unstable_by(|a, b| b.cmp(a));

    let preferred = combat::block_plan(before, player, false)?;
    let preferred = if is_legal(&preferred)? {
        preferred
    } else {
        Vec::new()
    };
    let mut tried: Vec<Vec<(ObjectId, ObjectId)>> = vec![preferred.clone()];
    let mut best = (score(&preferred)?, preferred);
    let budget = settings.block_plans as usize;
    // No blocks at all, and version 1's blocks with two creatures ganging
    // up on an attacker where that kills it: a step the one-blocker changes
    // below can't reach, since the first of the two blockers alone is
    // usually a worse block than none.
    for plan in [Vec::new(), combat::block_plan(before, player, true)?] {
        if tried.len() < budget && !tried.contains(&plan) && is_legal(&plan)? {
            let total = score(&plan)?;
            tried.push(plan.clone());
            if total > best.0 {
                best = (total, plan);
            }
        }
    }
    // Move one blocker (or take it out), for as long as that helps.
    let mut improved = true;
    while improved {
        improved = false;
        for &(_, attacker) in &attackers {
            // Interchangeable blockers doing the same thing now are tried once.
            let mut kinds_tried: Vec<(usize, Option<ObjectId>)> = Vec::new();
            for (&blocker, &class) in blockers.iter().zip(&classes) {
                if !possible.contains(&(blocker, attacker)) {
                    continue;
                }
                let blocking_now = best
                    .1
                    .iter()
                    .find(|(other, _)| *other == blocker)
                    .map(|(_, blocked)| *blocked);
                if kinds_tried.contains(&(class, blocking_now)) {
                    continue;
                }
                kinds_tried.push((class, blocking_now));

                let mut plan = best.1.clone();
                plan.retain(|(other, _)| *other != blocker);
                if blocking_now != Some(attacker) {
                    plan.push((blocker, attacker));
                }
                if !is_legal(&plan)? {
                    // One blocker on a creature with menace: add another.
                    let second = blockers.iter().find(|other| {
                        possible.contains(&(**other, attacker))
                            && !plan.iter().any(|(used, _)| used == *other)
                    });
                    let Some(&second) = second else {
                        continue;
                    };
                    plan.push((second, attacker));
                    if !is_legal(&plan)? {
                        continue;
                    }
                }
                if tried.len() >= budget {
                    return Ok(best.1);
                }
                if tried.contains(&plan) {
                    continue;
                }
                let total = score(&plan)?;
                tried.push(plan.clone());
                if total > best.0 {
                    best = (total, plan);
                    improved = true;
                }
            }
        }
    }
    Ok(best.1)
}
