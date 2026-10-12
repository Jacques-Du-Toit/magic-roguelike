//! Casting spells, choosing targets, triggered abilities, and resolving the
//! stack.

use super::action::{Action, GameEvent, Target};
use super::effects::{Applied, apply_effect};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::mana::{pay_cost, plan_payment};
use super::state::{
    Decision, DuelState, EffectStep, PendingTrigger, Resolving, StackItem, StackItemKind,
    TriggeredAbility, Zone, card_def,
};
use super::triggers::{Happening, fire};
use crate::cards::{
    AbilityDef, CardDef, CardType, ControlledBy, CreatureFilter, Effect, GraveyardKind, Keyword,
    SpellKind, TargetSpec,
};

/// Every spell the player could cast right now, as concrete actions: one
/// per legal choice of targets.
pub(crate) fn legal_casts(
    state: &DuelState,
    player: PlayerId,
) -> Result<Vec<Action>, InternalError> {
    let mut actions = Vec::new();
    // The hand, and a commander waiting in the command zone.
    let player_state = state.player(player);
    for &card in player_state.hand.iter().chain(&player_state.command) {
        let def = state.def(card)?;
        if !can_cast_now(state, player, def) {
            continue;
        }
        if plan_payment(state, player, &state.casting_cost(card)?, Some(card))?.is_none() {
            continue;
        }
        for targets in target_choices(state, player, &target_specs(def))? {
            actions.push(Action::CastSpell { card, targets });
        }
    }
    Ok(actions)
}

/// Whether the rules allow the player to cast this card at this moment,
/// leaving aside targets and whether they can pay for it.
fn can_cast_now(state: &DuelState, player: PlayerId, def: &CardDef) -> bool {
    if def.is_land() {
        return false;
    }
    def.has_type(CardType::Instant)
        || def.has_keyword(Keyword::Flash)
        || has_sorcery_timing(state, player)
}

/// Whether `target` is something the spec describes, right now, for a spell
/// or ability `player` controls. This is what keeps an Aura attached;
/// whether the player may aim at it is `is_legal_target`.
pub(crate) fn fits_target_spec(
    state: &DuelState,
    player: PlayerId,
    spec: TargetSpec,
    target: Target,
) -> Result<bool, InternalError> {
    Ok(candidates(state, player, spec)?.contains(&target))
}

/// Whether `player` may aim a spell or ability at `target`: it fits the
/// spec, and it isn't an opponent's permanent with hexproof.
pub(crate) fn is_legal_target(
    state: &DuelState,
    player: PlayerId,
    spec: TargetSpec,
    target: Target,
) -> Result<bool, InternalError> {
    Ok(fits_target_spec(state, player, spec, target)? && !is_hexproof_from(state, player, target)?)
}

fn is_hexproof_from(
    state: &DuelState,
    player: PlayerId,
    target: Target,
) -> Result<bool, InternalError> {
    let Target::Object { object } = target else {
        return Ok(false);
    };
    Ok(state.is_on_battlefield(object)
        && state.object(object)?.owner != player
        && state.has_keyword(object, Keyword::Hexproof)?)
}

/// The player's own main phase with an empty stack.
pub(crate) fn has_sorcery_timing(state: &DuelState, player: PlayerId) -> bool {
    player == state.active_player && state.step.is_main() && state.stack.is_empty()
}

/// The effects of an instant or sorcery.
fn spell_effects(def: &CardDef) -> &[Effect] {
    def.abilities
        .iter()
        .find_map(|ability| match ability {
            AbilityDef::Spell { effects } => Some(effects.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

/// The targets a list of effects needs chosen, in order. The entries that
/// stand for something already known (`same`, `this`, `triggering`) are not
/// choices.
pub(crate) fn chosen_specs(effects: &[Effect]) -> Vec<TargetSpec> {
    effects
        .iter()
        .flat_map(Effect::targets)
        .filter(TargetSpec::is_chosen)
        .collect()
}

/// What the spell's targets must be, in the order they are chosen: what an
/// Aura enchants, or those of the effects of an instant or sorcery.
fn target_specs(def: &CardDef) -> Vec<TargetSpec> {
    if let Some(enchant) = def.enchant {
        return vec![enchant];
    }
    chosen_specs(spell_effects(def))
}

/// Everything that fits a target spec right now, for a spell or ability
/// that `player` controls.
fn candidates(
    state: &DuelState,
    player: PlayerId,
    spec: TargetSpec,
) -> Result<Vec<Target>, InternalError> {
    let mut targets = Vec::new();
    let creatures =
        |targets: &mut Vec<Target>, filter: CreatureFilter| -> Result<(), InternalError> {
            for &object in &state.battlefield {
                if state.is_creature(object)? && creature_passes(state, player, object, filter)? {
                    targets.push(Target::Object { object });
                }
            }
            Ok(())
        };
    let players = |targets: &mut Vec<Target>| {
        for player in PlayerId::ALL {
            targets.push(Target::Player { player });
        }
    };
    match spec {
        TargetSpec::AnyTarget {} => {
            players(&mut targets);
            creatures(&mut targets, CreatureFilter::default())?;
        }
        TargetSpec::Creature(filter) => creatures(&mut targets, filter)?,
        TargetSpec::CardInYourGraveyard(filter) => {
            for &object in &state.player(player).graveyard {
                let def = state.def(object)?;
                let passes = match filter.kind {
                    None => true,
                    Some(GraveyardKind::Creature) => def.is_creature(),
                    Some(GraveyardKind::Permanent) => !def.is_instant_or_sorcery(),
                };
                if passes {
                    targets.push(Target::Object { object });
                }
            }
        }
        TargetSpec::Opponent {} => targets.push(Target::Player {
            player: player.opponent(),
        }),
        TargetSpec::Player {} => players(&mut targets),
        // Never chosen: they stand for something already known.
        TargetSpec::Same {}
        | TargetSpec::This {}
        | TargetSpec::Triggering {}
        | TargetSpec::You {} => {}
        TargetSpec::Spell(filter) => {
            for item in &state.stack {
                if item.kind != StackItemKind::Spell {
                    continue;
                }
                let is_creature = state.def(item.id)?.is_creature();
                let passes = match filter.kind {
                    None => true,
                    Some(SpellKind::Creature) => is_creature,
                    Some(SpellKind::Noncreature) => !is_creature,
                };
                if passes {
                    targets.push(Target::Object { object: item.id });
                }
            }
        }
    }
    Ok(targets)
}

/// Whether a creature on the battlefield meets a filter's conditions, for a
/// spell or ability that `player` controls.
fn creature_passes(
    state: &DuelState,
    player: PlayerId,
    creature: ObjectId,
    filter: CreatureFilter,
) -> Result<bool, InternalError> {
    if let Some(color) = filter.not_color
        && state.def(creature)?.colors.contains(&color)
    {
        return Ok(false);
    }
    if let Some(power) = filter.min_power
        && state.power(creature)? < power
    {
        return Ok(false);
    }
    if let Some(keyword) = filter.keyword
        && !state.has_keyword(creature, keyword)?
    {
        return Ok(false);
    }
    if let Some(controller) = filter.controller {
        let yours = state.object(creature)?.owner == player;
        if yours != (controller == ControlledBy::You) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every way for `player` to choose one target per spec. A spell with no
/// targets has exactly one way (no targets); a spec nothing fits leaves no
/// way at all.
pub(crate) fn target_choices(
    state: &DuelState,
    player: PlayerId,
    specs: &[TargetSpec],
) -> Result<Vec<Vec<Target>>, InternalError> {
    let mut choices: Vec<Vec<Target>> = vec![Vec::new()];
    for &spec in specs {
        let mut options = Vec::new();
        for target in candidates(state, player, spec)? {
            if !is_hexproof_from(state, player, target)? {
                options.push(target);
            }
        }
        let mut extended = Vec::new();
        for chosen in &choices {
            for &option in &options {
                let mut next = chosen.clone();
                next.push(option);
                extended.push(next);
            }
        }
        choices = extended;
    }
    Ok(choices)
}

pub(crate) fn cast_spell(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    card: ObjectId,
    targets: &[Target],
) -> Result<(), InternalError> {
    let cost = state.casting_cost(card)?;
    let plan = plan_payment(state, player, &cost, Some(card))?
        .ok_or_else(|| InternalError::new(format!("no way to pay for {card:?}")))?;

    // Each cast from the command zone makes the next one dearer.
    if state.object(card)?.zone == Zone::Command {
        state.player_mut(player).commander_casts += 1;
    }
    let spell = state.move_object(card, Zone::Stack)?;
    state.stack.push(StackItem {
        id: spell,
        controller: player,
        kind: StackItemKind::Spell,
        targets: targets.to_vec(),
        subject: None,
    });
    events.push(GameEvent::SpellCast {
        player,
        card: state.card_ref(spell)?,
        targets: targets.to_vec(),
    });
    pay_cost(state, events, player, &cost, &plan)?;
    // The caster keeps priority, once whatever triggers on the spell being
    // cast is on the stack above it.
    fire(state, Happening::CastSpell { player, spell })
}

/// Starts resolving the top item of the stack and carries on until it has
/// finished or one of its effects needs a choice from a player. Returns
/// whether it finished; if not, `state.decision` says what it waits for
/// and `continue_resolution` picks it up again.
pub(crate) fn resolve_top(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<bool, InternalError> {
    let item = state
        .stack
        .last()
        .cloned()
        .ok_or_else(|| InternalError::new("resolving an empty stack"))?;
    let effects = match &item.kind {
        StackItemKind::Spell => {
            let def = state.def(item.id)?;
            if !def.is_instant_or_sorcery() {
                resolve_permanent_spell(state, events, &item)?;
                return Ok(true);
            }
            spell_effects(def).to_vec()
        }
        StackItemKind::Trigger { ability } => {
            let effects = ability_effects(state, ability)?;
            state.stack.pop();
            effects
        }
    };

    // Targets are checked again on resolution, all at once. A spell or
    // ability whose targets have all become illegal does nothing; otherwise
    // each effect happens if its own targets are still legal.
    let mut chosen = item.targets.iter();
    let mut last: Option<(Target, bool)> = None;
    let (mut any_target, mut any_legal) = (false, false);
    let mut steps = Vec::new();
    let missing = || InternalError::new(format!("{:?} is short of targets", item.id));
    // Something the ability refers to without targeting it: there to be
    // acted on only while it is still on the battlefield.
    let known = |object: ObjectId| (Target::Object { object }, state.is_on_battlefield(object));
    for effect in effects {
        let mut targets = Vec::new();
        let mut all_legal = true;
        for spec in effect.targets() {
            let (target, legal) = match spec {
                TargetSpec::Same {} => last.ok_or_else(missing)?,
                TargetSpec::This {} => known(item.source()),
                TargetSpec::Triggering {} => known(item.subject.ok_or_else(missing)?),
                TargetSpec::You {} => (
                    Target::Player {
                        player: item.controller,
                    },
                    true,
                ),
                _ => {
                    let target = *chosen.next().ok_or_else(missing)?;
                    let legal = is_legal_target(state, item.controller, spec, target)?;
                    any_target = true;
                    any_legal |= legal;
                    (target, legal)
                }
            };
            last = Some((target, legal));
            all_legal &= legal;
            targets.push(target);
        }
        if all_legal {
            steps.push(EffectStep { effect, targets });
        }
    }
    let fizzled = any_target && !any_legal;
    if fizzled {
        steps.clear();
    }
    state.resolving = Some(Resolving {
        item,
        steps,
        fizzled,
    });
    continue_resolution(state, events)
}

/// Carries out the remaining effects of the spell or ability being
/// resolved, stopping if one needs a choice. Returns whether it finished.
pub(crate) fn continue_resolution(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<bool, InternalError> {
    loop {
        let resolving = state
            .resolving
            .as_mut()
            .ok_or_else(|| InternalError::new("nothing is resolving"))?;
        if resolving.steps.is_empty() {
            break;
        }
        let step = resolving.steps.remove(0);
        let (controller, source) = (resolving.item.controller, resolving.item.source());
        let applied = apply_effect(
            state,
            events,
            &step.effect,
            controller,
            source,
            &step.targets,
        )?;
        if applied == Applied::Waiting {
            return Ok(false);
        }
    }
    let Some(resolving) = state.resolving.take() else {
        return Ok(true);
    };
    let item = resolving.item;
    match item.kind {
        StackItemKind::Spell => {
            let in_graveyard = state.move_object(item.id, Zone::Graveyard)?;
            let card = state.card_ref(in_graveyard)?;
            let player = item.controller;
            events.push(if resolving.fizzled {
                GameEvent::SpellFizzled { player, card }
            } else {
                GameEvent::SpellResolved { player, card }
            });
        }
        StackItemKind::Trigger { .. } => {
            events.push(GameEvent::AbilityResolved { ability: item.id });
        }
    }
    Ok(true)
}

/// A permanent spell becomes a permanent, unless it is an Aura whose target
/// has become illegal.
fn resolve_permanent_spell(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    item: &StackItem,
) -> Result<(), InternalError> {
    let def = state.def(item.id)?;
    let player = item.controller;
    let mut fizzles = false;
    for (&spec, &target) in target_specs(def).iter().zip(&item.targets) {
        fizzles |= !is_legal_target(state, player, spec, target)?;
    }
    if fizzles {
        let in_graveyard = state.move_object(item.id, Zone::Graveyard)?;
        let card = state.card_ref(in_graveyard)?;
        events.push(GameEvent::SpellFizzled { player, card });
        return Ok(());
    }
    let permanent = state.move_object(item.id, Zone::Battlefield)?;
    events.push(GameEvent::PermanentEntered {
        player,
        card: state.card_ref(permanent)?,
    });
    // An Aura enters attached to its target.
    if let (true, Some(&Target::Object { object })) = (def.is_aura(), item.targets.first()) {
        state.object_mut(permanent)?.attached_to = Some(object);
        events.push(GameEvent::AuraAttached {
            aura: permanent,
            enchanted: object,
        });
    }
    permanent_entered(state, permanent)
}

/// Notes the abilities that trigger on a permanent having just entered the
/// battlefield. They go on the stack the next time a player would get
/// priority.
pub(crate) fn permanent_entered(
    state: &mut DuelState,
    permanent: ObjectId,
) -> Result<(), InternalError> {
    fire(state, Happening::Entered(permanent))
}

/// Puts the waiting triggered abilities on the stack: the active player's
/// first (so they resolve last), then by the id of their source.
///
/// An ability with targets has them chosen now. With no legal target it is
/// dropped; with one way to choose, that way is taken; otherwise its
/// controller is asked (`Decision::ChooseTargets`) and the rest wait behind
/// it. Returns whether the game is now waiting on such a choice.
pub(crate) fn put_triggers_on_stack(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<bool, InternalError> {
    let active = state.active_player;
    state
        .pending_triggers
        .sort_by_key(|trigger| (trigger.controller != active, trigger.ability.source()));
    while let Some(trigger) = state.pending_triggers.first().cloned() {
        let mut choices = trigger_target_choices(state, &trigger)?;
        let targets = match choices.len() {
            0 => {
                state.pending_triggers.remove(0);
                continue;
            }
            1 => choices.remove(0),
            _ => {
                state.decision = Some(Decision::ChooseTargets {
                    player: trigger.controller,
                    source: trigger.ability.source(),
                });
                return Ok(true);
            }
        };
        state.pending_triggers.remove(0);
        push_trigger(state, events, trigger, targets);
    }
    Ok(false)
}

/// Every way to aim a waiting triggered ability. One way, with no targets,
/// if it has none. A "you may" ability that has a legal target can also be
/// declined, which is the choice of no targets at all.
fn trigger_target_choices(
    state: &DuelState,
    trigger: &PendingTrigger,
) -> Result<Vec<Vec<Target>>, InternalError> {
    let specs = chosen_specs(&ability_effects(state, &trigger.ability)?);
    let mut choices = target_choices(state, trigger.controller, &specs)?;
    if !specs.is_empty() && !choices.is_empty() && is_optional(&trigger.ability)? {
        choices.push(Vec::new());
    }
    Ok(choices)
}

/// Whether a triggered ability is a "you may" one.
fn is_optional(ability: &TriggeredAbility) -> Result<bool, InternalError> {
    let TriggeredAbility::Card { ability } = ability else {
        return Ok(false);
    };
    Ok(matches!(
        card_def(&ability.def)?.abilities.get(ability.index),
        Some(AbilityDef::Triggered { optional: true, .. })
    ))
}

fn push_trigger(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    trigger: PendingTrigger,
    targets: Vec<Target>,
) {
    let id = state.next_object_id();
    events.push(GameEvent::AbilityTriggered {
        player: trigger.controller,
        source: trigger.ability.source(),
        ability: id,
    });
    state.stack.push(StackItem {
        id,
        controller: trigger.controller,
        kind: StackItemKind::Trigger {
            ability: trigger.ability,
        },
        targets,
        subject: trigger.subject,
    });
}

/// The ways to answer `Decision::ChooseTargets`: one action per way to aim
/// the first waiting triggered ability.
pub(crate) fn legal_target_choices(state: &DuelState) -> Result<Vec<Action>, InternalError> {
    let trigger = state
        .pending_triggers
        .first()
        .ok_or_else(|| InternalError::new("no triggered ability is waiting for targets"))?;
    Ok(trigger_target_choices(state, trigger)?
        .into_iter()
        .map(|targets| Action::ChooseTargets { targets })
        .collect())
}

/// Puts the first waiting triggered ability on the stack with the targets
/// its controller chose.
pub(crate) fn choose_targets(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    targets: &[Target],
) -> Result<(), InternalError> {
    if state.pending_triggers.is_empty() {
        return Err(InternalError::new(
            "no triggered ability is waiting for targets",
        ));
    }
    let trigger = state.pending_triggers.remove(0);
    // No targets for an ability that needs some: its controller declined.
    let declined =
        targets.is_empty() && !chosen_specs(&ability_effects(state, &trigger.ability)?).is_empty();
    if !declined {
        push_trigger(state, events, trigger, targets.to_vec());
    }
    Ok(())
}

/// The effects of a triggered ability.
fn ability_effects(
    state: &DuelState,
    ability: &TriggeredAbility,
) -> Result<Vec<Effect>, InternalError> {
    let abilities = match ability {
        // "Whenever you cast a noncreature spell, this creature gets +1/+1
        // until end of turn."
        TriggeredAbility::Keyword {
            keyword: Keyword::Prowess,
            ..
        } => {
            return Ok(vec![Effect::ModifyPtUntilEot {
                target: TargetSpec::This {},
                power: 1,
                toughness: 1,
                for_each: None,
            }]);
        }
        TriggeredAbility::Keyword { keyword, .. } => {
            return Err(InternalError::new(format!(
                "{keyword:?} is not a triggered ability"
            )));
        }
        TriggeredAbility::Card { ability } => &card_def(&ability.def)?.abilities,
        TriggeredAbility::Emblem { emblem, .. } => {
            &state
                .emblems
                .iter()
                .find(|candidate| candidate.id == *emblem)
                .ok_or_else(|| InternalError::new(format!("no emblem {emblem:?}")))?
                .def
                .abilities
        }
    };
    let index = match ability {
        TriggeredAbility::Card { ability } => ability.index,
        TriggeredAbility::Emblem { index, .. } => *index,
        TriggeredAbility::Keyword { .. } => usize::MAX,
    };
    match abilities.get(index) {
        Some(AbilityDef::Triggered { effects, .. } | AbilityDef::Activated { effects, .. }) => {
            Ok(effects.clone())
        }
        _ => Err(InternalError::new(format!(
            "{ability:?} is not a triggered or activated ability"
        ))),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Color;
    use crate::data;
    use crate::duel::error::ActionError;
    use crate::duel::state::{Decision, Step};
    use crate::duel::test_support::{
        cast, empty_hand, in_main_phase, pass_both, pass_until, put_in_hand, put_on_battlefield,
    };
    use crate::duel::{apply_action, legal_actions};

    fn tapped(state: &DuelState, id: ObjectId) -> bool {
        state.objects[&id].tapped
    }

    fn can_cast(state: &DuelState, player: PlayerId, card: ObjectId) -> bool {
        legal_actions(state, player).contains(&cast(card))
    }

    #[test]
    fn casting_a_creature_taps_lands_and_puts_it_on_the_stack() {
        let (mut state, me, opponent) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        let forests = [
            put_on_battlefield(&mut state, me, "forest"),
            put_on_battlefield(&mut state, me, "forest"),
        ];
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        let card = state.objects[&bears].card;
        assert_eq!(
            legal_actions(&state, me),
            [Action::PassPriority, cast(bears)]
        );

        let events = apply_action(&mut state, me, cast(bears)).unwrap();
        let spell = state.stack[0].id;
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.stack[0].controller, me);
        assert_eq!(state.objects[&spell].card, card);
        assert_eq!(state.objects[&spell].zone, Zone::Stack);
        assert!(state.player(me).hand.is_empty());
        assert!(tapped(&state, forests[0]) && tapped(&state, forests[1]));
        assert!(state.player(me).mana_pool.is_empty());
        assert_eq!(
            events,
            [
                GameEvent::SpellCast {
                    player: me,
                    card: state.card_ref(spell).unwrap(),
                    targets: vec![]
                },
                GameEvent::PermanentTapped {
                    permanent: forests[0]
                },
                GameEvent::PermanentTapped {
                    permanent: forests[1]
                },
            ]
        );
        // The caster keeps priority; the opponent gets it when they pass.
        assert_eq!(state.decision, Some(Decision::Priority { player: me }));
        assert_eq!(legal_actions(&state, me), [Action::PassPriority]);
        apply_action(&mut state, me, Action::PassPriority).unwrap();
        assert_eq!(legal_actions(&state, opponent), [Action::PassPriority]);
        state.assert_consistent();
    }

    #[test]
    fn a_creature_spell_resolves_onto_the_battlefield_when_both_players_pass() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        put_on_battlefield(&mut state, me, "forest");
        put_on_battlefield(&mut state, me, "forest");
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        let card = state.objects[&bears].card;
        apply_action(&mut state, me, cast(bears)).unwrap();

        let events = pass_both(&mut state);
        let permanent = *state.battlefield.last().unwrap();
        assert_eq!(state.objects[&permanent].card, card);
        assert_eq!(state.objects[&permanent].zone, Zone::Battlefield);
        assert!(state.objects[&permanent].summoning_sick);
        assert!(!state.objects[&permanent].tapped);
        assert!(state.stack.is_empty());
        assert_eq!(
            events,
            [GameEvent::PermanentEntered {
                player: me,
                card: state.card_ref(permanent).unwrap()
            }]
        );
        // Resolving doesn't end the step: the active player has priority.
        assert_eq!(state.step, Step::PrecombatMain);
        assert_eq!(state.decision, Some(Decision::Priority { player: me }));
        state.assert_consistent();
    }

    #[test]
    fn every_creature_can_be_cast_with_the_right_lands() {
        let land_for = |color: Color| match color {
            Color::W => "plains",
            Color::U => "island",
            Color::B => "swamp",
            Color::R => "mountain",
            Color::G => "forest",
        };
        let creatures: Vec<_> = data::embedded()
            .unwrap()
            .cards
            .iter()
            .filter(|card| card.is_creature() && !card.token)
            .collect();
        assert!(creatures.len() >= 37);
        for def in creatures {
            let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
            // A land for each coloured symbol, and the first colour's for
            // the generic mana.
            let mut lands: Vec<&str> = def.mana_cost.colored().map(land_for).collect();
            for _ in 0..def.mana_cost.generic() {
                lands.push(land_for(def.colors[0]));
            }
            // One land short is not enough.
            let last = lands.pop().unwrap();
            for land in lands {
                put_on_battlefield(&mut state, me, land);
            }
            let creature = put_in_hand(&mut state, me, def.id.as_str());
            assert!(!can_cast(&state, me, creature), "{}", def.id);
            put_on_battlefield(&mut state, me, last);
            assert!(can_cast(&state, me, creature), "{}", def.id);

            apply_action(&mut state, me, cast(creature)).unwrap();
            pass_both(&mut state);
            let permanent = *state.battlefield.last().unwrap();
            assert_eq!(state.objects[&permanent].def, def.id);
            assert!(
                state
                    .permanents_of(me)
                    .all(|p| p.tapped || p.id == permanent),
                "{}",
                def.id
            );
        }
    }

    #[test]
    fn coloured_costs_need_the_right_colour() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        put_on_battlefield(&mut state, me, "mountain");
        put_on_battlefield(&mut state, me, "mountain");
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        let giant = put_in_hand(&mut state, me, "hill_giant");
        assert!(!can_cast(&state, me, bears));
        assert!(!can_cast(&state, me, giant));
        assert_eq!(
            apply_action(&mut state, me, cast(bears)),
            Err(ActionError::IllegalAction {
                player: me,
                action: cast(bears)
            })
        );
        // Any colour pays for generic mana.
        put_on_battlefield(&mut state, me, "forest");
        assert!(can_cast(&state, me, bears));
        put_on_battlefield(&mut state, me, "plains");
        assert!(can_cast(&state, me, giant));
    }

    #[test]
    fn tapped_lands_cannot_pay() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        for _ in 0..3 {
            put_on_battlefield(&mut state, me, "forest");
        }
        let first = put_in_hand(&mut state, me, "grizzly_bears");
        let second = put_in_hand(&mut state, me, "grizzly_bears");
        apply_action(&mut state, me, cast(first)).unwrap();
        pass_both(&mut state);
        // One untapped Forest is left.
        assert!(!can_cast(&state, me, second));
    }

    #[test]
    fn creatures_are_cast_only_in_your_own_main_phase_with_an_empty_stack() {
        let (mut state, me, opponent) = in_main_phase("deck_a", "deck_c", 1);
        for player in [me, opponent] {
            empty_hand(&mut state, player);
            for _ in 0..4 {
                put_on_battlefield(&mut state, player, "forest");
            }
        }
        let mine = put_in_hand(&mut state, me, "grizzly_bears");
        let another = put_in_hand(&mut state, me, "grizzly_bears");
        let theirs = put_in_hand(&mut state, opponent, "grizzly_bears");

        // Not while a spell is on the stack, and never for the opponent.
        apply_action(&mut state, me, cast(mine)).unwrap();
        assert!(!can_cast(&state, me, another));
        apply_action(&mut state, me, Action::PassPriority).unwrap();
        assert!(!can_cast(&state, opponent, theirs));
        apply_action(&mut state, opponent, Action::PassPriority).unwrap();
        assert!(can_cast(&state, me, another));

        // Not in the end step or the upkeep.
        pass_until(&mut state, 1, Step::End);
        assert!(!can_cast(&state, me, another));
        pass_until(&mut state, 2, Step::Upkeep);
        assert!(!can_cast(&state, opponent, theirs));
        pass_until(&mut state, 2, Step::PostcombatMain);
        assert!(can_cast(&state, opponent, theirs));
        assert!(!can_cast(&state, me, another));
    }

    #[test]
    fn a_land_cannot_be_played_while_a_spell_is_on_the_stack() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        put_on_battlefield(&mut state, me, "forest");
        let elves = put_in_hand(&mut state, me, "llanowar_elves");
        let land = put_in_hand(&mut state, me, "forest");
        assert!(legal_actions(&state, me).contains(&Action::PlayLand { card: land }));
        apply_action(&mut state, me, cast(elves)).unwrap();
        assert!(!legal_actions(&state, me).contains(&Action::PlayLand { card: land }));
        pass_both(&mut state);
        assert!(legal_actions(&state, me).contains(&Action::PlayLand { card: land }));
    }

    #[test]
    fn lands_untap_in_their_controllers_untap_step() {
        let (mut state, me, opponent) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, opponent);
        let forest = put_on_battlefield(&mut state, me, "forest");
        let elves = put_in_hand(&mut state, me, "llanowar_elves");
        apply_action(&mut state, me, cast(elves)).unwrap();
        pass_both(&mut state);
        assert!(tapped(&state, forest));

        // Still tapped through the opponent's turn.
        pass_until(&mut state, 2, Step::PrecombatMain);
        assert!(tapped(&state, forest));
        let events = pass_until(&mut state, 3, Step::Upkeep);
        assert!(!tapped(&state, forest));
        assert!(events.contains(&GameEvent::PermanentsUntapped { player: me }));
    }

    // ---- The auto-payer ----

    #[test]
    fn llanowar_elves_taps_for_green_mana() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        let forest = put_on_battlefield(&mut state, me, "forest");
        let elves = put_on_battlefield(&mut state, me, "llanowar_elves");
        let bears = put_in_hand(&mut state, me, "grizzly_bears");

        let events = apply_action(&mut state, me, cast(bears)).unwrap();
        assert!(tapped(&state, forest) && tapped(&state, elves));
        assert!(events.contains(&GameEvent::PermanentTapped { permanent: elves }));
    }

    #[test]
    fn a_summoning_sick_creature_cannot_tap_for_mana() {
        let (mut state, me, opponent) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        empty_hand(&mut state, opponent);
        put_on_battlefield(&mut state, me, "forest");
        put_on_battlefield(&mut state, me, "forest");
        let elves = put_in_hand(&mut state, me, "llanowar_elves");
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        apply_action(&mut state, me, cast(elves)).unwrap();
        pass_both(&mut state);
        let elves = *state.battlefield.last().unwrap();

        // One Forest is untapped; the Elves arrived this turn.
        assert!(state.objects[&elves].summoning_sick);
        assert!(!can_cast(&state, me, bears));

        // Not on the opponent's turn either, but from the next own turn on.
        pass_until(&mut state, 2, Step::PrecombatMain);
        assert!(state.objects[&elves].summoning_sick);
        pass_until(&mut state, 3, Step::PrecombatMain);
        assert!(!state.objects[&elves].summoning_sick);
        assert!(can_cast(&state, me, bears));
    }

    #[test]
    fn lands_are_tapped_before_creatures() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        let elves = put_on_battlefield(&mut state, me, "llanowar_elves");
        let forests = [
            put_on_battlefield(&mut state, me, "forest"),
            put_on_battlefield(&mut state, me, "forest"),
        ];
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        apply_action(&mut state, me, cast(bears)).unwrap();
        assert!(tapped(&state, forests[0]) && tapped(&state, forests[1]));
        assert!(!tapped(&state, elves));
    }

    #[test]
    fn generic_mana_spares_the_colours_the_rest_of_the_hand_needs() {
        // Casting Elvish Visionary ({1}{G}) with Savannah Lions ({W}) in
        // hand should leave the Plains untapped.
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        let plains = put_on_battlefield(&mut state, me, "plains");
        let forests = [
            put_on_battlefield(&mut state, me, "forest"),
            put_on_battlefield(&mut state, me, "forest"),
        ];
        let visionary = put_in_hand(&mut state, me, "elvish_visionary");
        let lions = put_in_hand(&mut state, me, "savannah_lions");
        apply_action(&mut state, me, cast(visionary)).unwrap();
        assert!(tapped(&state, forests[0]) && tapped(&state, forests[1]));
        assert!(!tapped(&state, plains));
        // The Visionary resolves, then its trigger.
        pass_both(&mut state);
        pass_both(&mut state);
        assert!(can_cast(&state, me, lions));
    }

    #[test]
    fn generic_mana_spares_a_needed_colour_even_when_it_is_the_larger_pile() {
        // Youthful Knight ({1}{W}) with two Giant Growths ({G}) in hand:
        // pay with both Plains and keep the Forests.
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        let forests = [
            put_on_battlefield(&mut state, me, "forest"),
            put_on_battlefield(&mut state, me, "forest"),
        ];
        let plains = [
            put_on_battlefield(&mut state, me, "plains"),
            put_on_battlefield(&mut state, me, "plains"),
        ];
        let knight = put_in_hand(&mut state, me, "youthful_knight");
        put_in_hand(&mut state, me, "giant_growth");
        put_in_hand(&mut state, me, "giant_growth");
        apply_action(&mut state, me, cast(knight)).unwrap();
        assert!(tapped(&state, plains[0]) && tapped(&state, plains[1]));
        assert!(!tapped(&state, forests[0]) && !tapped(&state, forests[1]));
    }

    #[test]
    fn with_nothing_else_in_hand_generic_mana_comes_from_the_larger_pile() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        empty_hand(&mut state, me);
        let plains = put_on_battlefield(&mut state, me, "plains");
        let mountains = [
            put_on_battlefield(&mut state, me, "mountain"),
            put_on_battlefield(&mut state, me, "mountain"),
        ];
        let forest = put_on_battlefield(&mut state, me, "forest");
        let bears = put_in_hand(&mut state, me, "grizzly_bears");
        apply_action(&mut state, me, cast(bears)).unwrap();
        assert!(tapped(&state, forest) && tapped(&state, mountains[0]));
        assert!(!tapped(&state, mountains[1]) && !tapped(&state, plains));
    }

    #[test]
    fn mana_pools_are_empty_between_steps() {
        let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
        state.player_mut(me).mana_pool.add(Color::G);
        pass_both(&mut state);
        assert_eq!(state.step, Step::BeginCombat);
        assert!(state.player(me).mana_pool.is_empty());
    }
}
