//! Casting spells, choosing targets, triggered abilities, and resolving the
//! stack.

use super::action::{Action, GameEvent, Target};
use super::effects::apply_effect;
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::mana::{pay_cost, plan_payment};
use super::state::{
    AbilityRef, DuelState, PendingTrigger, StackItem, StackItemKind, TriggeredAbility, Zone,
    card_def,
};
use crate::cards::{
    AbilityDef, CardDef, CardType, CreatureFilter, Effect, Keyword, SpellKind, TargetSpec, Trigger,
    TriggerSubject,
};

/// Every spell the player could cast right now, as concrete actions: one
/// per legal choice of targets.
pub(crate) fn legal_casts(
    state: &DuelState,
    player: PlayerId,
) -> Result<Vec<Action>, InternalError> {
    let mut actions = Vec::new();
    for &card in &state.player(player).hand {
        let def = state.def(card)?;
        if !can_cast_now(state, player, def) {
            continue;
        }
        if plan_payment(state, player, &def.mana_cost, Some(card))?.is_none() {
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

/// Whether `target` is something the spec describes, right now. This is
/// what keeps an Aura attached; whether a player may aim at it is
/// `is_legal_target`.
pub(crate) fn fits_target_spec(
    state: &DuelState,
    spec: TargetSpec,
    target: Target,
) -> Result<bool, InternalError> {
    Ok(candidates(state, spec)?.contains(&target))
}

/// Whether `player` may aim a spell or ability at `target`: it fits the
/// spec, and it isn't an opponent's permanent with hexproof.
pub(crate) fn is_legal_target(
    state: &DuelState,
    player: PlayerId,
    spec: TargetSpec,
    target: Target,
) -> Result<bool, InternalError> {
    Ok(fits_target_spec(state, spec, target)? && !is_hexproof_from(state, player, target)?)
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

/// What the spell's targets must be, in the order they are chosen: what an
/// Aura enchants, or one per targeted effect of an instant or sorcery.
fn target_specs(def: &CardDef) -> Vec<TargetSpec> {
    if let Some(enchant) = def.enchant {
        return vec![enchant];
    }
    spell_effects(def)
        .iter()
        .filter_map(|effect| effect.target().copied())
        .collect()
}

/// Everything that fits a target spec right now.
fn candidates(state: &DuelState, spec: TargetSpec) -> Result<Vec<Target>, InternalError> {
    let mut targets = Vec::new();
    let creatures =
        |targets: &mut Vec<Target>, filter: CreatureFilter| -> Result<(), InternalError> {
            for &object in &state.battlefield {
                if state.is_creature(object)? && creature_passes(state, object, filter)? {
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
        TargetSpec::Player {} => players(&mut targets),
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

/// Whether a creature on the battlefield meets a filter's conditions.
fn creature_passes(
    state: &DuelState,
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
    Ok(true)
}

/// Every way for `player` to choose one target per spec. A spell with no
/// targets has exactly one way (no targets); a spec nothing fits leaves no
/// way at all.
fn target_choices(
    state: &DuelState,
    player: PlayerId,
    specs: &[TargetSpec],
) -> Result<Vec<Vec<Target>>, InternalError> {
    let mut choices: Vec<Vec<Target>> = vec![Vec::new()];
    for &spec in specs {
        let mut options = Vec::new();
        for target in candidates(state, spec)? {
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
    let def = state.def(card)?;
    let plan = plan_payment(state, player, &def.mana_cost, Some(card))?
        .ok_or_else(|| InternalError::new(format!("no way to pay for {card:?}")))?;

    let spell = state.move_object(card, Zone::Stack)?;
    state.stack.push(StackItem {
        id: spell,
        controller: player,
        kind: StackItemKind::Spell,
        targets: targets.to_vec(),
    });
    events.push(GameEvent::SpellCast {
        player,
        card: state.card_ref(spell)?,
        targets: targets.to_vec(),
    });
    pay_cost(state, events, player, &def.mana_cost, &plan)?;
    // The caster keeps priority.
    state.consecutive_passes = 0;
    Ok(())
}

/// Resolves the top item of the stack.
pub(crate) fn resolve_top(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
) -> Result<(), InternalError> {
    let item = state
        .stack
        .last()
        .cloned()
        .ok_or_else(|| InternalError::new("resolving an empty stack"))?;
    match &item.kind {
        StackItemKind::Spell => resolve_spell(state, events, &item),
        StackItemKind::Trigger { ability } => {
            state.stack.pop();
            for effect in ability_effects(state, ability)? {
                apply_effect(
                    state,
                    events,
                    &effect,
                    item.controller,
                    ability.source(),
                    None,
                )?;
            }
            events.push(GameEvent::AbilityResolved { ability: item.id });
            Ok(())
        }
    }
}

fn resolve_spell(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    item: &StackItem,
) -> Result<(), InternalError> {
    let def = state.def(item.id)?;
    let player = item.controller;

    // Targets are checked again on resolution. A spell whose targets have
    // all become illegal does nothing; otherwise it does what it can.
    let specs = target_specs(def);
    let mut still_legal = Vec::new();
    for (&spec, &target) in specs.iter().zip(&item.targets) {
        still_legal.push(is_legal_target(state, player, spec, target)?);
    }
    let fizzles = !still_legal.is_empty() && !still_legal.contains(&true);

    if !fizzles && !def.is_instant_or_sorcery() {
        // A permanent spell becomes a permanent. An Aura enters attached
        // to its target.
        let permanent = state.move_object(item.id, Zone::Battlefield)?;
        events.push(GameEvent::PermanentEntered {
            player,
            card: state.card_ref(permanent)?,
        });
        if let (true, Some(&Target::Object { object })) = (def.is_aura(), item.targets.first()) {
            state.object_mut(permanent)?.attached_to = Some(object);
            events.push(GameEvent::AuraAttached {
                aura: permanent,
                enchanted: object,
            });
        }
        return permanent_entered(state, permanent);
    }

    if !fizzles {
        let mut targets = item.targets.iter().zip(&still_legal);
        for effect in spell_effects(def) {
            let target = match effect.target() {
                None => None,
                Some(_) => match targets.next() {
                    Some((target, true)) => Some(*target),
                    _ => continue,
                },
            };
            apply_effect(state, events, effect, player, item.id, target)?;
        }
    }
    let in_graveyard = state.move_object(item.id, Zone::Graveyard)?;
    let card = state.card_ref(in_graveyard)?;
    events.push(if fizzles {
        GameEvent::SpellFizzled { player, card }
    } else {
        GameEvent::SpellResolved { player, card }
    });
    Ok(())
}

/// Notes the "when this enters" abilities of a permanent that has just
/// entered the battlefield. They go on the stack the next time a player
/// would get priority.
pub(crate) fn permanent_entered(
    state: &mut DuelState,
    permanent: ObjectId,
) -> Result<(), InternalError> {
    let object = state.object(permanent)?;
    let (def_id, controller) = (object.def.clone(), object.owner);
    for (index, ability) in state.def(permanent)?.abilities.iter().enumerate() {
        let AbilityDef::Triggered { trigger, .. } = ability else {
            continue;
        };
        match trigger {
            Trigger::EntersBattlefield {
                subject: TriggerSubject::This,
            } => state.pending_triggers.push(PendingTrigger {
                ability: TriggeredAbility::Card {
                    ability: AbilityRef {
                        source: permanent,
                        def: def_id.clone(),
                        index,
                    },
                },
                controller,
            }),
            Trigger::EntersBattlefield {
                subject: TriggerSubject::CreatureYouControl,
            }
            | Trigger::BeginningOfUpkeep {} => {}
        }
    }
    if state.def(permanent)?.is_creature() {
        let entered = Trigger::EntersBattlefield {
            subject: TriggerSubject::CreatureYouControl,
        };
        emblems_trigger(state, controller, &entered);
    }
    Ok(())
}

/// Puts the waiting triggered abilities on the stack: the active player's
/// first (so they resolve last), then by the id of their source.
pub(crate) fn put_triggers_on_stack(state: &mut DuelState, events: &mut Vec<GameEvent>) {
    let mut pending = std::mem::take(&mut state.pending_triggers);
    let active = state.active_player;
    pending.sort_by_key(|trigger| (trigger.controller != active, trigger.ability.source()));
    for trigger in pending {
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
            targets: Vec::new(),
        });
    }
}

/// Notes every ability of `controller`'s emblems that triggers on
/// `trigger`. They go on the stack the next time a player would get
/// priority.
pub(crate) fn emblems_trigger(state: &mut DuelState, controller: PlayerId, trigger: &Trigger) {
    let mut triggered = Vec::new();
    for emblem in &state.emblems {
        if emblem.controller != controller {
            continue;
        }
        for (index, ability) in emblem.def.abilities.iter().enumerate() {
            if matches!(ability, AbilityDef::Triggered { trigger: on, .. } if on == trigger) {
                triggered.push(PendingTrigger {
                    ability: TriggeredAbility::Emblem {
                        emblem: emblem.id,
                        index,
                    },
                    controller,
                });
            }
        }
    }
    state.pending_triggers.extend(triggered);
}

/// The effects of a triggered ability.
fn ability_effects(
    state: &DuelState,
    ability: &TriggeredAbility,
) -> Result<Vec<Effect>, InternalError> {
    let abilities = match ability {
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
    };
    match abilities.get(index) {
        Some(AbilityDef::Triggered { effects, .. }) => Ok(effects.clone()),
        _ => Err(InternalError::new(format!(
            "{ability:?} is not a triggered ability"
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
            .filter(|card| card.is_creature())
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
