//! Tests for triggered abilities with targets, for effects that stop for a
//! player's choice (discard, scry), and for the M4a cards built on them:
//! bounce, tap, keywords until end of turn, indestructible and bites.

use super::action::{Action, GameEvent, Target};
use super::ids::{ObjectId, PlayerId};
use super::state::{Decision, DuelState, Step};
use super::test_support::{
    attack, block, cast, cast_at, empty_board, finish_combat, pass_both, pass_until, put_in_hand,
    put_lands, put_on_battlefield,
};
use super::{apply_action, legal_actions, player_view, redact_events};
use crate::cards::Keyword;

fn at(object: ObjectId) -> Target {
    Target::Object { object }
}

fn face(player: PlayerId) -> Target {
    Target::Player { player }
}

fn life(state: &DuelState, player: PlayerId) -> i32 {
    state.player(player).life
}

fn act(state: &mut DuelState, player: PlayerId, action: Action) -> Vec<GameEvent> {
    apply_action(state, player, action).unwrap()
}

fn aim(targets: &[Target]) -> Action {
    Action::ChooseTargets {
        targets: targets.to_vec(),
    }
}

/// The definitions of the cards in a zone list.
fn names<'a>(state: &'a DuelState, zone: &[ObjectId]) -> Vec<&'a str> {
    zone.iter()
        .map(|id| state.objects[id].def.as_str())
        .collect()
}

/// Puts a creature card with an enters-the-battlefield ability in `player`'s
/// hand with the lands for it, casts it and lets it resolve. Returns the
/// permanent.
fn cast_creature(state: &mut DuelState, player: PlayerId, card: &str, land: &str) -> ObjectId {
    put_lands(state, player, land, 5);
    let spell = put_in_hand(state, player, card);
    act(state, player, cast(spell));
    pass_both(state);
    *state.battlefield.last().unwrap()
}

// ---- Triggered abilities with targets ----

#[test]
fn a_triggered_ability_with_a_choice_of_targets_asks_its_controller() {
    let (mut state, me, opponent) = empty_board(1);
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    let theirs = put_on_battlefield(&mut state, opponent, "hill_giant");
    let man_o_war = cast_creature(&mut state, me, "man_o_war", "island");

    // Nothing is on the stack until the target is chosen.
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: me,
            source: man_o_war
        })
    );
    assert!(state.stack.is_empty());
    assert_eq!(
        legal_actions(&state, me),
        [aim(&[at(mine)]), aim(&[at(theirs)]), aim(&[at(man_o_war)])]
    );
    assert_eq!(legal_actions(&state, opponent), []);
    assert!(apply_action(&mut state, me, aim(&[face(opponent)])).is_err());
    assert!(apply_action(&mut state, me, aim(&[])).is_err());

    let events = act(&mut state, me, aim(&[at(theirs)]));
    let ability = state.stack[0].id;
    assert_eq!(
        events,
        [GameEvent::AbilityTriggered {
            player: me,
            source: man_o_war,
            ability
        }]
    );
    assert_eq!(state.stack[0].targets, [at(theirs)]);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert_eq!(
        player_view(&state, me).unwrap().stack[0].targets,
        [at(theirs)]
    );

    let events = pass_both(&mut state);
    let in_hand = state.player(opponent).hand[0];
    assert_eq!(
        events,
        [
            GameEvent::PermanentReturnedToHand {
                player: opponent,
                permanent: theirs,
                card: state.card_ref(in_hand).unwrap()
            },
            GameEvent::AbilityResolved { ability }
        ]
    );
    assert_eq!(names(&state, &state.player(opponent).hand), ["hill_giant"]);
    assert!(state.is_on_battlefield(mine) && state.is_on_battlefield(man_o_war));
    state.assert_consistent();
}

#[test]
fn with_one_legal_target_it_is_taken_without_asking() {
    // Man-o'-War alone has to return itself.
    let (mut state, me, _) = empty_board(1);
    let man_o_war = cast_creature(&mut state, me, "man_o_war", "island");
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert_eq!(state.stack[0].targets, [at(man_o_war)]);
    pass_both(&mut state);
    assert_eq!(names(&state, &state.player(me).hand), ["man_o_war"]);
    assert!(
        state
            .battlefield
            .iter()
            .all(|id| state.def(*id).unwrap().is_land())
    );
}

#[test]
fn with_no_legal_target_the_ability_does_not_go_on_the_stack() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    // The opponent's only creature has hexproof.
    put_on_battlefield(&mut state, opponent, "gladecover_scout");
    cast_creature(&mut state, me, "ravenous_chupacabra", "swamp");
    assert!(state.stack.is_empty());
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
}

#[test]
fn ravenous_chupacabra_destroys_an_opponents_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let rats = put_on_battlefield(&mut state, opponent, "typhoid_rats");
    cast_creature(&mut state, me, "ravenous_chupacabra", "swamp");
    // Only the opponent's creatures can be chosen.
    assert_eq!(
        legal_actions(&state, me),
        [aim(&[at(giant)]), aim(&[at(rats)])]
    );
    act(&mut state, me, aim(&[at(giant)]));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(giant));
    assert!(state.is_on_battlefield(rats));
    assert_eq!(
        names(&state, &state.player(opponent).graveyard),
        ["hill_giant"]
    );
}

#[test]
fn a_triggered_ability_whose_target_has_gone_does_nothing() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, opponent, "island", 1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let unsummon = put_in_hand(&mut state, opponent, "unsummon");
    let chupacabra = cast_creature(&mut state, me, "ravenous_chupacabra", "swamp");
    let ability = state.stack[0].id;

    // In response the opponent returns the Giant to their hand.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(unsummon, at(giant)));
    pass_both(&mut state);
    assert_eq!(names(&state, &state.player(opponent).hand), ["hill_giant"]);

    let events = pass_both(&mut state);
    assert_eq!(events, [GameEvent::AbilityResolved { ability }]);
    assert!(state.is_on_battlefield(chupacabra));
    assert!(state.player(opponent).graveyard.len() == 1);
    state.assert_consistent();
}

#[test]
fn the_opponents_triggered_ability_asks_the_opponent() {
    let (mut state, me, opponent) = empty_board(1);
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    let other = put_on_battlefield(&mut state, me, "hill_giant");
    put_lands(&mut state, opponent, "forest", 4);
    let alpha = put_in_hand(&mut state, opponent, "briarpack_alpha");
    // Briarpack Alpha has flash: the opponent casts it in my main phase.
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast(alpha));
    pass_both(&mut state);
    let alpha = *state.battlefield.last().unwrap();
    assert_eq!(
        state.decision,
        Some(Decision::ChooseTargets {
            player: opponent,
            source: alpha
        })
    );
    assert_eq!(legal_actions(&state, me), []);
    assert_eq!(
        legal_actions(&state, opponent),
        [aim(&[at(mine)]), aim(&[at(other)]), aim(&[at(alpha)])]
    );
    act(&mut state, opponent, aim(&[at(alpha)]));
    // The active player gets priority first, as always.
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    pass_both(&mut state);
    assert_eq!(
        (state.power(alpha).unwrap(), state.toughness(alpha).unwrap()),
        (5, 5)
    );
}

#[test]
fn frost_lynx_taps_a_creature_and_keeps_it_tapped_through_one_untap_step() {
    let (mut state, me, opponent) = empty_board(1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let mine = put_on_battlefield(&mut state, me, "grizzly_bears");
    cast_creature(&mut state, me, "frost_lynx", "island");
    // The only creature an opponent controls is taken without asking.
    assert_eq!(state.stack[0].targets, [at(giant)]);
    let events = pass_both(&mut state);
    assert!(events.contains(&GameEvent::PermanentTapped { permanent: giant }));
    assert!(state.objects[&giant].tapped && !state.objects[&mine].tapped);

    // It stays tapped through its controller's next untap step...
    pass_until(&mut state, 2, Step::PrecombatMain);
    assert!(state.objects[&giant].tapped);
    assert!(!state.objects[&giant].skip_untap);
    // ...and untaps in the one after.
    pass_until(&mut state, 4, Step::Upkeep);
    assert!(!state.objects[&giant].tapped);
}

#[test]
fn skymarch_bloodletter_drains_the_opponent_for_1() {
    let (mut state, me, opponent) = empty_board(1);
    cast_creature(&mut state, me, "skymarch_bloodletter", "swamp");
    // "Target opponent" is no choice in a duel.
    assert_eq!(state.stack[0].targets, [face(opponent)]);
    pass_both(&mut state);
    assert_eq!((life(&state, me), life(&state, opponent)), (21, 19));
}

// ---- Discard ----

#[test]
fn burglar_rat_makes_the_opponent_choose_a_card_to_discard() {
    let (mut state, me, opponent) = empty_board(1);
    let bears = put_in_hand(&mut state, opponent, "grizzly_bears");
    let shock = put_in_hand(&mut state, opponent, "shock");
    cast_creature(&mut state, me, "burglar_rat", "swamp");
    let ability = state.stack[0].id;

    pass_both(&mut state);
    assert_eq!(
        state.decision,
        Some(Decision::Discard {
            player: opponent,
            remaining: 1
        })
    );
    assert_eq!(
        legal_actions(&state, opponent),
        [
            Action::Discard { card: bears },
            Action::Discard { card: shock }
        ]
    );
    assert_eq!(legal_actions(&state, me), []);

    let events = act(&mut state, opponent, Action::Discard { card: shock });
    assert!(matches!(events[0], GameEvent::CardDiscarded { player, .. } if player == opponent));
    assert_eq!(events[1], GameEvent::AbilityResolved { ability });
    assert_eq!(
        names(&state, &state.player(opponent).hand),
        ["grizzly_bears"]
    );
    assert_eq!(names(&state, &state.player(opponent).graveyard), ["shock"]);
    // Back to my main phase: the turn has not ended.
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert_eq!((state.turn, state.step), (1, Step::PrecombatMain));
    assert!(state.resolving.is_none());
    state.assert_consistent();
}

#[test]
fn a_discard_with_no_choice_in_it_is_not_asked() {
    let (mut state, me, opponent) = empty_board(1);
    put_in_hand(&mut state, opponent, "shock");
    cast_creature(&mut state, me, "burglar_rat", "swamp");
    pass_both(&mut state);
    assert!(state.player(opponent).hand.is_empty());
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));

    // With an empty hand nothing happens at all.
    let (mut state, me, opponent) = empty_board(2);
    cast_creature(&mut state, me, "burglar_rat", "swamp");
    let events = pass_both(&mut state);
    assert_eq!(events.len(), 1);
    assert!(state.player(opponent).graveyard.is_empty());
}

#[test]
fn mind_rot_makes_the_chosen_player_discard_two_cards() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "swamp", 3);
    let rot = put_in_hand(&mut state, me, "mind_rot");
    let cards = [
        put_in_hand(&mut state, opponent, "grizzly_bears"),
        put_in_hand(&mut state, opponent, "shock"),
        put_in_hand(&mut state, opponent, "hill_giant"),
    ];
    assert!(legal_actions(&state, me).contains(&cast_at(rot, face(me))));
    act(&mut state, me, cast_at(rot, face(opponent)));
    pass_both(&mut state);

    // The spell is still resolving while the opponent chooses.
    assert_eq!(state.stack.len(), 1);
    assert!(state.player(me).graveyard.is_empty());
    assert_eq!(
        state.decision,
        Some(Decision::Discard {
            player: opponent,
            remaining: 2
        })
    );
    act(&mut state, opponent, Action::Discard { card: cards[0] });
    assert_eq!(
        state.decision,
        Some(Decision::Discard {
            player: opponent,
            remaining: 1
        })
    );
    let events = act(&mut state, opponent, Action::Discard { card: cards[2] });
    assert!(matches!(
        events.last(),
        Some(GameEvent::SpellResolved { player, .. }) if *player == me
    ));
    assert!(state.stack.is_empty());
    assert_eq!(names(&state, &state.player(opponent).hand), ["shock"]);
    assert_eq!(
        names(&state, &state.player(opponent).graveyard),
        ["grizzly_bears", "hill_giant"]
    );
    assert_eq!(names(&state, &state.player(me).graveyard), ["mind_rot"]);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    state.assert_consistent();
}

// ---- Scry ----

#[test]
fn opt_shows_the_top_card_to_its_caster_only_and_then_draws() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "island", 1);
    let opt = put_in_hand(&mut state, me, "opt");
    let top = *state.player(me).library.last().unwrap();
    let below = state.player(me).library[state.player(me).library.len() - 2];
    let below_card = state.objects[&below].card;
    act(&mut state, me, cast(opt));
    pass_both(&mut state);

    assert_eq!(
        state.decision,
        Some(Decision::Scry {
            player: me,
            kept: 0,
            remaining: 1,
            surveil: false
        })
    );
    assert_eq!(
        legal_actions(&state, me),
        [Action::ScryKeep, Action::ScryBottom]
    );
    assert_eq!(
        player_view(&state, me).unwrap().looking_at,
        Some(state.card_ref(top).unwrap())
    );
    assert_eq!(player_view(&state, opponent).unwrap().looking_at, None);

    // On the bottom it goes, and the card below it is drawn.
    let events = act(&mut state, me, Action::ScryBottom);
    assert_eq!(
        events[0],
        GameEvent::CardPutOnBottom {
            player: me,
            card: state.card_ref(top).unwrap()
        }
    );
    assert!(matches!(
        redact_events(&events, opponent)[0],
        GameEvent::CardPutOnBottom { ref card, .. } if card.face.is_none()
    ));
    assert_eq!(state.player(me).library[0], top);
    assert_eq!(state.player(me).hand.len(), 1);
    assert_eq!(state.objects[&state.player(me).hand[0]].card, below_card);
    assert!(!state.player(me).library.contains(&below));
    assert_eq!(names(&state, &state.player(me).graveyard), ["opt"]);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert_eq!(player_view(&state, me).unwrap().looking_at, None);
    state.assert_consistent();
}

#[test]
fn a_card_kept_on_top_by_a_scry_is_the_next_one_drawn() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 1);
    let opt = put_in_hand(&mut state, me, "opt");
    let top = *state.player(me).library.last().unwrap();
    let card = state.objects[&top].card;
    let size = state.player(me).library.len();
    act(&mut state, me, cast(opt));
    pass_both(&mut state);
    let events = act(&mut state, me, Action::ScryKeep);
    assert_eq!(events[0], GameEvent::CardKeptOnTop { player: me });
    assert_eq!(state.player(me).library.len(), size - 1);
    assert_eq!(state.objects[&state.player(me).hand[0]].card, card);
}

#[test]
fn scrying_an_empty_library_is_skipped() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "island", 1);
    let opt = put_in_hand(&mut state, me, "opt");
    for card in state.player(me).library.clone() {
        state
            .move_object(card, super::state::Zone::Graveyard)
            .unwrap();
    }
    act(&mut state, me, cast(opt));
    let events = pass_both(&mut state);
    // No scry; the draw is fatigue.
    assert!(events.contains(&GameEvent::Fatigue {
        player: me,
        life_lost: 1
    }));
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
}

// ---- Indestructible and keywords until end of turn ----

#[test]
fn sheltering_light_saves_a_creature_from_destruction_and_from_damage() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 1);
    put_lands(&mut state, opponent, "swamp", 3);
    put_lands(&mut state, opponent, "mountain", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let light = put_in_hand(&mut state, me, "sheltering_light");
    let murder = put_in_hand(&mut state, opponent, "murder");
    let shock = put_in_hand(&mut state, opponent, "shock");

    let events_before = act(&mut state, me, cast_at(light, at(bears)));
    assert_eq!(events_before.len(), 2);
    let events = pass_both(&mut state);
    assert_eq!(
        events,
        [GameEvent::KeywordGranted {
            permanent: bears,
            keyword: Keyword::Indestructible
        }]
    );
    // Then the scry.
    assert!(matches!(state.decision, Some(Decision::Scry { player, .. }) if player == me));
    act(&mut state, me, Action::ScryKeep);
    assert!(
        player_view(&state, me)
            .unwrap()
            .battlefield
            .iter()
            .any(|permanent| {
                permanent.object == bears && permanent.keywords == [Keyword::Indestructible]
            })
    );

    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(murder, at(bears)));
    pass_both(&mut state);
    assert!(state.is_on_battlefield(bears));
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(shock, at(bears)));
    pass_both(&mut state);
    assert!(state.is_on_battlefield(bears));
    assert_eq!(state.objects[&bears].damage, 2);

    // It wears off at end of turn, along with the damage.
    pass_until(&mut state, 2, Step::Upkeep);
    assert!(state.is_on_battlefield(bears));
    assert!(!state.has_keyword(bears, Keyword::Indestructible).unwrap());
}

#[test]
fn an_indestructible_creature_still_dies_to_toughness_0() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 1);
    put_lands(&mut state, opponent, "swamp", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let light = put_in_hand(&mut state, me, "sheltering_light");
    let disfigure = put_in_hand(&mut state, opponent, "disfigure");
    act(&mut state, me, cast_at(light, at(bears)));
    pass_both(&mut state);
    act(&mut state, me, Action::ScryKeep);
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(disfigure, at(bears)));
    pass_both(&mut state);
    assert!(!state.is_on_battlefield(bears));
}

#[test]
fn sure_strike_gives_plus_3_and_first_strike_to_one_creature() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let strike = put_in_hand(&mut state, me, "sure_strike");

    // One target, though two effects use it.
    let casts: Vec<Action> = legal_actions(&state, me)
        .into_iter()
        .filter(|action| matches!(action, Action::CastSpell { .. }))
        .collect();
    assert_eq!(
        casts,
        [cast_at(strike, at(bears)), cast_at(strike, at(giant))]
    );

    attack(&mut state, &[bears]);
    block(&mut state, &[(giant, bears)]);
    act(&mut state, me, cast_at(strike, at(bears)));
    pass_both(&mut state);
    assert_eq!(state.power(bears).unwrap(), 5);
    assert!(state.has_keyword(bears, Keyword::FirstStrike).unwrap());
    finish_combat(&mut state);
    // The 5/2 first striker kills the 3/3 before it deals damage.
    assert!(state.is_on_battlefield(bears));
    assert!(!state.is_on_battlefield(giant));

    pass_until(&mut state, 2, Step::Upkeep);
    assert_eq!(state.power(bears).unwrap(), 2);
    assert!(!state.has_keyword(bears, Keyword::FirstStrike).unwrap());
}

// ---- Bounce and bite ----

#[test]
fn unsummon_returns_a_creature_to_its_owners_hand_and_its_aura_falls_off() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "plains", 2);
    put_lands(&mut state, me, "island", 1);
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let pacifism = put_in_hand(&mut state, me, "pacifism");
    let unsummon = put_in_hand(&mut state, me, "unsummon");
    act(&mut state, me, cast_at(pacifism, at(giant)));
    pass_both(&mut state);
    act(&mut state, me, cast_at(unsummon, at(giant)));
    pass_both(&mut state);
    assert_eq!(names(&state, &state.player(opponent).hand), ["hill_giant"]);
    assert_eq!(
        names(&state, &state.player(me).graveyard),
        ["unsummon", "pacifism"]
    );
    state.assert_consistent();
}

#[test]
fn rabid_bite_has_your_creature_deal_its_power_to_one_of_theirs() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let baloth = put_on_battlefield(&mut state, me, "rumbling_baloth");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let bite = put_in_hand(&mut state, me, "rabid_bite");
    let casts: Vec<Action> = legal_actions(&state, me)
        .into_iter()
        .filter(|action| matches!(action, Action::CastSpell { .. }))
        .collect();
    let both = |mine: ObjectId| Action::CastSpell {
        card: bite,
        targets: vec![at(mine), at(giant)],
    };
    assert_eq!(casts, [both(bears), both(baloth)]);

    act(&mut state, me, both(baloth));
    let events = pass_both(&mut state);
    assert_eq!(
        events[0],
        GameEvent::DamageDealt {
            source: baloth,
            target: at(giant),
            amount: 4
        }
    );
    // The Giant deals nothing back.
    assert!(!state.is_on_battlefield(giant));
    assert_eq!(state.objects[&baloth].damage, 0);
}

#[test]
fn rabid_bite_does_nothing_if_the_biter_has_gone() {
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "forest", 2);
    put_lands(&mut state, opponent, "island", 1);
    let bears = put_on_battlefield(&mut state, me, "grizzly_bears");
    let giant = put_on_battlefield(&mut state, opponent, "hill_giant");
    let bite = put_in_hand(&mut state, me, "rabid_bite");
    let unsummon = put_in_hand(&mut state, opponent, "unsummon");
    act(
        &mut state,
        me,
        Action::CastSpell {
            card: bite,
            targets: vec![at(bears), at(giant)],
        },
    );
    act(&mut state, me, Action::PassPriority);
    act(&mut state, opponent, cast_at(unsummon, at(bears)));
    pass_both(&mut state);
    let events = pass_both(&mut state);
    // One of its two targets is still legal, so the spell resolves, but
    // with nothing to do.
    assert!(matches!(events[..], [GameEvent::SpellResolved { .. }]));
    assert_eq!(state.objects[&giant].damage, 0);
}
