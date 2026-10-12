//! The searching AI: that it keeps to what its player can see, that it is
//! deterministic and leaves the duel alone, and positions where the right
//! play is known.

use super::*;
use crate::cards::CardDefId;
use crate::duel::test_support::{
    cast, cast_at, config, empty_board, empty_hand, pass_until, put_in_hand, put_lands,
    put_on_battlefield,
};
use crate::duel::{CardInstance, DuelRng, Target, Zone, apply_action, new_duel};

fn old(state: &DuelState, player: PlayerId) -> Action {
    ai_choose_action_with(state, player, AiKind::Heuristic).unwrap()
}

fn new(state: &DuelState, player: PlayerId) -> Action {
    ai_choose_action_with(state, player, AiKind::Search).unwrap()
}

/// Whether the searching AI has something to think about here, rather than
/// a decision it leaves to version 1.
fn is_searched(state: &DuelState, player: PlayerId) -> bool {
    match state.decision {
        Some(Decision::Priority { .. }) => {
            !crate::duel::should_auto_pass(state, player, false)
                && legal_actions(state, player).iter().any(|action| {
                    matches!(
                        action,
                        Action::CastSpell { .. } | Action::ActivateAbility { .. }
                    )
                })
        }
        Some(Decision::ChooseTargets { .. }) => legal_actions(state, player).len() > 1,
        _ => false,
    }
}

/// Plays a duel with `kind` on both sides. `check` sees the state before
/// each decision the search thinks about, and after `checks` of them the
/// duel is left where it is (0 plays it to its end).
fn play_with(
    kind: AiKind,
    decks: (&str, &str),
    seed: u64,
    checks: usize,
    mut check: impl FnMut(&DuelState, PlayerId),
) -> DuelState {
    let mut state = new_duel(config(decks.0, decks.1), seed).unwrap();
    let mut checked = 0;
    for _ in 0..50_000 {
        let Some(player) = state.waiting_on() else {
            return state;
        };
        if checks > 0 && is_searched(&state, player) {
            if checked == checks {
                return state;
            }
            checked += 1;
            check(&state, player);
        }
        let action = ai_choose_action_with(&state, player, kind).unwrap();
        apply_action(&mut state, player, action).unwrap();
    }
    panic!("{decks:?}, seed {seed}: no result by turn {}", state.turn);
}

#[test]
fn the_hash_behind_the_ais_dice_is_the_published_one() {
    // The FNV-1a test vectors.
    assert_eq!(search::fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(search::fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(search::fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn whole_games_with_the_search_on_both_sides_end_and_repeat_exactly() {
    let first = play_with(AiKind::Search, ("deck_b", "deck_d"), 3, 0, |_, _| {});
    assert!(first.result.is_some());
    first.assert_consistent();
    let again = play_with(AiKind::Search, ("deck_b", "deck_d"), 3, 0, |_, _| {});
    assert_eq!(again, first);
    // It doesn't simply play version 1's game.
    let by_old = play_with(AiKind::Heuristic, ("deck_b", "deck_d"), 3, 0, |_, _| {});
    assert_ne!(by_old, first);
}

#[test]
fn thinking_leaves_the_duel_untouched_and_comes_to_the_same_answer_each_time() {
    let mut thought_about = 0;
    play_with(
        AiKind::Search,
        ("deck_c", "deck_e"),
        4,
        20,
        |state, player| {
            let before = state.clone();
            let answer = new(state, player);
            // Not one field differs, the duel's dice included.
            assert_eq!(*state, before);
            assert_eq!(new(state, player), answer);
            thought_about += 1;
        },
    );
    assert_eq!(thought_about, 20);
}

/// A copy of the game in which everything `player` can't see is somewhere
/// else: the opponent's unseen cards are dealt out again between their hand
/// and library, the player's own library is in another order, and the
/// duel's dice are different ones.
fn with_the_unseen_rearranged(state: &DuelState, player: PlayerId, seed: u64) -> DuelState {
    let mut other = state.clone();
    let mut dice = DuelRng::from_seed(seed);
    let opponent = other.player(player.opponent());
    let unseen: Vec<ObjectId> = opponent
        .hand
        .iter()
        .chain(&opponent.library)
        .copied()
        .collect();
    other.redeal_cards(&unseen, &mut dice).unwrap();
    let library = other.player(player).library.clone();
    other.redeal_cards(&library, &mut dice).unwrap();
    other.rng = DuelRng::from_seed(seed);
    other
}

#[test]
fn the_search_decides_from_what_its_player_can_see_and_nothing_else() {
    for (decks, kind) in [
        (("deck_b", "deck_a"), AiKind::SearchKnowing),
        (("deck_g", "deck_d"), AiKind::SearchGuessing),
    ] {
        let mut decisions = 0;
        let mut moved = 0;
        play_with(kind, decks, 6, 20, |state, player| {
            decisions += 1;
            let other = with_the_unseen_rearranged(state, player, decisions);
            moved += u64::from(other != *state);
            assert_eq!(
                ai_choose_action_with(&other, player, kind).unwrap(),
                ai_choose_action_with(state, player, kind).unwrap(),
                "{decks:?}, decision {decisions}"
            );
        });
        assert!(decisions >= 15, "only {decisions} decisions");
        assert!(
            moved + 2 >= decisions,
            "the unseen cards moved only {moved} times"
        );
    }
}

#[test]
fn without_the_decklist_it_cannot_tell_what_the_opponents_unseen_cards_are() {
    // Every card the opponent hasn't shown becomes a Swamp. An AI that
    // doesn't know their decklist must decide the same.
    let mut decisions = 0;
    play_with(
        AiKind::SearchGuessing,
        ("deck_c", "deck_f"),
        2,
        20,
        |state, player| {
            decisions += 1;
            let mut blind = state.clone();
            let opponent = state.player(player.opponent());
            for id in opponent.hand.iter().chain(&opponent.library) {
                blind.objects.get_mut(id).unwrap().def = CardDefId("swamp".to_string());
            }
            assert_eq!(
                ai_choose_action_with(&blind, player, AiKind::SearchGuessing).unwrap(),
                ai_choose_action_with(state, player, AiKind::SearchGuessing).unwrap(),
                "decision {decisions}"
            );
        },
    );
    assert!(decisions >= 15, "only {decisions} decisions");
}

#[test]
fn it_passes_where_a_human_player_would_not_be_asked() {
    // In its own upkeep, holding an instant it could cast.
    let (mut state, me, opponent) = empty_board(1);
    put_lands(&mut state, me, "mountain", 2);
    put_on_battlefield(&mut state, opponent, "grizzly_bears");
    put_in_hand(&mut state, me, "shock");
    pass_until(&mut state, 3, Step::Upkeep);
    assert_eq!(state.decision, Some(Decision::Priority { player: me }));
    assert!(legal_actions(&state, me).len() > 1);
    assert_eq!(new(&state, me), Action::PassPriority);
}

#[test]
fn it_casts_raise_dead_for_a_creature_worth_having_back() {
    let (mut state, me, _) = empty_board(1);
    put_lands(&mut state, me, "swamp", 5);
    let nighthawk = put_in_hand(&mut state, me, "vampire_nighthawk");
    let nighthawk = state.move_object(nighthawk, Zone::Graveyard).unwrap();
    // A card for a card: version 1 sees nothing in it.
    let raise_dead = put_in_hand(&mut state, me, "raise_dead");
    let action = cast_at(raise_dead, Target::Object { object: nighthawk });
    assert!(legal_actions(&state, me).contains(&action));
    assert_eq!(old(&state, me), Action::PassPriority);
    assert_eq!(new(&state, me), action);
}

#[test]
fn it_casts_its_commander() {
    let mut config = config("deck_a", "deck_c");
    for player in &mut config.players {
        player.commander = Some(CardInstance::new(CardDefId("jasmine_boreal".to_string())));
    }
    let mut state = new_duel(config, 1).unwrap();
    let me = state.starting_player;
    apply_action(&mut state, me, Action::Keep).unwrap();
    apply_action(&mut state, me.opponent(), Action::Keep).unwrap();
    pass_until(&mut state, 1, Step::PrecombatMain);
    empty_hand(&mut state, me);
    empty_hand(&mut state, me.opponent());
    put_lands(&mut state, me, "plains", 1);
    put_lands(&mut state, me, "forest", 4);
    let commander = state.player(me).command[0];
    assert_eq!(new(&state, me), cast(commander));
}
