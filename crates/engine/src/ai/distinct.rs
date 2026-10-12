//! Telling apart only the choices that differ.
//!
//! With five identical tokens on the battlefield, sacrificing any one of
//! them is the same play, and so is aiming at any one of them. The AI
//! weighs each of its choices by playing it out on a copy of the game, so
//! weighing five that cannot differ costs five times what it should, and
//! inside the search's imagined games it costs that thousands of times.

use std::collections::BTreeSet;

use crate::cards::Keyword;
use crate::duel::{Action, DuelState, InternalError, ObjectId, PlayerId, Target, Zone};

/// What a card is, as far as any choice about it goes. Two cards alike in
/// all of this are interchangeable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Likeness {
    /// Something about it is its own: it is treated as like nothing else.
    Itself(ObjectId),
    Alike {
        def: &'static str,
        owner: PlayerId,
        zone: Zone,
        tapped: bool,
        summoning_sick: bool,
        damage: i32,
        deathtouch_damage: bool,
        power_bonus: i32,
        toughness_bonus: i32,
        granted_keywords: Vec<Keyword>,
        plus_counters: u32,
        skip_untap: bool,
    },
}

/// One part of what an action says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Kind(u8),
    Ability(usize),
    Player(PlayerId),
    Card(Likeness),
    /// The same card as the one named this many cards earlier in the
    /// action: a creature aiming its ability at itself is not the same play
    /// as aiming it at its twin.
    Again(usize),
    Nothing,
}

/// The objects that something else in the game refers to by id, so that
/// they are not interchangeable with a twin: a creature in combat, a
/// permanent with something attached, the target of a spell on the stack,
/// the source or subject of an ability that is waiting or has been used.
fn referred_to(state: &DuelState) -> BTreeSet<ObjectId> {
    let mut ids = BTreeSet::new();
    let mut targets = |targets: &[Target]| {
        for target in targets {
            if let Target::Object { object } = target {
                ids.insert(*object);
            }
        }
    };
    for item in &state.stack {
        targets(&item.targets);
    }
    if let Some(resolving) = &state.resolving {
        targets(&resolving.item.targets);
        for step in &resolving.steps {
            targets(&step.targets);
        }
    }
    for item in &state.stack {
        ids.insert(item.id);
        ids.insert(item.source());
        ids.extend(item.subject);
    }
    if let Some(resolving) = &state.resolving {
        ids.insert(resolving.item.source());
        ids.extend(resolving.item.subject);
    }
    for trigger in &state.pending_triggers {
        ids.insert(trigger.ability.source());
        ids.extend(trigger.subject);
    }
    for ability in &state.triggered_this_turn {
        ids.insert(ability.source());
    }
    if let Some(combat) = &state.combat {
        for attacker in &combat.attackers {
            ids.insert(attacker.attacker);
            ids.extend(&attacker.blockers);
        }
        ids.extend(&combat.dealt_first_strike_damage);
    }
    for &id in &state.battlefield {
        if let Some(object) = state.objects.get(&id)
            && let Some(host) = object.attached_to
        {
            ids.insert(id);
            ids.insert(host);
        }
    }
    ids
}

fn likeness(
    state: &DuelState,
    referred_to: &BTreeSet<ObjectId>,
    id: ObjectId,
) -> Result<Likeness, InternalError> {
    let Some(object) = state.objects.get(&id) else {
        // An emblem, or an ability on the stack.
        return Ok(Likeness::Itself(id));
    };
    let own = referred_to.contains(&id)
        || !matches!(
            object.zone,
            Zone::Battlefield | Zone::Hand | Zone::Graveyard
        )
        || state.is_commander(id);
    if own {
        return Ok(Likeness::Itself(id));
    }
    Ok(Likeness::Alike {
        def: state.def(id)?.id.as_str(),
        owner: object.owner,
        zone: object.zone,
        tapped: object.tapped,
        summoning_sick: object.summoning_sick,
        damage: object.damage,
        deathtouch_damage: object.deathtouch_damage,
        power_bonus: object.power_bonus,
        toughness_bonus: object.toughness_bonus,
        granted_keywords: object.granted_keywords.clone(),
        plus_counters: object.plus_counters,
        skip_untap: object.skip_untap,
    })
}

/// The actions among `actions` that are different plays: of several that
/// differ only in which of some interchangeable cards they name, the first
/// is kept. Only spells, abilities and target choices are ever dropped.
pub(super) fn distinct<'a>(
    state: &DuelState,
    actions: &'a [Action],
) -> Result<Vec<&'a Action>, InternalError> {
    if actions.len() < 3 {
        return Ok(actions.iter().collect());
    }
    let referred_to = referred_to(state);
    let mut seen: Vec<Vec<Part>> = Vec::new();
    let mut kept = Vec::new();
    for action in actions {
        let mut parts = Vec::new();
        let mut named: Vec<ObjectId> = Vec::new();
        let mut card = |id: ObjectId| -> Result<Part, InternalError> {
            if let Some(earlier) = named.iter().position(|other| *other == id) {
                return Ok(Part::Again(earlier));
            }
            named.push(id);
            likeness(state, &referred_to, id).map(Part::Card)
        };
        let aimed = |parts: &mut Vec<Part>,
                     card: &mut dyn FnMut(ObjectId) -> Result<Part, InternalError>,
                     targets: &[Target]|
         -> Result<(), InternalError> {
            for target in targets {
                parts.push(match target {
                    Target::Player { player } => Part::Player(*player),
                    Target::Object { object } => card(*object)?,
                });
            }
            Ok(())
        };
        match action {
            Action::CastSpell {
                card: spell,
                targets,
            } => {
                parts.push(Part::Kind(0));
                parts.push(card(*spell)?);
                aimed(&mut parts, &mut card, targets)?;
            }
            Action::ActivateAbility {
                source,
                ability,
                targets,
                sacrifice,
            } => {
                parts.push(Part::Kind(1));
                parts.push(card(*source)?);
                parts.push(Part::Ability(*ability));
                parts.push(match sacrifice {
                    Some(sacrificed) => card(*sacrificed)?,
                    None => Part::Nothing,
                });
                aimed(&mut parts, &mut card, targets)?;
            }
            Action::ChooseTargets { targets } => {
                parts.push(Part::Kind(2));
                aimed(&mut parts, &mut card, targets)?;
            }
            _ => {
                kept.push(action);
                continue;
            }
        }
        if !seen.contains(&parts) {
            seen.push(parts);
            kept.push(action);
        }
    }
    Ok(kept)
}

/// For each of `cards`, the position in the list of the first card it is
/// interchangeable with (its own, if there is none before it).
pub(super) fn classes(state: &DuelState, cards: &[ObjectId]) -> Result<Vec<usize>, InternalError> {
    let referred_to = referred_to(state);
    let mut likenesses: Vec<Likeness> = Vec::with_capacity(cards.len());
    let mut classes = Vec::with_capacity(cards.len());
    for &card in cards {
        let likeness = likeness(state, &referred_to, card)?;
        let class = likenesses
            .iter()
            .position(|other| *other == likeness)
            .unwrap_or(likenesses.len());
        classes.push(class);
        likenesses.push(likeness);
    }
    Ok(classes)
}
