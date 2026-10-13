//! Tests for the run's rules.

use super::*;
use crate::ai_choose_action;
use crate::cards::CardDefId;
use crate::data;
use crate::duel::test_support::{cast, empty_hand, pass_both, pass_until, put_in_hand, put_lands};
use crate::duel::{
    Action, Decision, DuelRng, DuelState, GameEvent, GameResult, PlayerId, Step, WinReason,
    apply_action,
};

fn run(planeswalker: &str, seed: u64) -> RunState {
    new_run(
        RunConfig {
            planeswalker: PlaneswalkerId(planeswalker.to_string()),
            emblem: None,
        },
        seed,
    )
    .unwrap()
}

/// Applies a run action. Travelling to a duel also picks the first feats
/// on offer, so that the duel has started: most tests are about what comes
/// after. The tests of the feats themselves use `apply_run_action`.
fn act(state: &mut RunState, action: RunAction) -> Vec<RunEvent> {
    let mut events = apply_run_action(state, action).unwrap();
    if matches!(state.phase, Phase::Feats { .. }) {
        let first = run_legal_actions(state).remove(0);
        events.extend(apply_run_action(state, first).unwrap());
    }
    events
}

fn travels(state: &RunState) -> Vec<NodeId> {
    run_legal_actions(state)
        .into_iter()
        .filter_map(|action| match action {
            RunAction::Travel { node } => Some(node),
            _ => None,
        })
        .collect()
}

fn kind_of(state: &RunState, node: NodeId) -> NodeKind {
    state.map.node(node).unwrap().kind
}

/// Ends the duel in progress with the given result, as if it had been
/// played out.
fn force_result(state: &mut RunState, result: GameResult) {
    let duel = state.duel.as_mut().unwrap();
    duel.result = Some(result);
    duel.decision = None;
}

fn win() -> GameResult {
    GameResult::Win {
        player: PLAYER,
        reason: WinReason::LifeZero,
    }
}

fn loss() -> GameResult {
    GameResult::Win {
        player: ENEMY,
        reason: WinReason::LifeZero,
    }
}

/// Makes the duel in progress one in which no feat scores: long, costly
/// and never ahead.
fn scrape(state: &mut RunState) {
    let stats = state.duel_stats.as_mut().unwrap();
    (stats.turns, stats.lowest_life, stats.creatures_lost) = (99, 1, 99);
    stats.high_points.clear();
}

/// Finishes the duel in progress with the given result, as a scrape: its
/// feats score nothing, so a win pays what a win pays at the least.
fn finish(state: &mut RunState, result: GameResult) -> Vec<RunEvent> {
    scrape(state);
    force_result(state, result);
    act(state, RunAction::FinishDuel)
}

fn feat(id: &str) -> FeatId {
    FeatId(id.to_string())
}

/// Plays the duel in progress to its end with the AI on both sides.
fn play_duel(state: &mut RunState) -> Vec<GameEvent> {
    let mut log = Vec::new();
    for _ in 0..50_000 {
        let duel = state.duel.as_ref().unwrap();
        let Some(player) = duel.waiting_on() else {
            return log;
        };
        let action = ai_choose_action(duel, player).unwrap();
        log.extend(apply_duel_action(state, player, action).unwrap());
    }
    panic!("the duel did not end");
}

/// Walks the map until a node of kind `wanted` is on offer, and returns it
/// without going there. Duels on the way are won, rewards skipped, other
/// nodes left, and a treasure's gold taken.
fn walk_to(state: &mut RunState, wanted: NodeKind) -> Option<NodeId> {
    for _ in 0..40 {
        match state.phase.clone() {
            Phase::Map => {
                let options = travels(state);
                let found = options
                    .iter()
                    .copied()
                    .find(|node| kind_of(state, *node) == wanted);
                if found.is_some() {
                    return found;
                }
                act(state, RunAction::Travel { node: options[0] });
            }
            // `act` has picked the feats already.
            Phase::Feats { .. } | Phase::Duel { .. } => {
                finish(state, win());
            }
            Phase::Reward { .. } => {
                act(state, RunAction::SkipReward);
            }
            Phase::Rest | Phase::Shop { .. } | Phase::Draft { .. } | Phase::Event { .. } => {
                act(state, RunAction::Leave);
            }
            Phase::Treasure { .. } => {
                act(state, RunAction::TakeGold);
            }
            Phase::Over { .. } => return None,
        }
    }
    None
}

/// Walks the map, taking a node of kind `stop_at` as soon as one is on
/// offer, and stops having travelled there.
fn travel_to(state: &mut RunState, stop_at: NodeKind) -> bool {
    match walk_to(state, stop_at) {
        Some(node) => {
            act(state, RunAction::Travel { node });
            true
        }
        None => false,
    }
}

/// A run with a node of the wanted kind on offer, and that node. Tries
/// seeds until the map allows it.
fn run_before(planeswalker: &str, wanted: NodeKind) -> (RunState, NodeId) {
    for seed in 0..200 {
        let mut state = run(planeswalker, seed);
        if let Some(node) = walk_to(&mut state, wanted) {
            return (state, node);
        }
    }
    panic!("no seed leads to a {wanted:?} node");
}

/// A run that has just arrived at a node of the wanted kind. Tries seeds
/// until the map allows it.
fn run_at(planeswalker: &str, stop_at: NodeKind) -> RunState {
    for seed in 0..200 {
        let mut state = run(planeswalker, seed);
        if travel_to(&mut state, stop_at) {
            return state;
        }
    }
    panic!("no seed leads to a {stop_at:?} node");
}

/// The one wound there is.
fn wound() -> ModifierId {
    ModifierId("festering_wound".to_string())
}

fn max_wounds() -> usize {
    usize::from(data::embedded().unwrap().balance.run.max_wounds)
}

fn deck_count(state: &RunState, card: &str) -> usize {
    state
        .deck
        .iter()
        .filter(|instance| instance.card_id.as_str() == card)
        .count()
}

// ---- Starting a run ----

#[test]
fn a_new_run_has_the_starting_deck_no_wounds_and_gold() {
    let balance = &data::embedded().unwrap().balance.run;
    let state = run("warden", 1);
    assert_eq!(state.gold, balance.starting_gold);
    assert_eq!(state.deck.len(), 10);
    assert!(state.relics.is_empty() && state.wounds.is_empty());
    assert_eq!(run_view(&state).unwrap().max_wounds, balance.max_wounds);
    assert_eq!(state.phase, Phase::Map);
    assert_eq!(state.position(), None);
    assert!(state.duel.is_none());

    let view = run_view(&state).unwrap();
    assert_eq!(view.seed, "1");
    let lands: u32 = view.basic_lands.iter().map(|entry| entry.count).sum();
    assert_eq!(lands, 7);
    assert_eq!(view.actions, run_legal_actions(&state));
    assert_eq!(view.actions.len(), 3);
}

#[test]
fn an_unknown_planeswalker_is_an_error() {
    let config = RunConfig {
        planeswalker: PlaneswalkerId("nobody".to_string()),
        emblem: None,
    };
    assert_eq!(
        new_run(config, 1),
        Err(RunError::UnknownPlaneswalker(PlaneswalkerId(
            "nobody".to_string()
        )))
    );
}

#[test]
fn a_run_starts_with_no_emblem_and_an_unknown_one_is_refused() {
    // There are no starting emblems for now, so nobody has a usual one.
    assert!(data::embedded().unwrap().run.emblems.is_empty());
    let mut state = run("warden", 3);
    assert_eq!(state.emblem, None);
    assert_eq!(run("firebrand", 1).emblem, None);
    assert_eq!(run_view(&state).unwrap().emblem, None);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    assert!(state.duel.as_ref().unwrap().emblems.is_empty());

    let config = |emblem: &ModifierId| RunConfig {
        planeswalker: PlaneswalkerId("warden".to_string()),
        emblem: Some(emblem.clone()),
    };
    let nothing = ModifierId("nothing".to_string());
    assert_eq!(
        new_run(config(&nothing), 3),
        Err(RunError::UnknownEmblem(nothing))
    );
    // A relic is not a starting emblem.
    let relic = data::embedded().unwrap().run.relics[0].id.clone();
    assert_eq!(
        new_run(config(&relic), 3),
        Err(RunError::UnknownEmblem(relic))
    );
}

#[test]
fn the_same_seed_gives_the_same_run_and_another_seed_another() {
    assert_eq!(run("warden", 5), run("warden", 5));
    assert_ne!(run("warden", 5).map, run("warden", 6).map);
}

// ---- The map ----

#[test]
fn travel_starts_in_the_first_row_and_follows_the_edges() {
    let mut state = run("warden", 3);
    let first_row: Vec<NodeId> = state
        .map
        .nodes
        .iter()
        .filter(|node| node.row == 0)
        .map(|node| node.id)
        .collect();
    assert_eq!(travels(&state), first_row);

    let start = first_row[1];
    let events = act(&mut state, RunAction::Travel { node: start });
    assert_eq!(
        events[0],
        RunEvent::Travelled {
            node: start,
            kind: NodeKind::Duel
        }
    );
    assert_eq!(state.position(), Some(start));
    finish(&mut state, win());
    act(&mut state, RunAction::SkipReward);
    assert_eq!(travels(&state), state.map.node(start).unwrap().next);

    // A node that isn't next is refused, and nothing changes.
    let before = state.clone();
    let elsewhere = RunAction::Travel { node: first_row[0] };
    assert_eq!(
        apply_run_action(&mut state, elsewhere.clone()),
        Err(RunError::IllegalAction(elsewhere))
    );
    assert_eq!(state, before);
}

// ---- Duels ----

#[test]
fn a_duel_node_starts_a_duel_set_up_from_the_run() {
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    let events = act(&mut state, RunAction::Travel { node });
    let Phase::Duel { enemy, kind } = state.phase.clone() else {
        panic!("expected a duel, got {:?}", state.phase);
    };
    assert_eq!(kind, DuelKind::Normal);
    // The feats were offered first, and `act` picked two.
    assert_eq!(
        events[1..],
        [
            RunEvent::FeatsOffered {
                enemy: enemy.clone(),
                kind
            },
            RunEvent::DuelStarted {
                enemy: enemy.clone(),
                kind
            }
        ]
    );
    let enemy = data::embedded().unwrap().run.enemy(&enemy).unwrap();
    assert_eq!(enemy.tier, EnemyTier::Easy);
    assert_eq!(current_enemy(&state), Some(&enemy.id));

    let duel = state.duel.as_ref().unwrap();
    // The player's 10 spells and 7 lands, no emblem, and the commander
    // waiting in the command zone.
    let player = duel.player(PLAYER);
    assert_eq!(player.hand.len() + player.library.len(), 17);
    assert!(duel.emblems.is_empty());
    assert_eq!(player.command.len(), 1);
    assert_eq!(
        duel.objects[&player.command[0]].def.as_str(),
        "jasmine_boreal"
    );
    // An easy enemy has no commander.
    assert!(duel.player(ENEMY).command.is_empty());

    // While it is being played the run only offers to concede.
    assert_eq!(run_legal_actions(&state), [RunAction::Concede]);
    assert_eq!(
        apply_run_action(&mut state, RunAction::FinishDuel),
        Err(RunError::IllegalAction(RunAction::FinishDuel))
    );
}

#[test]
fn duel_actions_go_through_the_run() {
    let mut state = run("firebrand", 4);
    assert_eq!(
        apply_duel_action(&mut state, PLAYER, crate::duel::Action::Keep),
        Err(RunError::NoDuel)
    );
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let first = state.duel.as_ref().unwrap().starting_player;
    let events = apply_duel_action(&mut state, first, crate::duel::Action::Keep).unwrap();
    assert_eq!(events, [GameEvent::HandKept { player: first }]);
    assert!(matches!(
        apply_duel_action(&mut state, first, crate::duel::Action::Keep),
        Err(RunError::Duel(_))
    ));

    // The duel's stats start with it and follow it.
    let stats = state.duel_stats.clone().unwrap();
    assert_eq!(
        (stats.player, stats.turns, stats.lowest_life),
        (PLAYER, 0, 20)
    );
    play_duel(&mut state);
    let duel = state.duel.as_ref().unwrap();
    assert!(duel.result.is_some());
    assert_eq!(run_legal_actions(&state), [RunAction::FinishDuel]);
    let stats = state.duel_stats.as_ref().unwrap();
    assert!(stats.turns >= 2, "{stats:?}");
    assert_eq!(stats.high_points.len(), stats.turns as usize + 1);
    assert!(stats.lowest_life <= duel.player(PLAYER).life);
    assert!(stats.value(Measure::MostCreatures, None) >= 1);
}

#[test]
fn winning_a_duel_pays_gold_and_offers_cards() {
    let gold = &data::embedded().unwrap().balance.run.gold;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let before = state.gold;
    let events = finish(&mut state, win());
    assert_eq!(state.gold, before + gold.duel_win);
    assert!(matches!(
        events[0],
        RunEvent::DuelEnded {
            outcome: DuelOutcome::Won,
            kind: DuelKind::Normal,
            ..
        }
    ));
    assert_eq!(
        events[1],
        RunEvent::GoldChanged {
            change: i64::from(gold.duel_win),
            gold: state.gold,
            reason: GoldReason::DuelWon
        }
    );
    // Its feats scored nothing, which is still reported.
    assert!(matches!(events[2], RunEvent::FeatsScored { points: 0, .. }));
    assert_eq!(events.len(), 3);
    assert!(state.wounds.is_empty());
    assert!(state.duel.is_none() && state.duel_stats.is_none() && state.feats.is_empty());

    // A win with no points offers the fewest cards.
    let Phase::Reward { choices } = state.phase.clone() else {
        panic!("expected a reward, got {:?}", state.phase);
    };
    assert_eq!(choices.len(), 2);
    assert_eq!(
        run_legal_actions(&state),
        [
            RunAction::TakeReward { choice: 0 },
            RunAction::TakeReward { choice: 1 },
            RunAction::SkipReward,
        ]
    );
}

// ---- Feats ----

/// A run that has arrived at its first duel and is choosing feats, with
/// the feats on offer.
fn choosing_feats(planeswalker: &str) -> (RunState, Vec<FeatId>) {
    let mut state = run(planeswalker, 3);
    let node = travels(&state)[0];
    apply_run_action(&mut state, RunAction::Travel { node }).unwrap();
    let Phase::Feats { offered, .. } = state.phase.clone() else {
        panic!("expected feats, got {:?}", state.phase);
    };
    (state, offered)
}

fn choose(feats: &[usize]) -> RunAction {
    RunAction::ChooseFeats {
        feats: feats.to_vec(),
    }
}

#[test]
fn a_duel_offers_five_feats_of_the_characters_and_two_are_picked() {
    let data = data::embedded().unwrap();
    let (mut state, offered) = choosing_feats("warden");
    assert!(state.duel.is_none());
    assert_eq!(offered.len(), 5);
    // Different ones, in the pool's order, all of them the Warden's.
    let places: Vec<usize> = offered
        .iter()
        .map(|id| data.run.feats.iter().position(|def| def.id == *id).unwrap())
        .collect();
    assert!(
        places.windows(2).all(|pair| pair[0] < pair[1]),
        "{places:?}"
    );
    assert_eq!(data.run.feats_for(&state.planeswalker).count(), 9);

    // Every pair, and nothing else: the duel can't be put off or dodged.
    let actions = run_legal_actions(&state);
    assert_eq!(actions.len(), 10);
    assert_eq!(actions[0], choose(&[0, 1]));
    assert_eq!(actions[9], choose(&[3, 4]));
    for refused in [
        choose(&[1, 0]),
        choose(&[2]),
        choose(&[0, 5]),
        choose(&[0, 1, 2]),
    ] {
        assert_eq!(
            apply_run_action(&mut state, refused.clone()),
            Err(RunError::IllegalAction(refused))
        );
    }

    let events = apply_run_action(&mut state, choose(&[1, 3])).unwrap();
    assert!(matches!(events[..], [RunEvent::DuelStarted { .. }]));
    assert!(matches!(state.phase, Phase::Duel { .. }));
    assert_eq!(state.feats, [offered[1].clone(), offered[3].clone()]);
    // The view says how they stand.
    let standing = run_view(&state).unwrap().feats;
    assert_eq!(standing.len(), 2);
    assert_eq!(standing[0].feat, offered[1]);

    // Another seed offers another five.
    let differs = (4..20).any(|seed| {
        let mut other = run("warden", seed);
        let node = travels(&other)[0];
        apply_run_action(&mut other, RunAction::Travel { node }).unwrap();
        !matches!(&other.phase, Phase::Feats { offered: theirs, .. } if *theirs == offered)
    });
    assert!(differs);
}

#[test]
fn a_character_without_feats_of_its_own_is_offered_the_shared_ones() {
    let data = data::embedded().unwrap();
    let (_, offered) = choosing_feats("firebrand");
    let shared: Vec<FeatId> = data
        .run
        .feats
        .iter()
        .filter(|def| def.planeswalkers.is_empty())
        .map(|def| def.id.clone())
        .collect();
    // Five of the six.
    assert_eq!(shared.len(), 6);
    assert_eq!(offered.len(), 5);
    assert!(offered.iter().all(|id| shared.contains(id)));
}

/// Wins the duel in progress having picked `feats`, with the stats set by
/// `change`, and returns what happened and how many cards are on offer.
fn win_with(feats: &[&str], change: impl FnOnce(&mut DuelStats)) -> (Vec<RunEvent>, usize) {
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    scrape(&mut state);
    state.feats = feats.iter().map(|id| feat(id)).collect();
    change(state.duel_stats.as_mut().unwrap());
    force_result(&mut state, win());
    let events = act(&mut state, RunAction::FinishDuel);
    let Phase::Reward { choices } = &state.phase else {
        panic!("expected a reward, got {:?}", state.phase);
    };
    (events, choices.len())
}

#[test]
fn the_feats_picked_are_scored_on_a_win_and_set_the_reward() {
    let gold_for_feats = |events: &[RunEvent]| {
        events.iter().find_map(|event| match event {
            RunEvent::GoldChanged {
                change,
                reason: GoldReason::Feats,
                ..
            } => Some(*change),
            _ => None,
        })
    };
    let score = |id: &str, score: u8, value: i32| FeatScore {
        feat: feat(id),
        score,
        value,
    };

    // Never below 16 life is two tiers of Unbroken; eight turns is one of
    // Swift. Three points: three cards, and a little gold.
    let (events, cards) = win_with(&["unbroken", "swift"], |stats| {
        (stats.lowest_life, stats.turns) = (16, 8);
    });
    assert!(events.contains(&RunEvent::FeatsScored {
        scores: vec![score("unbroken", 2, 16), score("swift", 1, 8)],
        points: 3
    }));
    assert_eq!((cards, gold_for_feats(&events)), (3, Some(15)));

    // One point: three cards and no more.
    let (events, cards) = win_with(&["unbroken", "swift"], |stats| stats.turns = 9);
    assert_eq!((cards, gold_for_feats(&events)), (3, None));

    // Everything: four cards and more gold.
    let (events, cards) = win_with(&["unbroken", "unbowed"], |stats| {
        (stats.lowest_life, stats.creatures_lost) = (20, 0);
    });
    assert!(events.contains(&RunEvent::FeatsScored {
        scores: vec![score("unbroken", 3, 20), score("unbowed", 3, 0)],
        points: 6
    }));
    assert_eq!((cards, gold_for_feats(&events)), (4, Some(30)));

    // Only the feats picked count: a duel that would have scored on
    // others scores nothing.
    let (events, cards) = win_with(&["legion", "adorned"], |stats| {
        (stats.lowest_life, stats.turns) = (20, 4);
    });
    assert!(events.contains(&RunEvent::FeatsScored {
        scores: vec![score("legion", 0, 0), score("adorned", 0, 0)],
        points: 0
    }));
    assert_eq!((cards, gold_for_feats(&events)), (2, None));
}

#[test]
fn the_points_make_the_cards_rarer() {
    // Six points take a duel's reward a step up the ladder, to the
    // elite's weights: far fewer commons over many rewards.
    let data = data::embedded().unwrap();
    let commons = |points: bool| {
        let mut commons = 0;
        for seed in 0..60 {
            let mut state = run("warden", seed);
            let node = travels(&state)[0];
            act(&mut state, RunAction::Travel { node });
            scrape(&mut state);
            if points {
                state.feats = vec![feat("unbroken"), feat("unbowed")];
                let stats = state.duel_stats.as_mut().unwrap();
                (stats.lowest_life, stats.creatures_lost) = (20, 0);
            }
            force_result(&mut state, win());
            act(&mut state, RunAction::FinishDuel);
            let Phase::Reward { choices } = &state.phase else {
                panic!("expected a reward");
            };
            // The first two cards of each, so that both count the same.
            commons += choices[..2]
                .iter()
                .filter(|id| data.cards.get(id).unwrap().rarity == crate::cards::Rarity::Common)
                .count();
        }
        commons
    };
    let (plain, pointed) = (commons(false), commons(true));
    assert!(pointed + 20 < plain, "{pointed} against {plain}");
}

#[test]
fn a_draw_scores_no_feats() {
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    force_result(&mut state, GameResult::Draw);
    let events = act(&mut state, RunAction::FinishDuel);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RunEvent::FeatsScored { .. }))
    );
    assert!(state.feats.is_empty() && state.duel_stats.is_none());
}

#[test]
fn taking_a_reward_adds_the_card_and_the_lands_follow() {
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    finish(&mut state, win());
    let Phase::Reward { choices } = state.phase.clone() else {
        panic!("expected a reward");
    };
    let events = act(&mut state, RunAction::TakeReward { choice: 1 });
    assert_eq!(
        events,
        [RunEvent::CardAdded {
            card: choices[1].clone()
        }]
    );
    assert_eq!(state.deck.len(), 11);
    assert_eq!(state.deck[10].card_id, choices[1]);
    assert_eq!(state.phase, Phase::Map);

    // 11 spells want 7 lands.
    let view = run_view(&state).unwrap();
    let lands: u32 = view.basic_lands.iter().map(|entry| entry.count).sum();
    assert_eq!(lands, 7);

    // Skipping leaves the deck alone.
    let mut other = run("warden", 3);
    let node = travels(&other)[0];
    act(&mut other, RunAction::Travel { node });
    finish(&mut other, win());
    assert_eq!(act(&mut other, RunAction::SkipReward), []);
    assert_eq!(other.deck.len(), 10);
}

#[test]
fn losing_a_duel_leaves_a_wound_a_little_gold_and_a_smaller_reward() {
    let balance = &data::embedded().unwrap().balance.run;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let before = state.gold;
    let events = finish(&mut state, loss());
    assert_eq!(state.wounds, [wound()]);
    assert_eq!(state.gold, before + balance.gold.duel_loss);
    assert!(events.contains(&RunEvent::WoundTaken { wound: wound() }));
    // Fewer cards to pick from than after a win.
    let Phase::Reward { choices } = state.phase.clone() else {
        panic!("expected a reward, got {:?}", state.phase);
    };
    assert_eq!(choices.len(), usize::from(balance.rewards.loss_choices));
    // No feats are scored for a duel that was lost.
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RunEvent::FeatsScored { .. }))
    );
    // The node is behind us.
    act(&mut state, RunAction::SkipReward);
    assert_eq!(travels(&state), state.map.node(node).unwrap().next);
}

#[test]
fn a_wound_starts_every_duel_on_the_battlefield_until_it_is_healed() {
    let wounds_of = |state: &RunState| {
        let duel = state.duel.as_ref().unwrap();
        duel.permanents_of(PLAYER)
            .filter(|permanent| permanent.def.as_str() == "festering_wound")
            .count()
    };
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    assert_eq!(wounds_of(&state), 0);
    finish(&mut state, loss());
    act(&mut state, RunAction::SkipReward);

    // In the next duel, and the one after: paying it off lasts one duel.
    for _ in 0..2 {
        assert!(travel_to(&mut state, NodeKind::Duel));
        assert_eq!(wounds_of(&state), 1);
        finish(&mut state, win());
        act(&mut state, RunAction::SkipReward);
    }
    state.wounds.clear();
    assert!(travel_to(&mut state, NodeKind::Duel));
    assert_eq!(wounds_of(&state), 0);
}

#[test]
fn conceding_counts_as_a_loss() {
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let events = act(&mut state, RunAction::Concede);
    assert!(matches!(
        events[0],
        RunEvent::DuelEnded {
            outcome: DuelOutcome::Lost,
            ..
        }
    ));
    assert_eq!(state.wounds.len(), 1);
    assert!(state.duel.is_none());
    assert!(matches!(state.phase, Phase::Reward { .. }));
}

#[test]
fn a_drawn_duel_costs_nothing_and_pays_a_little_gold() {
    let gold = &data::embedded().unwrap().balance.run.gold;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let before = state.gold;
    let events = finish(&mut state, GameResult::Draw);
    assert!(state.wounds.is_empty());
    assert_eq!(state.gold, before + gold.draw);
    assert!(matches!(
        events[0],
        RunEvent::DuelEnded {
            outcome: DuelOutcome::Drawn,
            ..
        }
    ));
    assert_eq!(state.phase, Phase::Map);
}

#[test]
fn the_third_wound_ends_the_run() {
    let mut state = run("warden", 3);
    let mut lost = 0;
    while lost < max_wounds() {
        let node = travels(&state)[0];
        act(&mut state, RunAction::Travel { node });
        if state.duel.is_none() {
            // The last action on offer always moves on.
            let leave = run_legal_actions(&state).pop().unwrap();
            act(&mut state, leave);
            continue;
        }
        let events = finish(&mut state, loss());
        lost += 1;
        assert_eq!(state.wounds.len(), lost);
        if lost < max_wounds() {
            act(&mut state, RunAction::SkipReward);
            assert_eq!(state.phase, Phase::Map);
        } else {
            assert_eq!(events.last(), Some(&RunEvent::RunEnded { won: false }));
        }
    }
    assert_eq!(state.phase, Phase::Over { won: false });
    assert_eq!(run_legal_actions(&state), []);
    assert_eq!(
        apply_run_action(&mut state, RunAction::Leave),
        Err(RunError::IllegalAction(RunAction::Leave))
    );
}

#[test]
fn the_player_meets_a_new_enemy_each_time_while_there_are_any() {
    let mut state = run("warden", 3);
    for _ in 0..2 {
        let node = travels(&state)[0];
        act(&mut state, RunAction::Travel { node });
        finish(&mut state, GameResult::Draw);
    }
    assert_eq!(state.fought.len(), 2);
    assert_ne!(state.fought[0], state.fought[1]);
}

// ---- Elites ----

#[test]
fn an_elite_win_gives_a_relic_gold_and_a_card_choice() {
    let data = data::embedded().unwrap();
    let mut state = run_at("warden", NodeKind::Elite);
    let Phase::Duel { enemy, kind } = state.phase.clone() else {
        panic!("expected a duel");
    };
    assert_eq!(kind, DuelKind::Elite);
    assert_eq!(data.run.enemy(&enemy).unwrap().tier, EnemyTier::Elite);

    {
        let duel = state.duel.as_mut().unwrap();
        duel.turn = 99;
        duel.players[PLAYER.index()].life = 1;
    }
    let before = state.gold;
    let events = finish(&mut state, win());
    assert_eq!(state.gold, before + data.balance.run.gold.elite_win);
    assert_eq!(state.relics.len(), 1);
    assert!(events.contains(&RunEvent::RelicGained {
        relic: state.relics[0].clone()
    }));
    assert!(matches!(state.phase, Phase::Reward { .. }));
}

#[test]
fn an_elite_loss_leaves_a_wound_and_no_relic() {
    let data = data::embedded().unwrap();
    let mut state = run_at("warden", NodeKind::Elite);
    let before = state.gold;
    finish(&mut state, loss());
    assert_eq!(state.wounds, [wound()]);
    assert_eq!(state.gold, before + data.balance.run.gold.elite_loss);
    assert!(state.relics.is_empty());
    assert!(matches!(state.phase, Phase::Reward { .. }));
}

#[test]
fn a_relic_shapes_the_next_duel_and_the_gold_relic_pays_per_win() {
    let data = data::embedded().unwrap();
    let relic = |id: &str| ModifierId(id.to_string());
    let mut state = run("firebrand", 3);
    state.relics = vec![
        relic("travellers_rations"),
        relic("war_banner"),
        relic("lucky_coin"),
    ];
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let duel = state.duel.as_ref().unwrap();
    assert_eq!(duel.player(PLAYER).life, 22);
    assert_eq!(duel.player(ENEMY).life, 20);
    let names: Vec<&str> = duel
        .emblems
        .iter()
        .map(|emblem| emblem.def.name.as_str())
        .collect();
    assert_eq!(names, ["War Banner"]);

    {
        let duel = state.duel.as_mut().unwrap();
        duel.turn = 99;
        duel.players[PLAYER.index()].life = 1;
    }
    let before = state.gold;
    let events = finish(&mut state, win());
    assert_eq!(state.gold, before + data.balance.run.gold.duel_win + 10);
    assert!(events.contains(&RunEvent::GoldChanged {
        change: 10,
        gold: state.gold,
        reason: GoldReason::Relic
    }));
}

// ---- Rest sites ----

#[test]
fn a_rest_site_heals_a_wound_removes_a_card_or_is_left() {
    let mut arrived = run_at("warden", NodeKind::Rest);
    assert_eq!(arrived.phase, Phase::Rest);
    // With no wound there is nothing to heal.
    let heal = RunAction::HealWound { wound: 0 };
    assert!(!run_legal_actions(&arrived).contains(&heal));
    assert!(run_legal_actions(&arrived).contains(&RunAction::Leave));
    assert!(run_legal_actions(&arrived).contains(&RunAction::RemoveCard { card: 9 }));
    assert!(!run_legal_actions(&arrived).contains(&RunAction::RemoveCard { card: 10 }));
    arrived.wounds = vec![wound(), wound()];

    let mut healed = arrived.clone();
    assert!(run_legal_actions(&healed).contains(&RunAction::HealWound { wound: 1 }));
    assert!(!run_legal_actions(&healed).contains(&RunAction::HealWound { wound: 2 }));
    let events = act(&mut healed, heal);
    assert_eq!(events, [RunEvent::WoundHealed { wound: wound() }]);
    // One wound a rest.
    assert_eq!(healed.wounds, [wound()]);
    assert_eq!(healed.phase, Phase::Map);

    let mut trimmed = arrived.clone();
    let removed = trimmed.deck[4].card_id.clone();
    let copies = deck_count(&trimmed, removed.as_str());
    let events = act(&mut trimmed, RunAction::RemoveCard { card: 4 });
    assert_eq!(
        events,
        [RunEvent::CardRemoved {
            card: removed.clone()
        }]
    );
    assert_eq!(trimmed.deck.len(), 9);
    assert_eq!(deck_count(&trimmed, removed.as_str()), copies - 1);
    // Only one thing may be done.
    assert_eq!(trimmed.phase, Phase::Map);
    assert_eq!(trimmed.wounds.len(), 2);

    let mut left = arrived.clone();
    assert_eq!(act(&mut left, RunAction::Leave), []);
    assert_eq!(left.phase, Phase::Map);
    assert_eq!((left.wounds.len(), left.deck.len()), (2, 10));
}

#[test]
fn a_small_deck_cannot_lose_more_cards() {
    let data = data::embedded().unwrap();
    let mut state = run_at("warden", NodeKind::Rest);
    state
        .deck
        .truncate(data.balance.run.min_deck_cards as usize);
    assert_eq!(run_legal_actions(&state), [RunAction::Leave]);
    let remove = RunAction::RemoveCard { card: 0 };
    assert_eq!(
        apply_run_action(&mut state, remove.clone()),
        Err(RunError::IllegalAction(remove))
    );
}

// ---- Shops ----

#[test]
fn a_shop_sells_what_the_player_can_afford() {
    let data = data::embedded().unwrap();
    let prices = &data.balance.run.shop.prices;
    let mut state = run_at("firebrand", NodeKind::Shop);
    let Phase::Shop {
        items,
        removal_price,
        removals_left,
    } = state.phase.clone()
    else {
        panic!("expected a shop, got {:?}", state.phase);
    };
    assert_eq!(items.len(), 5);
    assert_eq!(removal_price, prices.removal);
    assert_eq!(removals_left, data.balance.run.shop.removals);

    // With no gold, all one can do is leave.
    state.gold = 0;
    assert_eq!(run_legal_actions(&state), [RunAction::Leave]);
    let buy = RunAction::Buy { item: 0 };
    assert_eq!(
        apply_run_action(&mut state, buy.clone()),
        Err(RunError::IllegalAction(buy))
    );

    // With exactly the price of the first card, that card and anything
    // cheaper.
    state.gold = items[0].price;
    let affordable: Vec<RunAction> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.price <= items[0].price)
        .map(|(item, _)| RunAction::Buy { item })
        .collect();
    let offered: Vec<RunAction> = run_legal_actions(&state)
        .into_iter()
        .filter(|action| matches!(action, RunAction::Buy { .. }))
        .collect();
    assert_eq!(offered, affordable);

    let Ware::Card { card } = items[0].ware.clone() else {
        panic!("expected a card");
    };
    let events = act(&mut state, RunAction::Buy { item: 0 });
    assert_eq!(state.gold, 0);
    assert_eq!(state.deck.last().unwrap().card_id, card);
    assert_eq!(
        events,
        [
            RunEvent::GoldChanged {
                change: -i64::from(items[0].price),
                gold: 0,
                reason: GoldReason::Purchase
            },
            RunEvent::CardAdded { card }
        ]
    );
    // Sold items can't be bought twice.
    state.gold = 1000;
    assert!(!run_legal_actions(&state).contains(&RunAction::Buy { item: 0 }));
    let Phase::Shop { items: after, .. } = &state.phase else {
        panic!("expected to still be in the shop");
    };
    assert!(after[0].sold && !after[1].sold);
}

#[test]
fn a_shop_sells_a_land_and_a_relic_and_removes_one_card() {
    let data = data::embedded().unwrap();
    let prices = &data.balance.run.shop.prices;
    let mut state = run_at("firebrand", NodeKind::Shop);
    state.gold = 1000;
    let Phase::Shop { items, .. } = state.phase.clone() else {
        panic!("expected a shop");
    };

    act(&mut state, RunAction::Buy { item: 3 });
    assert_eq!(deck_count(&state, "rakdos_guildgate"), 1);
    assert_eq!(state.gold, 1000 - prices.land);
    // The Gate takes the place of a basic land.
    let view = run_view(&state).unwrap();
    let basics: u32 = view.basic_lands.iter().map(|entry| entry.count).sum();
    assert_eq!(basics, 9);

    let Ware::Relic { relic } = items[4].ware.clone() else {
        panic!("expected a relic");
    };
    let events = act(&mut state, RunAction::Buy { item: 4 });
    assert_eq!(state.relics, std::slice::from_ref(&relic));
    assert!(events.contains(&RunEvent::RelicGained { relic }));

    let gold = state.gold;
    let removed = state.deck[0].card_id.clone();
    let events = act(&mut state, RunAction::RemoveCard { card: 0 });
    assert_eq!(state.gold, gold - prices.removal);
    assert!(events.contains(&RunEvent::CardRemoved { card: removed }));
    // One removal per shop.
    assert!(
        !run_legal_actions(&state)
            .iter()
            .any(|action| matches!(action, RunAction::RemoveCard { .. }))
    );
    assert!(matches!(state.phase, Phase::Shop { .. }));
    act(&mut state, RunAction::Leave);
    assert_eq!(state.phase, Phase::Map);
}

// ---- Drafts ----

#[test]
fn a_draft_opens_a_pack_in_the_players_colours_and_two_cards_are_taken() {
    let data = data::embedded().unwrap();
    let draft = &data.balance.run.draft;
    let mut state = run_at("warden", NodeKind::Draft);
    let Phase::Draft {
        choices,
        picks_left,
    } = state.phase.clone()
    else {
        panic!("expected a draft, got {:?}", state.phase);
    };
    assert_eq!((choices.len(), picks_left), (5, 2));
    assert_eq!(
        (choices.len(), picks_left),
        (usize::from(draft.cards), draft.picks)
    );
    for (index, id) in choices.iter().enumerate() {
        let card = data.cards.get(id).unwrap();
        assert!(!card.is_land() && !card.is_legendary() && !card.token);
        assert!(!card.colors.is_empty());
        assert!(
            card.colors
                .iter()
                .all(|color| [crate::cards::Color::G, crate::cards::Color::W].contains(color)),
            "{id}"
        );
        assert!(!choices[..index].contains(id), "{id} is in the pack twice");
    }
    let mut expected: Vec<RunAction> = (0..5)
        .map(|choice| RunAction::TakeDraft { choice })
        .collect();
    expected.push(RunAction::Leave);
    assert_eq!(run_legal_actions(&state), expected);

    let cards = state.deck.len();
    let events = act(&mut state, RunAction::TakeDraft { choice: 3 });
    assert_eq!(
        events,
        [RunEvent::CardAdded {
            card: choices[3].clone()
        }]
    );
    // The card taken has left the pack, and one pick is left.
    let mut rest = choices.clone();
    rest.remove(3);
    assert_eq!(
        state.phase,
        Phase::Draft {
            choices: rest.clone(),
            picks_left: 1
        }
    );
    let gone = RunAction::TakeDraft { choice: 4 };
    assert_eq!(
        apply_run_action(&mut state, gone.clone()),
        Err(RunError::IllegalAction(gone))
    );

    act(&mut state, RunAction::TakeDraft { choice: 0 });
    assert_eq!(state.phase, Phase::Map);
    assert_eq!(state.deck.len(), cards + 2);
    assert_eq!(state.deck[cards].card_id, choices[3]);
    assert_eq!(state.deck[cards + 1].card_id, rest[0]);
}

#[test]
fn a_draft_can_be_left_with_one_card_or_none() {
    let arrived = run_at("firebrand", NodeKind::Draft);
    let cards = arrived.deck.len();

    let mut none = arrived.clone();
    assert_eq!(act(&mut none, RunAction::Leave), []);
    assert_eq!(none.phase, Phase::Map);
    assert_eq!(none.deck.len(), cards);

    let mut one = arrived.clone();
    act(&mut one, RunAction::TakeDraft { choice: 0 });
    act(&mut one, RunAction::Leave);
    assert_eq!(one.phase, Phase::Map);
    assert_eq!(one.deck.len(), cards + 1);
}

#[test]
fn every_path_passes_a_draft() {
    for seed in 0..50 {
        let state = run("warden", seed);
        let row: Vec<NodeKind> = state
            .map
            .nodes
            .iter()
            .filter(|node| node.row == 2)
            .map(|node| node.kind)
            .collect();
        assert!(!row.is_empty());
        assert!(row.iter().all(|kind| *kind == NodeKind::Draft), "{row:?}");
    }
}

// ---- Treasure ----

fn relic_id(id: &str) -> ModifierId {
    ModifierId(id.to_string())
}

fn every_relic() -> Vec<ModifierId> {
    let relics = &data::embedded().unwrap().run.relics;
    relics.iter().map(|relic| relic.id.clone()).collect()
}

#[test]
fn a_treasure_offers_two_relics_or_gold_and_one_is_taken() {
    let treasure = &data::embedded().unwrap().balance.run.treasure;
    let (mut arrived, node) = run_before("warden", NodeKind::Treasure);
    arrived.relics = vec![relic_id("war_banner")];
    let events = act(&mut arrived, RunAction::Travel { node });
    assert_eq!(
        events,
        [RunEvent::Travelled {
            node,
            kind: NodeKind::Treasure
        }]
    );
    let Phase::Treasure { relics, gold } = arrived.phase.clone() else {
        panic!("expected a treasure, got {:?}", arrived.phase);
    };
    assert_eq!((relics.len(), gold), (2, 40));
    assert_eq!(
        (relics.len(), gold),
        (usize::from(treasure.relics), treasure.gold)
    );
    // Two different relics, neither the one already owned.
    assert_ne!(relics[0], relics[1]);
    assert!(!relics.contains(&relic_id("war_banner")));
    assert!(relics.iter().all(|relic| every_relic().contains(relic)));
    assert_eq!(
        run_legal_actions(&arrived),
        [
            RunAction::TakeTreasure { choice: 0 },
            RunAction::TakeTreasure { choice: 1 },
            RunAction::TakeGold,
        ]
    );
    // It can't be walked away from with nothing, or emptied.
    for refused in [RunAction::Leave, RunAction::TakeTreasure { choice: 2 }] {
        assert_eq!(
            apply_run_action(&mut arrived, refused.clone()),
            Err(RunError::IllegalAction(refused))
        );
    }
    let before = arrived.gold;

    let mut took_relic = arrived.clone();
    let events = act(&mut took_relic, RunAction::TakeTreasure { choice: 1 });
    assert_eq!(
        events,
        [RunEvent::RelicGained {
            relic: relics[1].clone()
        }]
    );
    assert_eq!(
        took_relic.relics,
        [relic_id("war_banner"), relics[1].clone()]
    );
    assert_eq!(took_relic.gold, before);
    assert_eq!(took_relic.phase, Phase::Map);
    assert_eq!(travels(&took_relic), arrived.map.node(node).unwrap().next);

    let mut took_gold = arrived.clone();
    let events = act(&mut took_gold, RunAction::TakeGold);
    assert_eq!(
        events,
        [RunEvent::GoldChanged {
            change: 40,
            gold: before + 40,
            reason: GoldReason::Treasure
        }]
    );
    assert_eq!(took_gold.relics, [relic_id("war_banner")]);
    assert_eq!(took_gold.phase, Phase::Map);
}

#[test]
fn a_treasure_offers_the_relics_that_are_left_and_always_the_gold() {
    let all = every_relic();
    let (before, node) = run_before("firebrand", NodeKind::Treasure);

    // One relic left: that one, or the gold.
    let mut one_left = before.clone();
    one_left.relics = all[1..].to_vec();
    act(&mut one_left, RunAction::Travel { node });
    assert_eq!(
        one_left.phase,
        Phase::Treasure {
            relics: vec![all[0].clone()],
            gold: 40
        }
    );
    assert_eq!(
        run_legal_actions(&one_left),
        [RunAction::TakeTreasure { choice: 0 }, RunAction::TakeGold]
    );
    act(&mut one_left, RunAction::TakeTreasure { choice: 0 });
    assert_eq!(one_left.relics.len(), all.len());

    // None left: the gold is the way on.
    let mut none_left = before.clone();
    none_left.relics = all.clone();
    act(&mut none_left, RunAction::Travel { node });
    assert_eq!(
        none_left.phase,
        Phase::Treasure {
            relics: Vec::new(),
            gold: 40
        }
    );
    assert_eq!(run_legal_actions(&none_left), [RunAction::TakeGold]);
    let gold = none_left.gold;
    act(&mut none_left, RunAction::TakeGold);
    assert_eq!(none_left.gold, gold + 40);
    assert_eq!(none_left.phase, Phase::Map);
    assert_eq!(none_left.relics, all);
}

/// Every way through the map from `node` to the boss, as the kinds of the
/// nodes passed, `node` included.
fn paths_from(state: &RunState, node: NodeId) -> Vec<Vec<NodeKind>> {
    let here = state.map.node(node).unwrap();
    if here.next.is_empty() {
        return vec![vec![here.kind]];
    }
    let mut paths = Vec::new();
    for &next in &here.next {
        for mut path in paths_from(state, next) {
            path.insert(0, here.kind);
            paths.push(path);
        }
    }
    paths
}

#[test]
fn the_act_is_eleven_rows_with_as_many_duels_as_before_and_more_between_them() {
    use NodeKind::{Boss, Draft, Duel, Elite, Event, Rest, Shop, Treasure};
    // What each row may hold.
    let rows: [&[NodeKind]; 12] = [
        &[Duel],
        &[Duel],
        &[Draft],
        &[Treasure, Event],
        &[Duel, Elite],
        &[Shop, Rest, Event],
        &[Duel, Shop, Rest],
        &[Treasure, Event],
        &[Duel, Elite],
        &[Duel, Shop, Draft],
        &[Rest],
        &[Boss],
    ];
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..80 {
        let state = run("warden", seed);
        assert_eq!(state.map.rows, 12, "seed {seed}");
        for node in &state.map.nodes {
            let allowed = rows[usize::from(node.row)];
            assert!(allowed.contains(&node.kind), "seed {seed}: {node:?}");
            seen.insert((node.row, node.kind));
        }
        for start in travels(&state) {
            for path in paths_from(&state, start) {
                assert_eq!(path.len(), 12, "seed {seed}");
                let count =
                    |kinds: &[NodeKind]| path.iter().filter(|kind| kinds.contains(kind)).count();
                // Four duels are certain and two more can be chosen, as
                // on the shorter map; then the boss.
                let duels = count(&[Duel, Elite]);
                assert!((4..=6).contains(&duels), "seed {seed}: {path:?}");
                // Everything else: at least five stops, where there were
                // two to four.
                let others = count(&[Draft, Treasure, Event, Shop, Rest]);
                assert_eq!(duels + others, 11, "seed {seed}");
                // Two rows of treasures and events on every path.
                assert!(count(&[Treasure, Event]) >= 2, "seed {seed}: {path:?}");
                assert!(count(&[Event]) <= 3, "seed {seed}: {path:?}");
            }
        }
    }
    // Over many maps every row shows everything it may.
    let possible: usize = rows.iter().map(|row| row.len()).sum();
    assert_eq!(seen.len(), possible, "{seen:?}");
}

// ---- Events ----

fn event_id(id: &str) -> EventId {
    EventId(id.to_string())
}

/// A run that has just met the event `id` at an event node.
fn at_event(id: &str) -> RunState {
    let (mut state, node) = run_before("warden", NodeKind::Event);
    // Every other event has been met, so this is the one that is new.
    state.events_met = data::embedded()
        .unwrap()
        .run
        .events
        .iter()
        .map(|event| event.id.clone())
        .filter(|event| event.as_str() != id)
        .collect();
    act(&mut state, RunAction::Travel { node });
    assert!(
        matches!(&state.phase, Phase::Event { event, .. } if event.as_str() == id),
        "expected {id}, got {:?}",
        state.phase
    );
    state
}

fn take(option: usize, pick: Option<usize>) -> RunAction {
    RunAction::TakeEvent { option, pick }
}

#[test]
fn an_event_node_meets_an_event_that_can_be_walked_away_from() {
    let data = data::embedded().unwrap();
    let (before, node) = run_before("warden", NodeKind::Event);
    let mut state = before.clone();
    let events = act(&mut state, RunAction::Travel { node });
    assert_eq!(
        events,
        [RunEvent::Travelled {
            node,
            kind: NodeKind::Event
        }]
    );
    let Phase::Event { event, .. } = state.phase.clone() else {
        panic!("expected an event, got {:?}", state.phase);
    };
    assert!(data.run.event(&event).is_some());
    assert_eq!(state.events_met, [event]);
    assert_eq!(run_legal_actions(&state).last(), Some(&RunAction::Leave));

    // Walking away changes nothing but where the player is.
    let arrived = state.clone();
    assert_eq!(act(&mut state, RunAction::Leave), []);
    assert_eq!(state.phase, Phase::Map);
    assert_eq!(
        (&state.wounds, state.gold, &state.deck, &state.relics),
        (
            &arrived.wounds,
            arrived.gold,
            &arrived.deck,
            &arrived.relics
        )
    );
    assert_eq!(travels(&state), state.map.node(node).unwrap().next);

    // Once every event has been met, one comes round again.
    let mut again = before.clone();
    again.events_met = data.run.events.iter().map(|e| e.id.clone()).collect();
    act(&mut again, RunAction::Travel { node });
    assert!(matches!(again.phase, Phase::Event { .. }));
    assert_eq!(again.events_met.len(), data.run.events.len());

    // Every event in the data can be met, and each can be left.
    for def in &data.run.events {
        let mut state = at_event(def.id.as_str());
        act(&mut state, RunAction::Leave);
        assert_eq!(state.phase, Phase::Map);
    }
}

#[test]
fn the_devils_bargain_gives_a_relic_of_two_for_a_wound() {
    let mut state = at_event("devils_bargain");
    let Phase::Event { relics, cards, .. } = state.phase.clone() else {
        panic!("expected an event");
    };
    assert_eq!(relics.len(), 2);
    assert_ne!(relics[0], relics[1]);
    assert!(cards.is_empty());
    assert_eq!(
        run_legal_actions(&state),
        [take(0, Some(0)), take(0, Some(1)), RunAction::Leave]
    );
    // The relic has to be named.
    for refused in [take(0, None), take(0, Some(2)), take(1, Some(0))] {
        assert_eq!(
            apply_run_action(&mut state, refused.clone()),
            Err(RunError::IllegalAction(refused))
        );
    }

    // Not when the wound would end the run, and not with every relic owned.
    let mut last_legs = state.clone();
    last_legs.wounds = vec![wound(); max_wounds() - 1];
    assert_eq!(run_legal_actions(&last_legs), [RunAction::Leave]);
    let mut collector = at_event("devils_bargain");
    collector.relics = every_relic();
    collector.phase = Phase::Event {
        event: event_id("devils_bargain"),
        relics: Vec::new(),
        cards: Vec::new(),
    };
    assert_eq!(run_legal_actions(&collector), [RunAction::Leave]);

    let gold = state.gold;
    let events = act(&mut state, take(0, Some(1)));
    assert_eq!(
        events,
        [
            RunEvent::WoundTaken { wound: wound() },
            RunEvent::RelicGained {
                relic: relics[1].clone()
            }
        ]
    );
    assert_eq!(state.wounds, [wound()]);
    assert_eq!(state.relics, std::slice::from_ref(&relics[1]));
    assert_eq!(state.gold, gold);
    assert_eq!(state.phase, Phase::Map);
}

#[test]
fn the_moneylender_pays_for_a_wound_or_heals_one_for_gold() {
    let arrived = at_event("moneylender");

    // Unhurt, there is no wound to have healed.
    let mut healthy = arrived.clone();
    healthy.gold = 500;
    assert_eq!(
        run_legal_actions(&healthy),
        [take(0, None), RunAction::Leave]
    );
    let events = act(&mut healthy, take(0, None));
    assert_eq!(
        events,
        [
            RunEvent::WoundTaken { wound: wound() },
            RunEvent::GoldChanged {
                change: 100,
                gold: 600,
                reason: GoldReason::Event
            }
        ]
    );
    assert_eq!(healthy.phase, Phase::Map);

    // Hurt and with exactly the price: either way.
    let mut hurt = arrived.clone();
    (hurt.wounds, hurt.gold) = (vec![wound()], 70);
    assert_eq!(
        run_legal_actions(&hurt),
        [take(0, None), take(1, None), RunAction::Leave]
    );
    let events = act(&mut hurt, take(1, None));
    assert_eq!(
        events,
        [
            RunEvent::GoldChanged {
                change: -70,
                gold: 0,
                reason: GoldReason::Event
            },
            RunEvent::WoundHealed { wound: wound() }
        ]
    );
    assert!(hurt.wounds.is_empty());

    // One wound from the end and short of gold, she has nothing to offer.
    let mut broke = arrived.clone();
    (broke.wounds, broke.gold) = (vec![wound(); max_wounds() - 1], 69);
    assert_eq!(run_legal_actions(&broke), [RunAction::Leave]);
    assert_eq!(
        apply_run_action(&mut broke, take(0, None)),
        Err(RunError::IllegalAction(take(0, None)))
    );
}

#[test]
fn the_wandering_smith_swaps_a_card_for_one_a_rarity_higher() {
    use crate::cards::{Color, Rarity};
    let data = data::embedded().unwrap();
    let mut state = at_event("wandering_smith");
    // A land can't be reforged, and nor can a card with nothing above it.
    state.deck.push(crate::duel::CardInstance::new(CardDefId(
        "selesnya_guildgate".to_string(),
    )));
    let rarity = |card: &CardDefId| data.cards.get(card).unwrap().rarity;
    let picks: Vec<usize> = run_legal_actions(&state)
        .into_iter()
        .filter_map(|action| match action {
            RunAction::TakeEvent {
                option: 0,
                pick: Some(pick),
            } => Some(pick),
            RunAction::Leave => None,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    let gate = state.deck.len() - 1;
    assert!(!picks.contains(&gate));
    assert!(!picks.is_empty());
    for (index, card) in state.deck.iter().enumerate() {
        let land = data.cards.get(&card.card_id).unwrap().is_land();
        // No card of the pool is mythic, so a rare has nothing above it.
        let expected = !land && rarity(&card.card_id) != Rarity::Rare;
        assert_eq!(picks.contains(&index), expected, "{}", card.card_id);
    }
    assert_eq!(
        apply_run_action(&mut state, take(0, None)),
        Err(RunError::IllegalAction(take(0, None)))
    );

    let pick = *picks
        .iter()
        .find(|pick| rarity(&state.deck[**pick].card_id) == Rarity::Common)
        .unwrap();
    let old = state.deck[pick].card_id.clone();
    let copies = deck_count(&state, old.as_str());
    let size = state.deck.len();
    let twin = state.clone();
    let events = act(&mut state, take(0, Some(pick)));
    let [
        RunEvent::CardRemoved { card: removed },
        RunEvent::CardAdded { card: new },
    ] = events.as_slice()
    else {
        panic!("unexpected {events:?}");
    };
    assert_eq!(*removed, old);
    assert_eq!(rarity(new), Rarity::Uncommon);
    let def = data.cards.get(new).unwrap();
    assert!(!def.is_land() && !def.is_legendary() && !def.token);
    assert!(!def.colors.is_empty());
    assert!(def.colors.iter().all(|c| [Color::G, Color::W].contains(c)));
    assert_eq!(state.deck.len(), size);
    assert_eq!(deck_count(&state, old.as_str()), copies - 1);
    assert_eq!(state.deck.last().unwrap().card_id, *new);
    assert_eq!(state.phase, Phase::Map);

    // The same run gets the same card.
    let mut again = twin;
    act(&mut again, take(0, Some(pick)));
    assert_eq!(again, state);
}

#[test]
fn all_or_nothing_adds_all_three_cards_or_none() {
    let data = data::embedded().unwrap();
    let mut state = at_event("all_or_nothing");
    let Phase::Event { relics, cards, .. } = state.phase.clone() else {
        panic!("expected an event");
    };
    assert!(relics.is_empty());
    assert_eq!(cards.len(), 3);
    for (index, id) in cards.iter().enumerate() {
        let card = data.cards.get(id).unwrap();
        assert!(!card.is_land() && !card.is_legendary() && !card.token);
        assert!(!cards[..index].contains(id), "{id} is shown twice");
    }
    assert_eq!(run_legal_actions(&state), [take(0, None), RunAction::Leave]);
    let size = state.deck.len();
    let events = act(&mut state, take(0, None));
    let added: Vec<RunEvent> = cards
        .iter()
        .map(|card| RunEvent::CardAdded { card: card.clone() })
        .collect();
    assert_eq!(events, added);
    assert_eq!(state.deck.len(), size + 3);
    let tail: Vec<&CardDefId> = state.deck[size..].iter().map(|c| &c.card_id).collect();
    assert_eq!(tail, cards.iter().collect::<Vec<_>>());
    assert_eq!(state.phase, Phase::Map);
}

#[test]
fn the_rivals_wager_pays_now_and_costs_life_in_the_next_duel_only() {
    let wound = ModifierId("rivals_wager_wound".to_string());
    let mut state = at_event("rivals_wager");
    assert_eq!(run_legal_actions(&state), [take(0, None), RunAction::Leave]);
    let gold = state.gold;
    let events = act(&mut state, take(0, None));
    assert_eq!(
        events,
        [
            RunEvent::GoldChanged {
                change: 50,
                gold: gold + 50,
                reason: GoldReason::Event
            },
            RunEvent::NextDuelModified {
                modifier: wound.clone()
            }
        ]
    );
    assert_eq!(state.next_duel, std::slice::from_ref(&wound));
    assert_eq!(run_view(&state).unwrap().next_duel, [wound]);

    // It waits through nodes that aren't duels, and is spent on the first
    // duel there is.
    let mut duels = 0;
    for _ in 0..12 {
        match state.phase.clone() {
            Phase::Map => {
                let node = travels(&state)[0];
                act(&mut state, RunAction::Travel { node });
            }
            Phase::Duel { .. } => {
                duels += 1;
                let duel = state.duel.as_ref().unwrap();
                let expected = if duels == 1 { 15 } else { 20 };
                assert_eq!(duel.player(PLAYER).life, expected);
                // The wager never touches the enemy, whose life is its own
                // (the boss has more).
                assert!(duel.player(ENEMY).life >= 20);
                assert!(state.next_duel.is_empty());
                if duels == 2 {
                    return;
                }
                finish(&mut state, win());
            }
            Phase::Reward { .. } => {
                act(&mut state, RunAction::SkipReward);
            }
            _ => {
                assert_eq!(state.next_duel.len(), usize::from(duels == 0));
                let leave = run_legal_actions(&state).pop().unwrap();
                act(&mut state, leave);
            }
        }
    }
    panic!("no duel followed the wager");
}

// ---- The relics, in a duel of the run's ----

/// The duel a run with these relics starts, taken out of the run: both
/// players have kept, it is the player's first main phase and both hands
/// are empty. There is no starting emblem.
fn duel_with_relics(relics: &[&str]) -> DuelState {
    let mut state = run("warden", 3);
    state.relics = relics.iter().map(|id| relic_id(id)).collect();
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let mut duel = state.duel.take().unwrap();
    let first = duel.starting_player;
    apply_action(&mut duel, first, Action::Keep).unwrap();
    apply_action(&mut duel, first.opponent(), Action::Keep).unwrap();
    let turn = if first == PLAYER { 1 } else { 2 };
    pass_until(&mut duel, turn, Step::PrecombatMain);
    assert_eq!(duel.active_player, PLAYER);
    empty_hand(&mut duel, PLAYER);
    empty_hand(&mut duel, ENEMY);
    duel
}

/// Casts a card from nowhere and passes until the stack is empty or the
/// game asks for something other than priority.
fn cast_and_settle(duel: &mut DuelState, card: &str) -> Vec<GameEvent> {
    let card = put_in_hand(duel, PLAYER, card);
    let mut events = apply_action(duel, PLAYER, cast(card)).unwrap();
    for _ in 0..20 {
        if duel.stack.is_empty() || !matches!(duel.decision, Some(Decision::Priority { .. })) {
            break;
        }
        events.extend(pass_both(duel));
    }
    events
}

fn count_of(duel: &DuelState, player: PlayerId, card: &str) -> usize {
    duel.permanents_of(player)
        .filter(|permanent| permanent.def.as_str() == card)
        .count()
}

fn newest_permanent(duel: &DuelState) -> crate::duel::ObjectId {
    *duel.battlefield.last().unwrap()
}

#[test]
fn pilgrims_flask_scries_the_first_time_life_is_gained_each_turn() {
    let mut duel = duel_with_relics(&["pilgrims_flask"]);
    put_lands(&mut duel, PLAYER, "plains", 4);
    let life = duel.player(PLAYER).life;

    // Revitalize: gain 3 life and draw a card. Then the Flask's scry.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.player(PLAYER).life, life + 3);
    assert_eq!(
        duel.decision,
        Some(Decision::Scry {
            player: PLAYER,
            kept: 0,
            remaining: 1,
            surveil: false
        })
    );
    let top = *duel.player(PLAYER).library.last().unwrap();
    apply_action(&mut duel, PLAYER, Action::ScryBottom).unwrap();
    assert_ne!(duel.player(PLAYER).library.last(), Some(&top));
    assert_eq!(duel.decision, Some(Decision::Priority { player: PLAYER }));
    assert!(duel.stack.is_empty());

    // A second gain in the same turn scries nothing.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.player(PLAYER).life, life + 6);
    assert_eq!(duel.decision, Some(Decision::Priority { player: PLAYER }));
    assert!(duel.stack.is_empty());

    // Without the relic, gaining life asks nothing.
    let mut duel = duel_with_relics(&[]);
    put_lands(&mut duel, PLAYER, "plains", 2);
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.decision, Some(Decision::Priority { player: PLAYER }));
}

#[test]
fn beast_tamers_whistle_gains_life_for_a_creature_of_power_four() {
    let mut duel = duel_with_relics(&["beast_tamers_whistle"]);
    put_lands(&mut duel, PLAYER, "forest", 6);
    let life = duel.player(PLAYER).life;

    // Rumbling Baloth is a 4/4.
    cast_and_settle(&mut duel, "rumbling_baloth");
    assert_eq!(duel.player(PLAYER).life, life + 1);
    // Grizzly Bears is not big enough.
    cast_and_settle(&mut duel, "grizzly_bears");
    assert_eq!(duel.player(PLAYER).life, life + 1);
    assert!(duel.stack.is_empty());
    assert_eq!(duel.player(ENEMY).life, 20);
}

#[test]
fn votive_candle_makes_a_soldier_for_the_first_enchantment_each_turn() {
    let mut duel = duel_with_relics(&["votive_candle"]);
    put_lands(&mut duel, PLAYER, "plains", 7);

    // A creature spell is not an enchantment.
    cast_and_settle(&mut duel, "glory_seeker");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 0);

    cast_and_settle(&mut duel, "glorious_anthem");
    assert_eq!(count_of(&duel, PLAYER, "glorious_anthem"), 1);
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 1);

    // The second enchantment of the turn makes none.
    cast_and_settle(&mut duel, "intangible_virtue");
    assert_eq!(count_of(&duel, PLAYER, "intangible_virtue"), 1);
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 1);
    assert_eq!(count_of(&duel, ENEMY, "soldier_token"), 0);
}

#[test]
fn surveyors_map_starts_each_duel_with_a_basic_land() {
    let duel = duel_with_relics(&["surveyors_map"]);
    let lands: Vec<_> = duel.permanents_of(PLAYER).collect();
    assert_eq!(lands.len(), 1);
    // The Warden's starting spells have more green symbols than white.
    assert_eq!(lands[0].def.as_str(), "forest");
    assert!(!lands[0].tapped);
    assert_eq!(duel.permanents_of(ENEMY).count(), 0);
    // It is one more land than the deck holds.
    let player = duel.player(PLAYER);
    assert_eq!(player.hand.len() + player.library.len(), 17);
}

#[test]
fn merchants_ledger_lowers_every_price_in_a_shop() {
    let data = data::embedded().unwrap();
    let prices = &data.balance.run.shop.prices;
    let (before, node) = run_before("warden", NodeKind::Shop);

    let mut plain = before.clone();
    act(&mut plain, RunAction::Travel { node });
    let Phase::Shop { items: listed, .. } = plain.phase.clone() else {
        panic!("expected a shop");
    };

    let mut state = before.clone();
    state.relics = vec![relic_id("merchants_ledger")];
    act(&mut state, RunAction::Travel { node });
    let Phase::Shop {
        items,
        removal_price,
        ..
    } = state.phase.clone()
    else {
        panic!("expected a shop");
    };
    assert_eq!(items.len(), 5);
    // The same cards and land, each a fifth cheaper.
    for (item, plain) in items.iter().zip(&listed).take(4) {
        assert_eq!(item.ware, plain.ware);
        assert_eq!(item.price, plain.price * 80 / 100);
        assert!(item.price < plain.price);
    }
    assert_eq!(items[3].price, 40);
    assert_eq!(items[4].price, prices.relic * 80 / 100);
    assert_eq!(removal_price, 48);

    // What is affordable follows the lower price, and that is what is paid.
    state.gold = items[0].price;
    assert!(run_legal_actions(&state).contains(&RunAction::Buy { item: 0 }));
    let events = act(&mut state, RunAction::Buy { item: 0 });
    assert_eq!(state.gold, 0);
    assert!(events.contains(&RunEvent::GoldChanged {
        change: -i64::from(items[0].price),
        gold: 0,
        reason: GoldReason::Purchase
    }));
    state.gold = 48;
    let events = act(&mut state, RunAction::RemoveCard { card: 0 });
    assert_eq!(state.gold, 0);
    assert!(events.contains(&RunEvent::GoldChanged {
        change: -48,
        gold: 0,
        reason: GoldReason::Removal
    }));
}

#[test]
fn a_merchants_ledger_bought_in_a_shop_lowers_that_shops_other_prices() {
    let data = data::embedded().unwrap();
    let prices = &data.balance.run.shop.prices;
    let mut state = run_at("warden", NodeKind::Shop);
    let Phase::Shop {
        mut items,
        removal_price,
        removals_left,
    } = state.phase.clone()
    else {
        panic!("expected a shop");
    };
    items[4].ware = Ware::Relic {
        relic: relic_id("merchants_ledger"),
    };
    state.phase = Phase::Shop {
        items: items.clone(),
        removal_price,
        removals_left,
    };
    state.gold = 1000;

    // A card bought before the Ledger costs the listed price.
    act(&mut state, RunAction::Buy { item: 1 });
    assert_eq!(state.gold, 1000 - items[1].price);
    let gold = state.gold;
    // So does the Ledger itself.
    act(&mut state, RunAction::Buy { item: 4 });
    assert_eq!(state.gold, gold - prices.relic);

    let Phase::Shop {
        items: after,
        removal_price,
        ..
    } = state.phase.clone()
    else {
        panic!("expected to still be in the shop");
    };
    for index in [0, 2, 3] {
        assert_eq!(after[index].price, items[index].price * 80 / 100);
    }
    // What was sold keeps the price paid.
    assert_eq!(after[1].price, items[1].price);
    assert_eq!(after[4].price, prices.relic);
    assert_eq!(removal_price, prices.removal * 80 / 100);
    let gold = state.gold;
    act(&mut state, RunAction::Buy { item: 0 });
    assert_eq!(state.gold, gold - after[0].price);
}

// ---- The boss ----

/// A run with the boss node next, and that node.
fn before_the_boss(planeswalker: &str) -> (RunState, NodeId) {
    let mut state = run(planeswalker, 3);
    let node = walk_to(&mut state, NodeKind::Boss).unwrap();
    (state, node)
}

/// A run that has just started the boss duel.
fn at_the_boss(planeswalker: &str) -> RunState {
    let (mut state, node) = before_the_boss(planeswalker);
    act(&mut state, RunAction::Travel { node });
    state
}

#[test]
fn the_boss_duel_starts_on_arriving_with_the_wounds_carried() {
    let data = data::embedded().unwrap();
    let (mut state, node) = before_the_boss("firebrand");
    state.wounds = vec![wound()];
    state.relics.clear();
    // No feats for the boss: it is only to be beaten.
    let events = apply_run_action(&mut state, RunAction::Travel { node }).unwrap();
    assert!(matches!(events[1], RunEvent::DuelStarted { .. }));
    assert!(state.feats.is_empty() && state.duel_stats.is_some());
    assert!(run_view(&state).unwrap().feats.is_empty());
    let Phase::Duel { enemy, kind } = state.phase.clone() else {
        panic!("expected the boss duel");
    };
    assert_eq!(kind, DuelKind::Boss);
    assert_eq!(data.run.enemy(&enemy).unwrap().tier, EnemyTier::Boss);
    let duel = state.duel.as_ref().unwrap();
    assert_eq!(duel.player(PLAYER).life, 20);
    // The boss's own emblem gives it more life than a player starts with.
    assert!(duel.player(ENEMY).life > 20);
    // Nothing but the wound.
    let permanents: Vec<&str> = duel
        .permanents_of(PLAYER)
        .map(|permanent| permanent.def.as_str())
        .collect();
    assert_eq!(permanents, ["festering_wound"]);
    assert!(
        duel.emblems
            .iter()
            .any(|emblem| emblem.controller == ENEMY && emblem.def.name == "The Pyre")
    );
}

#[test]
fn beating_the_boss_wins_the_run_and_losing_to_it_ends_the_run() {
    let mut won = at_the_boss("warden");
    let events = finish(&mut won, win());
    assert_eq!(won.phase, Phase::Over { won: true });
    assert_eq!(events.last(), Some(&RunEvent::RunEnded { won: true }));
    assert_eq!(run_legal_actions(&won), []);

    let mut lost = at_the_boss("warden");
    finish(&mut lost, loss());
    // However few wounds were carried.
    assert!(lost.wounds.is_empty());
    assert_eq!(lost.phase, Phase::Over { won: false });

    let mut conceded = at_the_boss("warden");
    act(&mut conceded, RunAction::Concede);
    assert_eq!(conceded.phase, Phase::Over { won: false });
}

#[test]
fn a_drawn_boss_duel_is_fought_again() {
    let mut state = at_the_boss("warden");
    let first = state.duel.clone();
    let events = finish(&mut state, GameResult::Draw);
    assert!(matches!(
        events.last(),
        Some(RunEvent::DuelStarted {
            kind: DuelKind::Boss,
            ..
        })
    ));
    assert!(matches!(
        state.phase,
        Phase::Duel {
            kind: DuelKind::Boss,
            ..
        }
    ));
    let rematch = state.duel.as_ref().unwrap();
    assert_eq!(rematch.player(PLAYER).life, 20);
    assert!(rematch.result.is_none());
    // A new duel from a new seed.
    assert_ne!(Some(rematch), first.as_ref());
}

// ---- Whole runs ----

/// Plays a whole run: the AI plays both sides of every duel, and
/// everything else is chosen at random with `choice_seed`. `check` sees the
/// state before every run action. Returns the final state and everything
/// that happened.
fn play_run(
    planeswalker: &str,
    seed: u64,
    choice_seed: u64,
    mut check: impl FnMut(&RunState),
) -> (RunState, Vec<RunEvent>, Vec<GameEvent>) {
    let mut state = run(planeswalker, seed);
    let mut choices = DuelRng::from_seed(choice_seed);
    let (mut run_log, mut duel_log) = (Vec::new(), Vec::new());
    for _ in 0..2_000 {
        check(&state);
        if matches!(state.phase, Phase::Over { .. }) {
            return (state, run_log, duel_log);
        }
        if state
            .duel
            .as_ref()
            .is_some_and(|duel| duel.result.is_none())
        {
            duel_log.extend(play_duel(&mut state));
            continue;
        }
        let actions = run_legal_actions(&state);
        assert!(!actions.is_empty(), "nothing to do in {:?}", state.phase);
        let pick = choices.below(u32::try_from(actions.len()).unwrap()) as usize;
        run_log.extend(act(&mut state, actions[pick].clone()));
    }
    panic!("the run did not end");
}

#[test]
fn the_same_seed_and_actions_give_the_same_run_duels_included() {
    let (state, run_log, duel_log) = play_run("warden", 11, 5, |_| {});
    let (again, run_again, duel_again) = play_run("warden", 11, 5, |_| {});
    assert_eq!(state, again);
    assert_eq!(run_log, run_again);
    assert_eq!(duel_log, duel_again);
    assert!(duel_log.len() > 100);

    let (_, other_run, other_duels) = play_run("warden", 12, 5, |_| {});
    assert_ne!(run_log, other_run);
    assert_ne!(duel_log, other_duels);
    let (_, _, other_planeswalker) = play_run("firebrand", 11, 5, |_| {});
    assert_ne!(duel_log, other_planeswalker);
}

#[test]
fn a_run_saved_mid_duel_and_restored_plays_on_identically() {
    let mut state = run("firebrand", 21);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    // Part of the way into the duel.
    for _ in 0..30 {
        let duel = state.duel.as_ref().unwrap();
        let Some(player) = duel.waiting_on() else {
            break;
        };
        let action = ai_choose_action(duel, player).unwrap();
        apply_duel_action(&mut state, player, action).unwrap();
    }
    let json = serde_json::to_string(&state).unwrap();
    let mut restored: RunState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, state);

    let finish_run = |state: &mut RunState| {
        let mut log = play_duel(state);
        let mut events = act(state, RunAction::FinishDuel);
        if matches!(state.phase, Phase::Reward { .. }) {
            events.extend(act(state, RunAction::TakeReward { choice: 0 }));
        }
        let node = travels(state)[0];
        events.extend(act(state, RunAction::Travel { node }));
        log.extend(play_duel(state));
        (log, events)
    };
    assert_eq!(finish_run(&mut state), finish_run(&mut restored));
    assert_eq!(restored, state);
}

#[test]
fn random_runs_always_have_something_to_do_and_end() {
    let data = data::embedded().unwrap();
    let (mut wins, mut losses) = (0, 0);
    for seed in 0..12 {
        let planeswalker = if seed % 2 == 0 { "warden" } else { "firebrand" };
        let (state, run_log, _) = play_run(planeswalker, seed, seed + 100, |state| {
            let view = run_view(state).unwrap();
            assert_eq!(view.actions, run_legal_actions(state));
            match &state.phase {
                Phase::Over { .. } => assert!(view.actions.is_empty()),
                Phase::Duel { .. } => {
                    // Either the duel waits on someone, or it can be left.
                    let duel = state.duel.as_ref().unwrap();
                    assert_eq!(
                        duel.waiting_on().is_some(),
                        view.actions == [RunAction::Concede]
                    );
                    assert_eq!(
                        duel.result.is_some(),
                        view.actions == [RunAction::FinishDuel]
                    );
                }
                _ => {
                    assert!(state.duel.is_none());
                    assert!(!view.actions.is_empty());
                }
            }
            assert!(state.wounds.len() <= max_wounds());
            for instance in &state.deck {
                assert!(data.cards.get(&instance.card_id).is_some());
            }
        });
        match state.phase {
            Phase::Over { won: true } => wins += 1,
            Phase::Over { won: false } => losses += 1,
            other => panic!("the run stopped in {other:?}"),
        }
        assert_eq!(
            run_log.last(),
            Some(&RunEvent::RunEnded {
                won: matches!(state.phase, Phase::Over { won: true })
            })
        );
        // The path goes one row at a time.
        for (row, node) in state.path.iter().enumerate() {
            assert_eq!(usize::from(state.map.node(*node).unwrap().row), row);
        }
    }
    assert_eq!(wins + losses, 12);
}

#[test]
fn the_view_and_the_content_serialise() {
    let mut state = run_at("warden", NodeKind::Shop);
    state.relics.push(ModifierId("war_banner".to_string()));
    let view = run_view(&state).unwrap();
    let json = serde_json::to_string(&view).unwrap();
    assert_eq!(serde_json::from_str::<RunView>(&json).unwrap(), view);
    assert!(json.contains(r#""phase":{"type":"shop""#));

    let treasure = run_view(&run_at("warden", NodeKind::Treasure)).unwrap();
    let json = serde_json::to_string(&treasure).unwrap();
    assert_eq!(serde_json::from_str::<RunView>(&json).unwrap(), treasure);
    assert!(json.contains(r#""phase":{"type":"treasure","relics":["#));
    assert!(json.contains(r#"{"type":"take_gold"}"#));

    let event = run_view(&at_event("devils_bargain")).unwrap();
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(serde_json::from_str::<RunView>(&json).unwrap(), event);
    assert!(json.contains(r#""phase":{"type":"event","event":"devils_bargain","relics":["#));
    assert!(json.contains(r#"{"type":"take_event","option":0,"pick":1}"#));
    assert_eq!(run_content().unwrap().events.len(), 5);

    let content = run_content().unwrap();
    assert_eq!(content.planeswalkers.len(), 2);
    assert_eq!(content.wounds.len(), 1);
    assert!(
        content
            .reward_cards
            .contains(&CardDefId("selesnya_guildgate".to_string()))
    );
    assert!(
        !content
            .reward_cards
            .contains(&CardDefId("forest".to_string()))
    );
    let json = serde_json::to_string(&content).unwrap();
    assert_eq!(serde_json::from_str::<RunContent>(&json).unwrap(), content);
}

#[test]
fn soothing_spring_adds_a_life_to_the_first_gain_of_each_turn() {
    let mut duel = duel_with_relics(&["soothing_spring"]);
    put_lands(&mut duel, PLAYER, "plains", 4);
    let life = duel.player(PLAYER).life;
    // Revitalize gains 3, and the Spring 1 more; its own life sets
    // nothing more off.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.player(PLAYER).life, life + 4);
    assert!(duel.stack.is_empty());
    // A second gain in the same turn is as printed.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.player(PLAYER).life, life + 7);
    // With no life gained it does nothing, turn after turn.
    let turn = duel.turn + 2;
    empty_hand(&mut duel, PLAYER);
    pass_until(&mut duel, turn, Step::PrecombatMain);
    assert_eq!(duel.player(PLAYER).life, life + 7);
}

#[test]
fn sentinels_horn_gives_vigilance_to_creatures_with_a_counter() {
    let mut duel = duel_with_relics(&["sentinels_horn"]);
    put_lands(&mut duel, PLAYER, "forest", 3);
    cast_and_settle(&mut duel, "grizzly_bears");
    let bears = newest_permanent(&duel);
    assert!(
        !duel
            .has_keyword(bears, crate::cards::Keyword::Vigilance)
            .unwrap()
    );
    let veil = put_in_hand(&mut duel, PLAYER, "snakeskin_veil");
    let target = crate::duel::Target::Object { object: bears };
    apply_action(
        &mut duel,
        PLAYER,
        crate::duel::test_support::cast_at(veil, target),
    )
    .unwrap();
    pass_both(&mut duel);
    assert!(
        duel.has_keyword(bears, crate::cards::Keyword::Vigilance)
            .unwrap()
    );
}

#[test]
fn a_relic_held_back_is_never_offered() {
    let data = data::embedded().unwrap();
    let held: Vec<&ModifierId> = data
        .run
        .relics
        .iter()
        .filter(|relic| relic.reserved)
        .map(|relic| &relic.id)
        .collect();
    assert_eq!(held.len(), 2);
    // However many are asked for, and whatever the dice: only the others.
    for seed in 0..20 {
        let mut rng = DuelRng::from_seed(seed);
        let offered = super::rewards::relic_offer(&mut rng, data, &[], 20);
        assert_eq!(offered.len(), data.run.relics.len() - held.len());
        assert!(offered.iter().all(|relic| !held.contains(&relic)));
    }
}

#[test]
fn anointing_oil_grows_the_creatures_that_have_a_counter_when_life_is_first_gained() {
    let mut duel = duel_with_relics(&["anointing_oil"]);
    put_lands(&mut duel, PLAYER, "plains", 6);
    put_lands(&mut duel, PLAYER, "forest", 5);
    cast_and_settle(&mut duel, "grizzly_bears");
    let bears = newest_permanent(&duel);
    cast_and_settle(&mut duel, "glory_seeker");
    let seeker = newest_permanent(&duel);
    let veil = put_in_hand(&mut duel, PLAYER, "snakeskin_veil");
    let target = crate::duel::Target::Object { object: bears };
    apply_action(
        &mut duel,
        PLAYER,
        crate::duel::test_support::cast_at(veil, target),
    )
    .unwrap();
    pass_both(&mut duel);
    assert_eq!(duel.power(bears).unwrap(), 3);

    // The Bears have a counter and get another; the Seeker has none.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.power(bears).unwrap(), 4);
    assert_eq!(duel.power(seeker).unwrap(), 2);
    // Once a turn.
    cast_and_settle(&mut duel, "revitalize");
    assert_eq!(duel.power(bears).unwrap(), 4);
}

#[test]
fn drovers_bell_makes_a_soldier_for_the_first_creature_of_power_five_each_turn() {
    let mut duel = duel_with_relics(&["drovers_bell"]);
    put_lands(&mut duel, PLAYER, "forest", 16);
    // A 4/4 is not big enough (nor is the commander, a 4/5).
    cast_and_settle(&mut duel, "rumbling_baloth");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 0);
    cast_and_settle(&mut duel, "colossal_dreadmaw");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 1);
    assert!(duel.stack.is_empty());
    assert_eq!(count_of(&duel, ENEMY, "soldier_token"), 0);
    // Once a turn: a second big creature makes none.
    cast_and_settle(&mut duel, "colossal_dreadmaw");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 1);
}

/// The Bell's Soldier can itself be a creature of power 5 (under two
/// Collective Blessings it enters as a 7/7). Were the Bell not once a turn,
/// each Soldier would ring it again without end.
#[test]
fn drovers_bell_does_not_ring_itself_for_ever() {
    let mut duel = duel_with_relics(&["drovers_bell"]);
    put_lands(&mut duel, PLAYER, "forest", 6);
    for _ in 0..2 {
        crate::duel::test_support::put_on_battlefield(&mut duel, PLAYER, "collective_blessing");
    }
    cast_and_settle(&mut duel, "grizzly_bears");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 1);
    assert!(duel.stack.is_empty());
}

#[test]
fn weavers_thread_gains_a_life_for_the_first_aura_spell_each_turn() {
    let mut duel = duel_with_relics(&["weavers_thread"]);
    put_lands(&mut duel, PLAYER, "plains", 8);
    let life = duel.player(PLAYER).life;
    cast_and_settle(&mut duel, "glory_seeker");
    let seeker = crate::duel::Target::Object {
        object: newest_permanent(&duel),
    };
    // An enchantment that is no Aura gains nothing.
    cast_and_settle(&mut duel, "intangible_virtue");
    assert_eq!(duel.player(PLAYER).life, life);
    // Pacifism is an Aura: 1 life as it is cast. A second Aura in the same
    // turn gains none.
    for expected in [life + 1, life + 1] {
        let aura = put_in_hand(&mut duel, PLAYER, "pacifism");
        apply_action(
            &mut duel,
            PLAYER,
            crate::duel::test_support::cast_at(aura, seeker),
        )
        .unwrap();
        pass_both(&mut duel);
        pass_both(&mut duel);
        assert_eq!(duel.player(PLAYER).life, expected);
    }
}

#[test]
fn rally_horn_adds_a_soldier_to_the_first_token_of_each_turn() {
    let mut duel = duel_with_relics(&["rally_horn"]);
    // Nothing is there at the start, and a creature that is no token
    // brings nothing.
    assert_eq!(duel.permanents_of(PLAYER).count(), 0);
    put_lands(&mut duel, PLAYER, "plains", 6);
    cast_and_settle(&mut duel, "glory_seeker");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 0);
    // Raise the Alarm's two Soldiers, and one more for the first of them.
    // That third Soldier is a token too, but the Horn sounds once a turn.
    cast_and_settle(&mut duel, "raise_the_alarm");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 3);
    cast_and_settle(&mut duel, "raise_the_alarm");
    assert_eq!(count_of(&duel, PLAYER, "soldier_token"), 5);
    assert!(duel.stack.is_empty());
    assert_eq!(count_of(&duel, ENEMY, "soldier_token"), 0);
}

#[test]
fn whetstone_puts_a_counter_on_the_first_creature_token_each_turn() {
    let mut duel = duel_with_relics(&["whetstone"]);
    put_lands(&mut duel, PLAYER, "plains", 6);
    // A creature that is no token gets nothing.
    cast_and_settle(&mut duel, "glory_seeker");
    assert_eq!(duel.power(newest_permanent(&duel)).unwrap(), 2);

    // Raise the Alarm makes two Soldiers: the first gets the counter.
    cast_and_settle(&mut duel, "raise_the_alarm");
    let powers = |duel: &DuelState| -> Vec<i32> {
        let mut powers: Vec<i32> = duel
            .permanents_of(PLAYER)
            .filter(|permanent| permanent.def.as_str() == "soldier_token")
            .map(|permanent| duel.power(permanent.id).unwrap())
            .collect();
        powers.sort_unstable();
        powers
    };
    assert_eq!(powers(&duel), [1, 2]);
    // More tokens in the same turn get none.
    cast_and_settle(&mut duel, "raise_the_alarm");
    assert_eq!(powers(&duel), [1, 1, 1, 2]);

    // The counter stays, and the next turn's first token gets one.
    let turn = duel.turn + 2;
    pass_until(&mut duel, turn, Step::PrecombatMain);
    empty_hand(&mut duel, PLAYER);
    cast_and_settle(&mut duel, "raise_the_alarm");
    assert_eq!(powers(&duel), [1, 1, 1, 1, 2, 2]);
}

// ---- The balance pass: hard enemies late in the act, and cards held back ----

#[test]
fn the_acts_duels_get_harder_easy_then_normal_then_hard() {
    let data = data::embedded().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..12 {
        let mut state = run("warden", seed);
        for _ in 0..200 {
            match state.phase.clone() {
                Phase::Over { .. } => break,
                Phase::Map => {
                    let node = travels(&state)[0];
                    act(&mut state, RunAction::Travel { node });
                }
                Phase::Duel { enemy, kind } => {
                    let row = state.map.node(state.position().unwrap()).unwrap().row;
                    let tier = data.run.enemy(&enemy).unwrap().tier;
                    let expected = match (kind, row) {
                        (DuelKind::Boss, _) => EnemyTier::Boss,
                        (DuelKind::Elite, _) => EnemyTier::Elite,
                        (DuelKind::Normal, 0..=1) => EnemyTier::Easy,
                        (DuelKind::Normal, 2..=7) => EnemyTier::Normal,
                        (DuelKind::Normal, _) => EnemyTier::Hard,
                    };
                    assert_eq!(tier, expected, "seed {seed}, row {row}: {enemy}");
                    seen.insert(tier);
                    finish(&mut state, win());
                }
                // Anything else is left by its last action: skip, leave,
                // or a treasure's gold.
                _ => {
                    let leave = run_legal_actions(&state).pop().unwrap();
                    act(&mut state, leave);
                }
            }
        }
        assert_eq!(state.phase, Phase::Over { won: true }, "seed {seed}");
    }
    // A dozen acts between them meet every tier.
    assert_eq!(seen.len(), 5, "{seen:?}");
}

#[test]
fn a_card_held_back_is_never_offered() {
    let data = data::embedded().unwrap();
    let warden = data
        .run
        .planeswalker(&PlaneswalkerId("warden".to_string()))
        .unwrap();
    let held: Vec<&CardDefId> = data
        .cards
        .iter()
        .filter(|card| card.reserved)
        .map(|card| &card.id)
        .collect();
    // The starting deck's own cards are among them: a starter card is
    // never a reward.
    assert!(held.len() >= 20);
    for entry in &warden.spells {
        assert!(held.contains(&&entry.card), "{}", entry.card);
    }
    let pool = super::rewards::offered_spells(&data.cards, &warden.colors);
    assert!(pool.iter().all(|card| !card.reserved));
    for seed in 0..40 {
        let mut rng = DuelRng::from_seed(seed);
        let weights = &data.balance.run.rewards.top;
        let mut offered = super::rewards::card_reward(&mut rng, data, &warden.colors, weights, 4);
        offered.extend(super::rewards::draft_pack(&mut rng, data, &warden.colors));
        offered.extend(super::rewards::event_cards(
            &mut rng,
            data,
            &warden.colors,
            3,
        ));
        for item in super::rewards::shop_stock(&mut rng, data, &warden.colors, &[]) {
            if let Ware::Card { card } = item.ware {
                offered.push(card);
            }
        }
        for card in &offered {
            assert!(!held.contains(&card), "seed {seed} offers {card}");
        }
    }
}
