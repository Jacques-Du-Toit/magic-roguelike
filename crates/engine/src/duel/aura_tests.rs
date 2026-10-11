//! Tests for Auras, which so far means Pacifism.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step, Zone};
use super::test_support::{
    attack, cast, cast_at, empty_board, finish_combat, pass_both, pass_until,
    pass_until_other_decision, put_in_hand, put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions};

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

fn graveyard(state: &DuelState, player: PlayerId) -> Vec<&str> {
    state
        .player(player)
        .graveyard
        .iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

/// Casts Pacifism from `me` on `creature` and resolves it. Returns the Aura.
fn pacify(state: &mut DuelState, me: PlayerId, creature: ObjectId) -> ObjectId {
    put_lands(state, me, "plains", 2);
    let pacifism = put_in_hand(state, me, "pacifism");
    act(state, me, cast_at(pacifism, at(creature)));
    pass_both(state);
    *state.battlefield.last().unwrap()
}

#[test]
fn pacifism_enters_attached_to_target_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    let mine = put_on_battlefield(&mut state, me, "savannah_lions");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let pacifism = put_in_hand(&mut state, me, "pacifism");

    // It targets a creature, any creature, and needs one.
    let casts: Vec<Action> = legal_actions(&state, me)
        .into_iter()
        .filter(|action| matches!(action, Action::CastSpell { .. }))
        .collect();
    assert_eq!(
        casts,
        [cast_at(pacifism, at(mine)), cast_at(pacifism, at(theirs))]
    );
    assert!(apply_action(&mut state, me, cast(pacifism)).is_err());

    act(&mut state, me, cast_at(pacifism, at(theirs)));
    let events = pass_both(&mut state);
    let aura = *state.battlefield.last().unwrap();
    assert_eq!(state.objects[&aura].def.as_str(), "pacifism");
    assert_eq!(state.objects[&aura].zone, Zone::Battlefield);
    assert_eq!(state.objects[&aura].attached_to, Some(theirs));
    assert_eq!(state.objects[&aura].owner, me);
    assert_eq!(
        events,
        [
            GameEvent::PermanentEntered {
                player: me,
                card: state.card_ref(aura).unwrap()
            },
            GameEvent::AuraAttached {
                aura,
                enchanted: theirs
            }
        ]
    );
    state.assert_consistent();
}

#[test]
fn pacifism_is_cast_at_sorcery_speed() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "plains", 2);
    let lions = put_on_battlefield(&mut state, me, "savannah_lions");
    let pacifism = put_in_hand(&mut state, opponent, "pacifism");
    act(&mut state, me, Action::PassPriority);
    assert_eq!(legal_actions(&state, opponent), [Action::PassPriority]);
    act(&mut state, opponent, Action::PassPriority);

    attack(&mut state, &[]);
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert!(legal_actions(&state, opponent).contains(&cast_at(pacifism, at(lions))));
}

#[test]
fn a_pacified_creature_cannot_attack() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    pacify(&mut state, me, giant);

    pass_until_other_decision(&mut state);
    // My own turn has no attackers; pass on to the opponent's.
    assert_eq!(state.turn, 2);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareAttackers { player: opponent })
    );
    assert_eq!(
        legal_actions(&state, opponent),
        [Action::Attack { creature: bears }, Action::ConfirmAttackers]
    );
    assert!(apply_action(&mut state, opponent, Action::Attack { creature: giant }).is_err());
}

#[test]
fn with_only_a_pacified_creature_there_is_no_attack_to_declare() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    pacify(&mut state, me, giant);
    pass_until(&mut state, 2, Step::PostcombatMain);
    assert_eq!(state.player(me).life, 20);
}

#[test]
fn a_pacified_creature_cannot_block() {
    let (mut state, me, opponent) = empty_board(1);
    let attacker = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    pacify(&mut state, me, giant);

    attack(&mut state, &[attacker]);
    // No block is possible, so the defending player isn't asked.
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, 18);
}

#[test]
fn an_unpacified_creature_next_to_a_pacified_one_can_still_block() {
    let (mut state, me, opponent) = empty_board(1);
    let attacker = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let rats = put_on_battlefield(&mut state, opponent, "typhoid_rats");
    pacify(&mut state, me, giant);
    attack(&mut state, &[attacker]);
    pass_until_other_decision(&mut state);
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::Block {
                blocker: rats,
                attacker
            },
            Action::ConfirmBlockers
        ]
    );
}

#[test]
fn pacifism_does_not_stop_mana_abilities() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "plains", 2);
    let elves = put_on_battlefield(&mut state, me, "llanowar_elves");
    let visionary = put_in_hand(&mut state, me, "elvish_visionary");
    put_lands(&mut state, me, "forest", 1);
    // The opponent pacifies my Elves on their turn.
    let pacifism = put_in_hand(&mut state, opponent, "pacifism");
    attack(&mut state, &[]);
    pass_until(&mut state, 2, Step::PrecombatMain);
    act(&mut state, opponent, cast_at(pacifism, at(elves)));
    pass_both(&mut state);

    pass_until(&mut state, 3, Step::PrecombatMain);
    act(&mut state, me, cast(visionary));
    assert!(state.objects[&elves].tapped);
}

#[test]
fn pacifism_goes_to_its_owners_graveyard_when_the_creature_dies() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let aura = pacify(&mut state, me, bears);
    put_lands(&mut state, me, "mountain", 1);
    let shock = put_in_hand(&mut state, me, "shock");

    act(&mut state, me, cast_at(shock, at(bears)));
    let events = pass_both(&mut state);
    assert!(!state.is_on_battlefield(aura));
    assert_eq!(graveyard(&state, opponent), ["grizzly_bears"]);
    // Shock finishes resolving before state-based actions are checked.
    assert_eq!(graveyard(&state, me), ["shock", "pacifism"]);
    assert!(events.iter().any(|event| matches!(
        event,
        GameEvent::PermanentPutIntoGraveyard { player, permanent, .. }
            if *player == me && *permanent == aura
    )));
    state.assert_consistent();
}

#[test]
fn pacifism_fizzles_if_its_target_dies_in_response() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "plains", 2);
    put_lands(&mut state, me, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let pacifism = put_in_hand(&mut state, opponent, "pacifism");
    let shock = put_in_hand(&mut state, me, "shock");

    // On the opponent's turn they pacify my Bears; I Shock my own Bears in
    // response, so the Aura has nothing to enchant.
    attack(&mut state, &[]);
    pass_until(&mut state, 2, Step::PrecombatMain);
    act(&mut state, opponent, cast_at(pacifism, at(bears)));
    act(&mut state, opponent, Action::PassPriority);
    act(&mut state, me, cast_at(shock, at(bears)));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));

    let events = pass_both(&mut state);
    assert!(matches!(
        &events[..],
        [GameEvent::SpellFizzled { player, .. }] if *player == opponent
    ));
    assert_eq!(graveyard(&state, opponent), ["pacifism"]);
    assert!(
        state
            .battlefield
            .iter()
            .all(|id| state.objects[id].def.as_str() != "pacifism")
    );
}

#[test]
fn pacifism_can_be_countered() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_lands(&mut state, opponent, "island", 3);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let pacifism = put_in_hand(&mut state, me, "pacifism");
    let cancel = put_in_hand(&mut state, opponent, "cancel");
    act(&mut state, me, cast_at(pacifism, at(giant)));
    let spell = state.stack[0].id;
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(cancel, at(spell)));
    pass_both(&mut state);
    assert_eq!(graveyard(&state, me), ["pacifism"]);
    assert!(
        !state
            .is_restricted(giant, crate::cards::Restricted::Attack)
            .unwrap()
    );
}

#[test]
fn pacifism_on_an_attacking_creature_does_not_remove_it_from_combat() {
    // There is no instant-speed way to do this with the starter cards, so
    // attach the Aura directly.
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    let aura = put_on_battlefield(&mut state, opponent, "pacifism");
    state.objects.get_mut(&aura).unwrap().attached_to = Some(bears);
    finish_combat(&mut state);
    assert_eq!(state.player(opponent).life, 18);
    assert!(state.is_on_battlefield(aura));
}
