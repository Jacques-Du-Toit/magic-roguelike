//! Activated abilities: what a player may activate, and paying for it.

use super::action::{Action, GameEvent, Target};
use super::damage::creature_dies;
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::mana::{pay_cost, plan_payment};
use super::stack::{chosen_specs, target_choices};
use super::state::{AbilityRef, DuelState, StackItem, StackItemKind, TriggeredAbility, Zone};
use super::triggers::{Happening, fire};
use crate::cards::{AbilityDef, ActivationCost, Effect, Keyword, SacrificeCost};

/// The activated ability `abilities[index]` of a permanent.
fn activated(
    state: &DuelState,
    source: ObjectId,
    index: usize,
) -> Result<(&'static ActivationCost, &'static [Effect]), InternalError> {
    match state.def(source)?.abilities.get(index) {
        Some(AbilityDef::Activated { cost, effects }) => Ok((cost, effects)),
        _ => Err(InternalError::new(format!(
            "{source:?} has no activated ability {index}"
        ))),
    }
}

/// Every activated ability the player could use right now, as concrete
/// actions: one per legal choice of targets and of what to sacrifice.
pub(crate) fn legal_activations(
    state: &DuelState,
    player: PlayerId,
) -> Result<Vec<Action>, InternalError> {
    let mut actions = Vec::new();
    for permanent in state.permanents_of(player) {
        let source = permanent.id;
        let def = state.def(source)?;
        for (index, ability) in def.abilities.iter().enumerate() {
            let AbilityDef::Activated { cost, effects } = ability else {
                continue;
            };
            if cost.tap {
                // A creature can't tap for a cost the turn it arrives.
                let sick = def.is_creature()
                    && permanent.summoning_sick
                    && !state.has_keyword(source, Keyword::Haste)?;
                if permanent.tapped || sick {
                    continue;
                }
            }
            if plan_payment(state, player, &cost.mana, None)?.is_none() {
                continue;
            }
            let sacrifices: Vec<Option<ObjectId>> = match cost.sacrifice {
                None => vec![None],
                Some(SacrificeCost::Creature) => {
                    state.creatures_of(player)?.into_iter().map(Some).collect()
                }
            };
            let targets = target_choices(state, player, &chosen_specs(effects))?;
            for &sacrifice in &sacrifices {
                for targets in &targets {
                    actions.push(Action::ActivateAbility {
                        source,
                        ability: index,
                        targets: targets.clone(),
                        sacrifice,
                    });
                }
            }
        }
    }
    Ok(actions)
}

/// Puts an activated ability on the stack and pays its cost: mana, then the
/// tap, then the sacrifice.
pub(crate) fn activate(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    source: ObjectId,
    index: usize,
    targets: &[Target],
    sacrifice: Option<ObjectId>,
) -> Result<(), InternalError> {
    let (cost, _) = activated(state, source, index)?;
    let plan = plan_payment(state, player, &cost.mana, None)?
        .ok_or_else(|| InternalError::new(format!("no way to pay for {source:?}'s ability")))?;

    let id = state.next_object_id();
    let ability = AbilityRef {
        source,
        def: state.object(source)?.def.clone(),
        index,
    };
    state.stack.push(StackItem {
        id,
        controller: player,
        kind: StackItemKind::Trigger {
            ability: TriggeredAbility::Card { ability },
        },
        targets: targets.to_vec(),
        subject: None,
    });
    events.push(GameEvent::AbilityActivated {
        player,
        source,
        ability: id,
        targets: targets.to_vec(),
    });

    pay_cost(state, events, player, &cost.mana, &plan)?;
    if cost.tap {
        state.object_mut(source)?.tapped = true;
        events.push(GameEvent::PermanentTapped { permanent: source });
    }
    if let Some(permanent) = sacrifice {
        sacrifice_permanent(state, events, player, permanent)?;
    }
    Ok(())
}

/// A player sacrifices one of their permanents: it goes to its owner's
/// graveyard, and a creature dies.
pub(crate) fn sacrifice_permanent(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    permanent: ObjectId,
) -> Result<(), InternalError> {
    fire(state, Happening::Sacrificed(permanent))?;
    events.push(GameEvent::PermanentSacrificed { player, permanent });
    if state.is_creature(permanent)? {
        return creature_dies(state, events, permanent);
    }
    let card = state.move_object(permanent, Zone::Graveyard)?;
    events.push(GameEvent::PermanentPutIntoGraveyard {
        player,
        permanent,
        card: state.card_ref(card)?,
    });
    Ok(())
}
