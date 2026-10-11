//! Casting spells and resolving the stack.

use super::action::{Action, GameEvent, Target};
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::mana::{pay_cost, plan_payment};
use super::state::{DuelState, StackItem, StackItemKind, Zone};
use crate::cards::CardDef;

/// Every spell the player could cast right now, as concrete actions.
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
        actions.push(Action::CastSpell {
            card,
            targets: Vec::new(),
        });
    }
    Ok(actions)
}

/// Whether the rules allow the player to cast this card at this moment,
/// leaving aside whether they can pay for it.
fn can_cast_now(state: &DuelState, player: PlayerId, def: &CardDef) -> bool {
    // Only creature spells are castable so far.
    def.is_creature() && has_sorcery_timing(state, player)
}

/// The player's own main phase with an empty stack.
pub(crate) fn has_sorcery_timing(state: &DuelState, player: PlayerId) -> bool {
    player == state.active_player && state.step.is_main() && state.stack.is_empty()
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
    match item.kind {
        StackItemKind::Spell => {
            // A permanent spell becomes a permanent.
            let permanent = state.move_object(item.id, Zone::Battlefield)?;
            events.push(GameEvent::PermanentEntered {
                player: item.controller,
                card: state.card_ref(permanent)?,
            });
        }
    }
    Ok(())
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
    fn every_starter_creature_can_be_cast_with_the_right_lands() {
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
        assert_eq!(creatures.len(), 17);
        for def in creatures {
            let (mut state, me, _) = in_main_phase("deck_a", "deck_c", 1);
            let color = def.colors[0];
            // One land short is not enough.
            for _ in 1..def.mana_cost.mana_value() {
                put_on_battlefield(&mut state, me, land_for(color));
            }
            let creature = put_in_hand(&mut state, me, def.id.as_str());
            assert!(!can_cast(&state, me, creature), "{}", def.id);
            put_on_battlefield(&mut state, me, land_for(color));
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
        assert_eq!(state.step, Step::PostcombatMain);
        assert!(state.player(me).mana_pool.is_empty());
    }
}
