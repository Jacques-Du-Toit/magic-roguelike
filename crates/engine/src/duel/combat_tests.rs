//! Tests for combat and its keywords.

use super::action::{Action, BlockDeclared, GameEvent, Target};
use super::error::ActionError;
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, GameResult, Step, WinReason, Zone};
use super::test_support::{
    attack, block, cast, empty_board, finish_combat, pass_both, pass_until,
    pass_until_other_decision, put_in_hand, put_on_battlefield,
};
use super::{apply_action, legal_actions, result};

fn life(state: &DuelState, player: PlayerId) -> i32 {
    state.player(player).life
}

/// Whether a permanent has left the battlefield.
fn gone(state: &DuelState, permanent: ObjectId) -> bool {
    !state.is_on_battlefield(permanent)
}

fn steps(events: &[GameEvent]) -> Vec<Step> {
    events
        .iter()
        .filter_map(|event| match event {
            GameEvent::StepBegan { step } => Some(*step),
            _ => None,
        })
        .collect()
}

/// Every `(source, target, amount)` of damage in the events, in order.
fn damage(events: &[GameEvent]) -> Vec<(ObjectId, Target, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameEvent::DamageDealt {
                source,
                target,
                amount,
            } => Some((*source, *target, *amount)),
            _ => None,
        })
        .collect()
}

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

// ---- Attacking ----

#[test]
fn an_unblocked_attacker_damages_the_defending_player() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");

    let events = attack(&mut state, &[bears]);
    assert!(events.contains(&GameEvent::AttackersDeclared {
        player: me,
        attackers: vec![bears]
    }));
    assert!(events.contains(&GameEvent::PermanentTapped { permanent: bears }));
    assert!(state.objects[&bears].tapped);
    assert_eq!(state.step, Step::DeclareAttackers);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));

    let events = finish_combat(&mut state);
    assert_eq!(damage(&events), [(bears, face(opponent), 2)]);
    assert_eq!(life(&state, opponent), 18);
    assert_eq!(life(&state, me), 20);
    assert!(events.contains(&GameEvent::LifeChanged {
        player: opponent,
        change: -2,
        life: 18
    }));
    // Nothing could block, so no first-strike step and no block decision.
    assert_eq!(
        steps(&events),
        [
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain
        ]
    );
    assert_eq!(state.combat, None);
}

#[test]
fn only_ready_creatures_of_the_active_player_can_attack() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let tapped = put_on_battlefield(&mut state, me, "hill_giant");
    state.objects.get_mut(&tapped).unwrap().tapped = true;
    let sick = put_on_battlefield(&mut state, me, "centaur_courser");
    state.objects.get_mut(&sick).unwrap().summoning_sick = true;
    put_on_battlefield(&mut state, me, "forest");
    put_on_battlefield(&mut state, opponent, "grizzly_bears");

    pass_until_other_decision(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareAttackers { player: me })
    );
    assert_eq!(
        legal_actions(&state, me),
        [Action::Attack { creature: bears }, Action::ConfirmAttackers]
    );
    assert_eq!(legal_actions(&state, opponent), []);
    for creature in [tapped, sick] {
        assert_eq!(
            apply_action(&mut state, me, Action::Attack { creature }),
            Err(ActionError::IllegalAction {
                player: me,
                action: Action::Attack { creature }
            })
        );
    }
}

#[test]
fn a_creature_cannot_be_declared_as_an_attacker_twice() {
    let (mut state, me, _) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until_other_decision(&mut state);
    apply_action(&mut state, me, Action::Attack { creature: bears }).unwrap();
    assert_eq!(
        legal_actions(&state, me),
        [
            Action::RemoveAttacker { creature: bears },
            Action::ResetDeclaration,
            Action::ConfirmAttackers
        ]
    );
}

#[test]
fn one_attacker_can_be_taken_back_out_of_the_declaration() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    pass_until_other_decision(&mut state);
    // Nothing is declared yet, so nothing can be removed.
    assert!(apply_action(&mut state, me, Action::RemoveAttacker { creature: bears }).is_err());
    apply_action(&mut state, me, Action::Attack { creature: bears }).unwrap();
    apply_action(&mut state, me, Action::Attack { creature: giant }).unwrap();

    let events = apply_action(&mut state, me, Action::RemoveAttacker { creature: bears }).unwrap();
    assert_eq!(events, []);
    assert_eq!(
        legal_actions(&state, me),
        [
            Action::Attack { creature: bears },
            Action::RemoveAttacker { creature: giant },
            Action::ResetDeclaration,
            Action::ConfirmAttackers
        ]
    );

    // Only the giant attacks.
    apply_action(&mut state, me, Action::ConfirmAttackers).unwrap();
    assert!(state.objects[&giant].tapped && !state.objects[&bears].tapped);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 17);
}

#[test]
fn the_attack_declaration_can_be_reset() {
    let (mut state, me, _) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    pass_until_other_decision(&mut state);
    apply_action(&mut state, me, Action::Attack { creature: bears }).unwrap();
    apply_action(&mut state, me, Action::Attack { creature: giant }).unwrap();

    let events = apply_action(&mut state, me, Action::ResetDeclaration).unwrap();
    assert_eq!(events, []);
    assert_eq!(
        legal_actions(&state, me),
        [
            Action::Attack { creature: bears },
            Action::Attack { creature: giant },
            Action::ConfirmAttackers
        ]
    );
    assert!(!state.objects[&bears].tapped);
}

#[test]
fn attacking_with_nothing_skips_to_the_end_of_combat() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    pass_until_other_decision(&mut state);
    let events = apply_action(&mut state, me, Action::ConfirmAttackers).unwrap();
    assert_eq!(
        events,
        [GameEvent::StepBegan {
            step: Step::EndCombat
        }]
    );
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 20);
}

#[test]
fn a_creature_cast_this_turn_cannot_attack() {
    let (mut state, me, _) = empty_board(1);
    put_on_battlefield(&mut state, me, "forest");
    put_on_battlefield(&mut state, me, "forest");
    let bears = put_in_hand(&mut state, me, "grizzly_bears");
    apply_action(&mut state, me, cast(bears)).unwrap();
    pass_both(&mut state);
    // With no possible attacker there is no declaration at all.
    let events = finish_combat(&mut state);
    assert_eq!(
        steps(&events),
        [
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::EndCombat,
            Step::PostcombatMain
        ]
    );
}

#[test]
fn attackers_untap_in_their_controllers_next_untap_step() {
    let (mut state, me, _) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    pass_until(&mut state, 2, Step::Upkeep);
    assert!(state.objects[&bears].tapped);
    // The opponent attacks with nothing on their turn.
    pass_until(&mut state, 3, Step::Upkeep);
    assert!(!state.objects[&bears].tapped);
}

#[test]
fn lethal_combat_damage_ends_the_game() {
    let (mut state, me, opponent) = empty_board(1);
    state.player_mut(opponent).life = 2;
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[bears]);
    let events = pass_until_other_decision(&mut state);
    let expected = GameResult::Win {
        player: me,
        reason: WinReason::LifeZero,
    };
    assert_eq!(result(&state), Some(expected));
    assert_eq!(
        events.last(),
        Some(&GameEvent::GameOver { result: expected })
    );
    assert_eq!(state.decision, None);
}

// ---- Blocking ----

#[test]
fn a_blocked_attacker_and_its_blocker_damage_each_other() {
    let (mut state, me, opponent) = empty_board(1);
    let attacker = put_on_battlefield(&mut state, me, "grizzly_bears");
    let blocker = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let (attacker_card, blocker_card) =
        (state.objects[&attacker].card, state.objects[&blocker].card);

    attack(&mut state, &[attacker]);
    let events = block(&mut state, &[(blocker, attacker)]);
    assert_eq!(
        events.last(),
        Some(&GameEvent::BlockersDeclared {
            player: opponent,
            blocks: vec![BlockDeclared {
                attacker,
                blockers: vec![blocker]
            }]
        })
    );
    // The attacking player gets priority after blocks.
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));

    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events),
        [(attacker, at(blocker), 2), (blocker, at(attacker), 2)]
    );
    assert!(gone(&state, attacker) && gone(&state, blocker));
    assert_eq!((life(&state, me), life(&state, opponent)), (20, 20));

    let in_graveyard = |player: PlayerId| {
        let id = state.player(player).graveyard[0];
        (state.objects[&id].card, state.objects[&id].zone)
    };
    assert_eq!(in_graveyard(me), (attacker_card, Zone::Graveyard));
    assert_eq!(in_graveyard(opponent), (blocker_card, Zone::Graveyard));
    let died: Vec<ObjectId> = events
        .iter()
        .filter_map(|event| match event {
            GameEvent::CreatureDied { permanent, .. } => Some(*permanent),
            _ => None,
        })
        .collect();
    assert_eq!(died, [attacker, blocker]);
    state.assert_consistent();
}

#[test]
fn damage_that_is_not_lethal_wears_off_at_cleanup() {
    let (mut state, me, opponent) = empty_board(1);
    let courser = put_on_battlefield(&mut state, me, "centaur_courser");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[courser]);
    block(&mut state, &[(bears, courser)]);
    finish_combat(&mut state);
    assert!(gone(&state, bears));
    assert_eq!(state.objects[&courser].damage, 2);
    assert_eq!(life(&state, opponent), 20);

    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(state.objects[&courser].damage, 0);
}

#[test]
fn the_defending_player_chooses_blocks_among_untapped_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    let stays_home = put_on_battlefield(&mut state, me, "centaur_courser");
    let blocker = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let tapped = put_on_battlefield(&mut state, opponent, "hill_giant");
    state.objects.get_mut(&tapped).unwrap().tapped = true;

    attack(&mut state, &[bears, giant]);
    pass_until_other_decision(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::DeclareBlockers { player: opponent })
    );
    assert_eq!(legal_actions(&state, me), []);
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::Block {
                blocker,
                attacker: bears
            },
            Action::Block {
                blocker,
                attacker: giant
            },
            Action::ConfirmBlockers
        ]
    );
    for (blocker, attacker) in [(tapped, bears), (blocker, stays_home), (bears, giant)] {
        assert!(apply_action(&mut state, opponent, Action::Block { blocker, attacker }).is_err());
    }

    // A creature blocks only one attacker.
    apply_action(
        &mut state,
        opponent,
        Action::Block {
            blocker,
            attacker: giant,
        },
    )
    .unwrap();
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::RemoveBlocker { blocker },
            Action::ResetDeclaration,
            Action::ConfirmBlockers
        ]
    );
    apply_action(&mut state, opponent, Action::ResetDeclaration).unwrap();
    assert_eq!(legal_actions(&state, opponent).len(), 3);

    // One block can be taken back on its own, and made differently.
    let block = |attacker| Action::Block { blocker, attacker };
    assert!(apply_action(&mut state, opponent, Action::RemoveBlocker { blocker }).is_err());
    apply_action(&mut state, opponent, block(giant)).unwrap();
    let events = apply_action(&mut state, opponent, Action::RemoveBlocker { blocker }).unwrap();
    assert_eq!(events, []);
    assert_eq!(
        legal_actions(&state, opponent),
        [block(bears), block(giant), Action::ConfirmBlockers]
    );

    // Not blocking at all is allowed.
    apply_action(&mut state, opponent, Action::ConfirmBlockers).unwrap();
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 15);
}

#[test]
fn damage_is_assigned_lethal_to_each_blocker_then_the_rest_to_the_last() {
    let (mut state, me, opponent) = empty_board(1);
    let courser = put_on_battlefield(&mut state, me, "centaur_courser");
    let lions = put_on_battlefield(&mut state, opponent, "savannah_lions");
    let elves = put_on_battlefield(&mut state, opponent, "llanowar_elves");
    attack(&mut state, &[courser]);
    block(&mut state, &[(lions, courser), (elves, courser)]);
    let events = finish_combat(&mut state);
    // Lions is a 2/1: 1 is lethal. The other 2 go to the last blocker.
    assert_eq!(
        damage(&events),
        [
            (courser, at(lions), 1),
            (courser, at(elves), 2),
            (lions, at(courser), 2),
            (elves, at(courser), 1),
        ]
    );
    assert!(gone(&state, lions) && gone(&state, elves) && gone(&state, courser));
    assert_eq!(life(&state, opponent), 20);
}

#[test]
fn an_attacker_too_small_to_kill_every_blocker_kills_the_first() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    attack(&mut state, &[giant]);
    block(&mut state, &[(bears, giant), (spider, giant)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events)[..2],
        [(giant, at(bears), 2), (giant, at(spider), 1)]
    );
    assert!(gone(&state, bears) && gone(&state, giant));
    assert_eq!(state.objects[&spider].damage, 1);
}

#[test]
fn excess_damage_without_trample_stays_on_the_blocker() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    let lions = put_on_battlefield(&mut state, opponent, "savannah_lions");
    attack(&mut state, &[giant]);
    block(&mut state, &[(lions, giant)]);
    let events = finish_combat(&mut state);
    assert_eq!(damage(&events)[0], (giant, at(lions), 3));
    assert_eq!(life(&state, opponent), 20);
    assert_eq!(state.objects[&giant].damage, 2);
}

// ---- Vigilance, haste, flying, reach, menace ----

#[test]
fn vigilance_serra_angel_attacks_without_tapping() {
    let (mut state, me, opponent) = empty_board(1);
    let angel = put_on_battlefield(&mut state, me, "serra_angel");
    let events = attack(&mut state, &[angel]);
    assert!(!state.objects[&angel].tapped);
    assert!(!events.contains(&GameEvent::PermanentTapped { permanent: angel }));
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 16);
}

#[test]
fn haste_raging_goblin_attacks_the_turn_it_is_cast() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "mountain");
    let goblin = put_in_hand(&mut state, me, "raging_goblin");
    apply_action(&mut state, me, cast(goblin)).unwrap();
    pass_both(&mut state);
    let goblin = *state.battlefield.last().unwrap();
    assert!(state.objects[&goblin].summoning_sick);

    attack(&mut state, &[goblin]);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 19);
}

#[test]
fn flying_creatures_are_blocked_only_by_flying_or_reach() {
    let (mut state, me, opponent) = empty_board(1);
    let drake = put_on_battlefield(&mut state, me, "wind_drake");
    let walker = put_on_battlefield(&mut state, me, "grizzly_bears");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    let flyer = put_on_battlefield(&mut state, opponent, "inspiring_overseer");

    attack(&mut state, &[drake, walker]);
    pass_until_other_decision(&mut state);
    let blocks: Vec<(ObjectId, ObjectId)> = legal_actions(&state, opponent)
        .into_iter()
        .filter_map(|action| match action {
            Action::Block { blocker, attacker } => Some((blocker, attacker)),
            _ => None,
        })
        .collect();
    // Bears can't block the Drake; a flyer can block a ground creature.
    assert_eq!(
        blocks,
        [
            (bears, walker),
            (spider, drake),
            (spider, walker),
            (flyer, drake),
            (flyer, walker)
        ]
    );
}

#[test]
fn reach_giant_spider_blocks_and_kills_a_flyer() {
    let (mut state, me, opponent) = empty_board(1);
    let drake = put_on_battlefield(&mut state, me, "wind_drake");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    attack(&mut state, &[drake]);
    block(&mut state, &[(spider, drake)]);
    finish_combat(&mut state);
    assert!(gone(&state, drake));
    assert_eq!(state.objects[&spider].damage, 2);
}

#[test]
fn an_unblockable_in_practice_flyer_gets_no_block_decision() {
    let (mut state, me, opponent) = empty_board(1);
    let drake = put_on_battlefield(&mut state, me, "wind_drake");
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[drake]);
    let events = finish_combat(&mut state);
    assert!(events.contains(&GameEvent::BlockersDeclared {
        player: opponent,
        blocks: vec![]
    }));
    assert_eq!(life(&state, opponent), 18);
}

#[test]
fn menace_boggart_brute_cannot_be_blocked_by_one_creature() {
    let (mut state, me, opponent) = empty_board(1);
    let brute = put_on_battlefield(&mut state, me, "boggart_brute");
    let first = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    let second = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[brute]);
    pass_until_other_decision(&mut state);

    // No blockers is fine.
    assert!(legal_actions(&state, opponent).contains(&Action::ConfirmBlockers));
    let block_with = |blocker| Action::Block {
        blocker,
        attacker: brute,
    };
    apply_action(&mut state, opponent, block_with(first)).unwrap();
    assert_eq!(
        legal_actions(&state, opponent),
        [
            block_with(second),
            Action::RemoveBlocker { blocker: first },
            Action::ResetDeclaration
        ]
    );
    assert_eq!(
        apply_action(&mut state, opponent, Action::ConfirmBlockers),
        Err(ActionError::IllegalAction {
            player: opponent,
            action: Action::ConfirmBlockers
        })
    );

    apply_action(&mut state, opponent, block_with(second)).unwrap();
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::RemoveBlocker { blocker: first },
            Action::RemoveBlocker { blocker: second },
            Action::ResetDeclaration,
            Action::ConfirmBlockers
        ]
    );
    apply_action(&mut state, opponent, Action::ConfirmBlockers).unwrap();
    let events = finish_combat(&mut state);
    // The 3/2 Brute kills the first Bears and dies to the pair.
    assert_eq!(
        damage(&events)[..2],
        [(brute, at(first), 2), (brute, at(second), 1)]
    );
    assert!(gone(&state, brute) && gone(&state, first) && !gone(&state, second));
}

#[test]
fn a_lone_blocker_must_let_a_menace_creature_through() {
    let (mut state, me, opponent) = empty_board(1);
    let brute = put_on_battlefield(&mut state, me, "boggart_brute");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[brute]);
    pass_until_other_decision(&mut state);
    apply_action(
        &mut state,
        opponent,
        Action::Block {
            blocker: bears,
            attacker: brute,
        },
    )
    .unwrap();
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::RemoveBlocker { blocker: bears },
            Action::ResetDeclaration
        ]
    );
    apply_action(&mut state, opponent, Action::ResetDeclaration).unwrap();
    apply_action(&mut state, opponent, Action::ConfirmBlockers).unwrap();
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 17);
}

// ---- First strike ----

#[test]
fn first_strike_youthful_knight_kills_before_it_can_be_hit() {
    let (mut state, me, opponent) = empty_board(1);
    let knight = put_on_battlefield(&mut state, me, "youthful_knight");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[knight]);
    block(&mut state, &[(bears, knight)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        steps(&events),
        [
            Step::FirstStrikeDamage,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain
        ]
    );
    assert_eq!(damage(&events), [(knight, at(bears), 2)]);
    assert!(gone(&state, bears));
    assert_eq!(state.objects[&knight].damage, 0);
}

#[test]
fn a_first_strike_blocker_kills_the_attacker_first() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let knight = put_on_battlefield(&mut state, opponent, "youthful_knight");
    attack(&mut state, &[bears]);
    block(&mut state, &[(knight, bears)]);
    let events = finish_combat(&mut state);
    assert_eq!(damage(&events), [(knight, at(bears), 2)]);
    assert!(gone(&state, bears) && !gone(&state, knight));
}

#[test]
fn a_creature_that_survives_first_strike_hits_back() {
    let (mut state, me, opponent) = empty_board(1);
    let knight = put_on_battlefield(&mut state, me, "youthful_knight");
    let courser = put_on_battlefield(&mut state, opponent, "centaur_courser");
    attack(&mut state, &[knight]);
    block(&mut state, &[(courser, knight)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events),
        [(knight, at(courser), 2), (courser, at(knight), 3)]
    );
    assert!(gone(&state, knight));
    assert_eq!(state.objects[&courser].damage, 2);
}

#[test]
fn first_strikers_deal_damage_once_and_others_in_the_regular_step() {
    let (mut state, me, opponent) = empty_board(1);
    let knight = put_on_battlefield(&mut state, me, "youthful_knight");
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    attack(&mut state, &[knight, bears]);

    pass_until(&mut state, 1, Step::FirstStrikeDamage);
    assert_eq!(life(&state, opponent), 18);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    let events = finish_combat(&mut state);
    assert_eq!(damage(&events), [(bears, face(opponent), 2)]);
    assert_eq!(life(&state, opponent), 16);
}

#[test]
fn without_first_strike_there_is_one_damage_step() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let blocker = put_on_battlefield(&mut state, opponent, "hill_giant");
    attack(&mut state, &[bears]);
    block(&mut state, &[(blocker, bears)]);
    let events = finish_combat(&mut state);
    assert!(!steps(&events).contains(&Step::FirstStrikeDamage));
}

// ---- Trample ----

#[test]
fn trample_colossal_dreadmaw_tramples_over_a_blocker() {
    let (mut state, me, opponent) = empty_board(1);
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let bears = put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[dreadmaw]);
    block(&mut state, &[(bears, dreadmaw)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events),
        [
            (dreadmaw, at(bears), 2),
            (dreadmaw, face(opponent), 4),
            (bears, at(dreadmaw), 2)
        ]
    );
    assert_eq!(life(&state, opponent), 16);
    assert!(gone(&state, bears));
}

#[test]
fn trample_assigns_lethal_damage_to_every_blocker_first() {
    let (mut state, me, opponent) = empty_board(1);
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let lions = put_on_battlefield(&mut state, opponent, "savannah_lions");
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    attack(&mut state, &[dreadmaw]);
    block(&mut state, &[(lions, dreadmaw), (spider, dreadmaw)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events)[..3],
        [
            (dreadmaw, at(lions), 1),
            (dreadmaw, at(spider), 4),
            (dreadmaw, face(opponent), 1)
        ]
    );
    assert_eq!(life(&state, opponent), 19);
    assert!(gone(&state, lions) && gone(&state, spider));
    assert_eq!(state.objects[&dreadmaw].damage, 4);
}

// ---- Deathtouch ----

#[test]
fn deathtouch_typhoid_rats_kills_whatever_it_damages() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, me, "hill_giant");
    let rats = put_on_battlefield(&mut state, opponent, "typhoid_rats");
    attack(&mut state, &[giant]);
    block(&mut state, &[(rats, giant)]);
    finish_combat(&mut state);
    assert!(gone(&state, giant) && gone(&state, rats));
}

#[test]
fn deathtouch_damage_does_not_linger_on_a_survivor_of_the_check() {
    // The flag is only about damage dealt since the last check: a creature
    // that somehow survives is not killed again later.
    let (mut state, me, opponent) = empty_board(1);
    let rats = put_on_battlefield(&mut state, me, "typhoid_rats");
    attack(&mut state, &[rats]);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 19);
    assert!(
        state
            .objects
            .values()
            .all(|object| !object.deathtouch_damage)
    );
}

#[test]
fn deathtouch_needs_only_one_damage_per_blocker() {
    let (mut state, me, opponent) = empty_board(1);
    let nighthawk = put_on_battlefield(&mut state, me, "vampire_nighthawk");
    // The Nighthawk flies, so the blockers need reach or flying.
    let spider = put_on_battlefield(&mut state, opponent, "giant_spider");
    let angel = put_on_battlefield(&mut state, opponent, "serra_angel");
    attack(&mut state, &[nighthawk]);
    block(&mut state, &[(spider, nighthawk), (angel, nighthawk)]);
    let events = finish_combat(&mut state);
    assert_eq!(
        damage(&events)[..2],
        [(nighthawk, at(spider), 1), (nighthawk, at(angel), 1)]
    );
    assert!(gone(&state, spider) && gone(&state, angel) && gone(&state, nighthawk));
}

#[test]
fn trample_over_a_deathtouch_blocker_and_die_to_it() {
    let (mut state, me, opponent) = empty_board(1);
    let dreadmaw = put_on_battlefield(&mut state, me, "colossal_dreadmaw");
    let rats = put_on_battlefield(&mut state, opponent, "typhoid_rats");
    attack(&mut state, &[dreadmaw]);
    block(&mut state, &[(rats, dreadmaw)]);
    finish_combat(&mut state);
    assert_eq!(life(&state, opponent), 15);
    assert!(gone(&state, dreadmaw) && gone(&state, rats));
}

// ---- Lifelink ----

#[test]
fn lifelink_healers_hawk_gains_life_for_damage_to_a_player() {
    let (mut state, me, opponent) = empty_board(1);
    let hawk = put_on_battlefield(&mut state, me, "healers_hawk");
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    attack(&mut state, &[hawk]);
    let events = finish_combat(&mut state);
    assert_eq!((life(&state, me), life(&state, opponent)), (21, 19));
    assert!(events.contains(&GameEvent::LifeChanged {
        player: me,
        change: 1,
        life: 21
    }));
}

#[test]
fn lifelink_counts_damage_dealt_to_creatures() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let nighthawk = put_on_battlefield(&mut state, opponent, "vampire_nighthawk");
    attack(&mut state, &[bears]);
    block(&mut state, &[(nighthawk, bears)]);
    finish_combat(&mut state);
    // The 2/3 Nighthawk kills the Bears with deathtouch and survives.
    assert_eq!((life(&state, me), life(&state, opponent)), (20, 22));
    assert!(gone(&state, bears));
    assert_eq!(state.objects[&nighthawk].damage, 2);
}

#[test]
fn vampire_nighthawk_flies_over_ground_blockers() {
    let (mut state, me, opponent) = empty_board(1);
    let nighthawk = put_on_battlefield(&mut state, me, "vampire_nighthawk");
    put_on_battlefield(&mut state, opponent, "hill_giant");
    attack(&mut state, &[nighthawk]);
    finish_combat(&mut state);
    assert_eq!((life(&state, me), life(&state, opponent)), (22, 18));
}
