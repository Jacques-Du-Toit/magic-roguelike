//! The AI's attack and block decisions.
//!
//! Each call works out the whole attack or block it wants from scratch,
//! then returns the next step towards it. The plan depends only on the
//! board, not on what has been declared so far, so the steps add up.

use std::collections::BTreeSet;

use super::evaluate::creature_value;
use crate::cards::{Keyword, Restricted};
use crate::duel::{Action, DuelState, InternalError, ObjectId, PlayerId};

/// What happens when `attacker` is blocked by `blocker` alone:
/// `(attacker dies, blocker dies)`.
fn fight(
    state: &DuelState,
    attacker: ObjectId,
    blocker: ObjectId,
) -> Result<(bool, bool), InternalError> {
    // Whether `source` kills `victim` with `hits` rounds of its damage.
    let kills = |source: ObjectId, victim: ObjectId, hits: i32| -> Result<bool, InternalError> {
        let power = state.power(source)?;
        let toughness_left = state.toughness(victim)? - state.object(victim)?.damage;
        Ok(power > 0
            && (state.has_keyword(source, Keyword::Deathtouch)? || power * hits >= toughness_left))
    };
    let double = |creature: ObjectId| state.has_keyword(creature, Keyword::DoubleStrike);
    let first = |creature: ObjectId| -> Result<bool, InternalError> {
        Ok(state.has_keyword(creature, Keyword::FirstStrike)? || double(creature)?)
    };
    let hits = |creature: ObjectId| -> Result<i32, InternalError> {
        Ok(if double(creature)? { 2 } else { 1 })
    };
    let (attacker_first, blocker_first) = (first(attacker)?, first(blocker)?);
    // A kill in the first-strike step leaves the other no chance to hit back.
    Ok(
        if attacker_first && !blocker_first && kills(attacker, blocker, 1)? {
            (false, true)
        } else if blocker_first && !attacker_first && kills(blocker, attacker, 1)? {
            (true, false)
        } else {
            (
                kills(blocker, attacker, hits(blocker)?)?,
                kills(attacker, blocker, hits(attacker)?)?,
            )
        },
    )
}

/// Whether `blocker` is able to block `attacker` at all.
fn can_block(
    state: &DuelState,
    blocker: ObjectId,
    attacker: ObjectId,
) -> Result<bool, InternalError> {
    if state.object(blocker)?.tapped || state.is_restricted(blocker, Restricted::Block)? {
        return Ok(false);
    }
    Ok((!state.has_keyword(attacker, Keyword::Flying)?
        || state.has_keyword(blocker, Keyword::Flying)?
        || state.has_keyword(blocker, Keyword::Reach)?)
        && state.can_be_blocked_by(attacker, blocker)?)
}

/// The creatures of `defender` that could block `attacker`.
fn blockers_for(
    state: &DuelState,
    defender: PlayerId,
    attacker: ObjectId,
) -> Result<Vec<ObjectId>, InternalError> {
    let mut blockers = Vec::new();
    for creature in state.creatures_of(defender)? {
        if can_block(state, creature, attacker)? {
            blockers.push(creature);
        }
    }
    Ok(blockers)
}

pub(super) fn declared_attackers(state: &DuelState) -> Vec<ObjectId> {
    state
        .combat
        .as_ref()
        .map(|combat| combat.attackers.iter().map(|a| a.attacker).collect())
        .unwrap_or_default()
}

pub(super) fn declare_attackers(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
) -> Result<Action, InternalError> {
    let declared = declared_attackers(state);
    let mut able = declared.clone();
    for action in actions {
        if let Action::Attack { creature } = action {
            able.push(*creature);
        }
    }
    able.sort_unstable();

    let plan = attack_plan(state, player, &able)?;
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

/// Which of the creatures able to attack should.
pub(super) fn attack_plan(
    state: &DuelState,
    player: PlayerId,
    able: &[ObjectId],
) -> Result<Vec<ObjectId>, InternalError> {
    let defender = player.opponent();
    if damage_past_blockers(state, defender, able)? >= state.player(defender).life {
        return Ok(able.to_vec());
    }

    let mut plan = Vec::new();
    for &attacker in able {
        let blockers = blockers_for(state, defender, attacker)?;
        let safe = if state.has_keyword(attacker, Keyword::Menace)? {
            !pair_kills(state, attacker, &blockers)?
        } else {
            let mut safe = true;
            for &blocker in &blockers {
                let (attacker_dies, blocker_dies) = fight(state, attacker, blocker)?;
                let fair_trade = blocker_dies
                    && creature_value(state, attacker)? <= creature_value(state, blocker)?;
                if attacker_dies && !fair_trade {
                    safe = false;
                }
            }
            safe
        };
        if safe {
            plan.push(attacker);
        }
    }
    Ok(plan)
}

/// Whether two of `blockers` together would kill `attacker`.
fn pair_kills(
    state: &DuelState,
    attacker: ObjectId,
    blockers: &[ObjectId],
) -> Result<bool, InternalError> {
    if blockers.len() < 2 {
        return Ok(false);
    }
    let mut powers = Vec::new();
    for &blocker in blockers {
        if state.has_keyword(blocker, Keyword::Deathtouch)? && state.power(blocker)? > 0 {
            return Ok(true);
        }
        powers.push(state.power(blocker)?.max(0));
    }
    powers.sort_unstable_by(|a, b| b.cmp(a));
    let toughness_left = state.toughness(attacker)? - state.object(attacker)?.damage;
    Ok(powers[0] + powers[1] >= toughness_left)
}

/// The damage the defending player takes if all of `attackers` attack and
/// the defender blocks to take as little as possible: each blocker stops
/// one attacker, the biggest it can (two blockers for menace).
fn damage_past_blockers(
    state: &DuelState,
    defender: PlayerId,
    attackers: &[ObjectId],
) -> Result<i32, InternalError> {
    let mut by_power = Vec::new();
    for &attacker in attackers {
        let hits = if state.has_keyword(attacker, Keyword::DoubleStrike)? {
            2
        } else {
            1
        };
        by_power.push((state.power(attacker)?.max(0) * hits, attacker));
    }
    by_power.sort_unstable_by(|a, b| b.cmp(a));

    let mut used = BTreeSet::new();
    let mut damage = 0;
    for (power, attacker) in by_power {
        let free: Vec<ObjectId> = blockers_for(state, defender, attacker)?
            .into_iter()
            .filter(|blocker| !used.contains(blocker))
            .collect();
        let needed = if state.has_keyword(attacker, Keyword::Menace)? {
            2
        } else {
            1
        };
        if free.len() >= needed {
            used.extend(free.into_iter().take(needed));
        } else {
            damage += power;
        }
    }
    Ok(damage)
}

pub(super) fn declare_blockers(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    gang_blocks: bool,
) -> Result<Action, InternalError> {
    let mut declared = Vec::new();
    if let Some(combat) = &state.combat {
        for attacker in &combat.attackers {
            for &blocker in &attacker.blockers {
                declared.push((blocker, attacker.attacker));
            }
        }
    }
    let plan = block_plan(state, player, gang_blocks)?;
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

/// Two free blockers that together kill `attacker` at the cost of one of
/// them at most, when that one is worth no more than the attacker.
fn gang_for(
    state: &DuelState,
    attacker: ObjectId,
    free: &[ObjectId],
) -> Result<Option<(ObjectId, ObjectId)>, InternalError> {
    // First strike decides such fights in ways this doesn't work out.
    for keyword in [Keyword::FirstStrike, Keyword::DoubleStrike] {
        if state.has_keyword(attacker, keyword)? {
            return Ok(None);
        }
    }
    let power = state.power(attacker)?.max(0);
    let deadly = state.has_keyword(attacker, Keyword::Deathtouch)? && power > 0;
    let toughness_left = state.toughness(attacker)? - state.object(attacker)?.damage;
    let attacker_value = creature_value(state, attacker)?;
    // (what the pair stands to lose, the pair)
    let mut best: Option<(i32, (ObjectId, ObjectId))> = None;
    for (index, &first) in free.iter().enumerate() {
        for &second in free.iter().skip(index + 1) {
            let mut together = 0;
            let mut kills = false;
            let mut needed = Vec::new();
            for blocker in [first, second] {
                let blocker_power = state.power(blocker)?.max(0);
                together += blocker_power;
                kills |= blocker_power > 0 && state.has_keyword(blocker, Keyword::Deathtouch)?;
                let left = state.toughness(blocker)? - state.object(blocker)?.damage;
                needed.push(if deadly { 1 } else { left.max(1) });
            }
            if !(kills || together >= toughness_left) {
                continue;
            }
            // It can't kill both, and the one it would kill is the dearer
            // of those it can.
            if power >= needed.iter().sum() {
                continue;
            }
            let mut loss = 0;
            for (blocker, needed) in [first, second].into_iter().zip(&needed) {
                if power >= *needed {
                    loss = loss.max(creature_value(state, blocker)?);
                }
            }
            if loss <= attacker_value && best.is_none_or(|(least, _)| loss < least) {
                best = Some((loss, (first, second)));
            }
        }
    }
    Ok(best.map(|(_, pair)| pair))
}

/// The blocks to make, as `(blocker, attacker)` pairs. With `gang_blocks`,
/// two creatures also block one attacker to kill it when that costs one of
/// them at most; version 1 itself doesn't, only the players in the
/// searching AI's imagined games.
pub(super) fn block_plan(
    state: &DuelState,
    player: PlayerId,
    gang_blocks: bool,
) -> Result<Vec<(ObjectId, ObjectId)>, InternalError> {
    let mut attackers = Vec::new();
    for attacker in declared_attackers(state) {
        if state.is_creature(attacker)? {
            attackers.push((state.power(attacker)?.max(0), attacker));
        }
    }
    // Deal with the biggest attackers first.
    attackers.sort_unstable_by(|a, b| b.cmp(a));

    let mut plan: Vec<(ObjectId, ObjectId)> = Vec::new();
    let mut used = BTreeSet::new();
    let mut blocked = BTreeSet::new();

    // Blocks worth making for their own sake. Menace attackers would need
    // two blockers, so they are only blocked to survive (below).
    for &(_, attacker) in &attackers {
        if state.has_keyword(attacker, Keyword::Menace)? {
            continue;
        }
        // (how good the block is, the blocker's value, the blocker)
        let mut best: Option<(u8, i32, ObjectId)> = None;
        for blocker in blockers_for(state, player, attacker)? {
            if used.contains(&blocker) {
                continue;
            }
            let (attacker_dies, blocker_dies) = fight(state, attacker, blocker)?;
            let value = creature_value(state, blocker)?;
            let rank = match (attacker_dies, blocker_dies) {
                // Kills the attacker and lives.
                (true, false) => 0,
                // Stops the damage and lives.
                (false, false) => 1,
                // Trades for something worth at least as much.
                (true, true) if value <= creature_value(state, attacker)? => 2,
                _ => continue,
            };
            if best.is_none_or(|(best_rank, best_value, _)| (rank, value) < (best_rank, best_value))
            {
                best = Some((rank, value, blocker));
            }
        }
        if let Some((_, _, blocker)) = best {
            plan.push((blocker, attacker));
            used.insert(blocker);
            blocked.insert(attacker);
        }
    }

    if gang_blocks {
        for &(_, attacker) in &attackers {
            if blocked.contains(&attacker) || state.has_keyword(attacker, Keyword::Menace)? {
                continue;
            }
            let free: Vec<ObjectId> = blockers_for(state, player, attacker)?
                .into_iter()
                .filter(|blocker| !used.contains(blocker))
                .collect();
            if let Some((first, second)) = gang_for(state, attacker, &free)? {
                for blocker in [first, second] {
                    plan.push((blocker, attacker));
                    used.insert(blocker);
                }
                blocked.insert(attacker);
            }
        }
    }

    // If what gets through would be lethal, throw the cheapest creatures in
    // front of the biggest attackers until it isn't.
    let life = state.player(player).life;
    for &(_, attacker) in &attackers {
        let incoming: i32 = attackers
            .iter()
            .filter(|(_, other)| !blocked.contains(other))
            .map(|(power, _)| power)
            .sum();
        if incoming < life {
            break;
        }
        if blocked.contains(&attacker) {
            continue;
        }
        let mut free = Vec::new();
        for blocker in blockers_for(state, player, attacker)? {
            if !used.contains(&blocker) {
                free.push((creature_value(state, blocker)?, blocker));
            }
        }
        free.sort_unstable();
        let needed = if state.has_keyword(attacker, Keyword::Menace)? {
            2
        } else {
            1
        };
        if free.len() < needed {
            continue;
        }
        for &(_, blocker) in free.iter().take(needed) {
            plan.push((blocker, attacker));
            used.insert(blocker);
        }
        blocked.insert(attacker);
    }
    Ok(plan)
}
