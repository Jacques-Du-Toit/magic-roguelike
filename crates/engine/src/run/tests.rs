//! Tests for the run's rules.

use super::*;
use crate::ai_choose_action;
use crate::cards::CardDefId;
use crate::data;
use crate::duel::{DuelRng, GameEvent, GameResult, WinReason};

fn run(planeswalker: &str, seed: u64) -> RunState {
    new_run(
        RunConfig {
            planeswalker: PlaneswalkerId(planeswalker.to_string()),
        },
        seed,
    )
    .unwrap()
}

fn act(state: &mut RunState, action: RunAction) -> Vec<RunEvent> {
    apply_run_action(state, action).unwrap()
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

/// Finishes the duel in progress with the given result.
fn finish(state: &mut RunState, result: GameResult) -> Vec<RunEvent> {
    force_result(state, result);
    act(state, RunAction::FinishDuel)
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

/// Walks the map, taking a node of kind `stop_at` as soon as one is on
/// offer, and stops having travelled there. Duels on the way are won, rewards skipped, other nodes left.
fn travel_to(state: &mut RunState, stop_at: NodeKind) -> bool {
    for _ in 0..40 {
        match state.phase.clone() {
            Phase::Map => {
                let options = travels(state);
                let node = options
                    .iter()
                    .copied()
                    .find(|node| kind_of(state, *node) == stop_at)
                    .unwrap_or(options[0]);
                act(state, RunAction::Travel { node });
                if kind_of(state, node) == stop_at {
                    return true;
                }
            }
            Phase::Duel { .. } => {
                finish(state, win());
            }
            Phase::Reward { .. } => {
                act(state, RunAction::SkipReward);
            }
            Phase::Rest | Phase::Shop { .. } => {
                act(state, RunAction::Leave);
            }
            Phase::Boons { .. } | Phase::Over { .. } => return false,
        }
    }
    false
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

fn deck_count(state: &RunState, card: &str) -> usize {
    state
        .deck
        .iter()
        .filter(|instance| instance.card_id.as_str() == card)
        .count()
}

// ---- Starting a run ----

#[test]
fn a_new_run_has_the_starting_deck_hearts_and_gold() {
    let balance = &data::embedded().unwrap().balance.run;
    let state = run("warden", 1);
    assert_eq!(state.hearts, balance.hearts);
    assert_eq!(state.max_hearts, balance.hearts);
    assert_eq!(state.gold, balance.starting_gold);
    assert_eq!(state.deck.len(), 15);
    assert!(state.relics.is_empty() && state.boons.is_empty());
    assert_eq!(state.phase, Phase::Map);
    assert_eq!(state.position(), None);
    assert!(state.duel.is_none());

    let view = run_view(&state).unwrap();
    assert_eq!(view.seed, "1");
    let lands: u32 = view.basic_lands.iter().map(|entry| entry.count).sum();
    assert_eq!(lands, 10);
    assert_eq!(view.actions, run_legal_actions(&state));
    assert_eq!(view.actions.len(), 3);
}

#[test]
fn an_unknown_planeswalker_is_an_error() {
    let config = RunConfig {
        planeswalker: PlaneswalkerId("nobody".to_string()),
    };
    assert_eq!(
        new_run(config, 1),
        Err(RunError::UnknownPlaneswalker(PlaneswalkerId(
            "nobody".to_string()
        )))
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
    assert_eq!(
        events[1],
        RunEvent::DuelStarted {
            enemy: enemy.clone(),
            kind
        }
    );
    let enemy = data::embedded().unwrap().run.enemy(&enemy).unwrap();
    assert_eq!(enemy.tier, EnemyTier::Easy);
    assert_eq!(current_enemy(&state), Some(&enemy.id));

    let duel = state.duel.as_ref().unwrap();
    // The player's 15 spells and 10 lands, and the passive as an emblem.
    let player = duel.player(PLAYER);
    assert_eq!(player.hand.len() + player.library.len(), 25);
    assert_eq!(duel.emblems.len(), 1);
    assert_eq!(duel.emblems[0].controller, PLAYER);
    assert_eq!(duel.emblems[0].def.name, "Warden's Welcome");

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

    play_duel(&mut state);
    assert!(state.duel.as_ref().unwrap().result.is_some());
    assert_eq!(run_legal_actions(&state), [RunAction::FinishDuel]);
}

#[test]
fn winning_a_duel_pays_gold_and_offers_three_cards() {
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
    assert_eq!(events.len(), 2);
    assert_eq!(state.hearts, state.max_hearts);
    assert!(state.duel.is_none());

    let Phase::Reward { choices } = state.phase.clone() else {
        panic!("expected a reward, got {:?}", state.phase);
    };
    assert_eq!(choices.len(), 3);
    assert_eq!(
        run_legal_actions(&state),
        [
            RunAction::TakeReward { choice: 0 },
            RunAction::TakeReward { choice: 1 },
            RunAction::TakeReward { choice: 2 },
            RunAction::SkipReward,
        ]
    );
}

#[test]
fn a_win_pays_the_same_however_it_was_won() {
    let gold = &data::embedded().unwrap().balance.run.gold;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    // A quick win at full life earns no more than any other.
    state.duel.as_mut().unwrap().turn = 3;
    let before = state.gold;
    finish(&mut state, win());
    assert_eq!(state.gold, before + gold.duel_win);
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
    assert_eq!(state.deck.len(), 16);
    assert_eq!(state.deck[15].card_id, choices[1]);
    assert_eq!(state.phase, Phase::Map);

    // 16 spells want 11 lands.
    let view = run_view(&state).unwrap();
    let lands: u32 = view.basic_lands.iter().map(|entry| entry.count).sum();
    assert_eq!(lands, 11);

    // Skipping leaves the deck alone.
    let mut other = run("warden", 3);
    let node = travels(&other)[0];
    act(&mut other, RunAction::Travel { node });
    finish(&mut other, win());
    assert_eq!(act(&mut other, RunAction::SkipReward), []);
    assert_eq!(other.deck.len(), 15);
}

#[test]
fn losing_a_duel_costs_a_heart_and_pays_a_little_gold() {
    let gold = &data::embedded().unwrap().balance.run.gold;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let before = state.gold;
    let events = finish(&mut state, loss());
    assert_eq!(state.hearts, state.max_hearts - 1);
    assert_eq!(state.gold, before + gold.duel_loss);
    assert!(events.contains(&RunEvent::HeartsChanged {
        change: -1,
        hearts: state.hearts
    }));
    // No card reward: straight back to the map, and the node is behind us.
    assert_eq!(state.phase, Phase::Map);
    assert_eq!(travels(&state), state.map.node(node).unwrap().next);
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
    assert_eq!(state.hearts, state.max_hearts - 1);
    assert!(state.duel.is_none());
    assert_eq!(state.phase, Phase::Map);
}

#[test]
fn a_drawn_duel_costs_nothing_and_pays_a_little_gold() {
    let gold = &data::embedded().unwrap().balance.run.gold;
    let mut state = run("warden", 3);
    let node = travels(&state)[0];
    act(&mut state, RunAction::Travel { node });
    let before = state.gold;
    let events = finish(&mut state, GameResult::Draw);
    assert_eq!(state.hearts, state.max_hearts);
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
fn losing_the_last_heart_ends_the_run() {
    let mut state = run("warden", 3);
    let hearts = state.hearts;
    let mut lost = 0;
    while lost < hearts {
        let node = travels(&state)[0];
        act(&mut state, RunAction::Travel { node });
        if state.duel.is_none() {
            act(&mut state, RunAction::Leave);
            continue;
        }
        let events = finish(&mut state, loss());
        lost += 1;
        assert_eq!(state.hearts, hearts - lost);
        if lost < hearts {
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
fn an_elite_loss_costs_a_heart_and_gives_nothing() {
    let data = data::embedded().unwrap();
    let mut state = run_at("warden", NodeKind::Elite);
    let before = state.gold;
    finish(&mut state, loss());
    assert_eq!(state.hearts, state.max_hearts - 1);
    assert_eq!(state.gold, before + data.balance.run.gold.elite_loss);
    assert!(state.relics.is_empty());
    assert_eq!(state.phase, Phase::Map);
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
    assert_eq!(duel.player(PLAYER).life, 23);
    assert_eq!(duel.player(ENEMY).life, 20);
    let names: Vec<&str> = duel
        .emblems
        .iter()
        .map(|emblem| emblem.def.name.as_str())
        .collect();
    assert_eq!(names, ["Firebrand's Fury", "War Banner"]);

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
fn a_rest_site_restores_a_heart_removes_a_card_or_is_left() {
    let mut arrived = run_at("warden", NodeKind::Rest);
    assert_eq!(arrived.phase, Phase::Rest);
    // At full hearts there is nothing to restore.
    assert!(!run_legal_actions(&arrived).contains(&RunAction::RestHeal));
    assert!(run_legal_actions(&arrived).contains(&RunAction::Leave));
    assert!(run_legal_actions(&arrived).contains(&RunAction::RemoveCard { card: 14 }));
    assert!(!run_legal_actions(&arrived).contains(&RunAction::RemoveCard { card: 15 }));
    arrived.hearts = 1;

    let mut healed = arrived.clone();
    let events = act(&mut healed, RunAction::RestHeal);
    assert_eq!(
        events,
        [RunEvent::HeartsChanged {
            change: 1,
            hearts: 2
        }]
    );
    assert_eq!(healed.hearts, 2);
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
    assert_eq!(trimmed.deck.len(), 14);
    assert_eq!(deck_count(&trimmed, removed.as_str()), copies - 1);
    // Only one thing may be done.
    assert_eq!(trimmed.phase, Phase::Map);
    assert_eq!(trimmed.hearts, 1);

    let mut left = arrived.clone();
    assert_eq!(act(&mut left, RunAction::Leave), []);
    assert_eq!(left.phase, Phase::Map);
    assert_eq!((left.hearts, left.deck.len()), (1, 15));
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

// ---- Boons and the boss ----

/// A run standing at the boss node, choosing boons.
fn at_the_boss(planeswalker: &str) -> RunState {
    let mut state = run(planeswalker, 3);
    assert!(travel_to(&mut state, NodeKind::Boss));
    assert!(matches!(state.phase, Phase::Boons { .. }));
    state
}

#[test]
fn each_remaining_heart_buys_one_boon_for_the_boss_duel() {
    let data = data::embedded().unwrap();
    let boons: Vec<ModifierId> = data
        .balance
        .run
        .boons
        .iter()
        .map(|boon| boon.id.clone())
        .collect();
    let mut state = at_the_boss("warden");
    state.hearts = 2;
    state.phase = Phase::Boons { picks_left: 2 };
    let mut expected: Vec<RunAction> = boons
        .iter()
        .map(|boon| RunAction::TakeBoon { boon: boon.clone() })
        .collect();
    expected.push(RunAction::StartBoss);
    assert_eq!(run_legal_actions(&state), expected);

    let life = RunAction::TakeBoon {
        boon: boons[0].clone(),
    };
    let events = act(&mut state, life.clone());
    assert_eq!(
        events,
        [RunEvent::BoonTaken {
            boon: boons[0].clone()
        }]
    );
    // Each boon once.
    assert!(!run_legal_actions(&state).contains(&life));
    act(
        &mut state,
        RunAction::TakeBoon {
            boon: boons[2].clone(),
        },
    );
    // No picks left.
    assert_eq!(run_legal_actions(&state), [RunAction::StartBoss]);
    assert_eq!(state.hearts, 2);

    act(&mut state, RunAction::StartBoss);
    let Phase::Duel { enemy, kind } = state.phase.clone() else {
        panic!("expected the boss duel");
    };
    assert_eq!(kind, DuelKind::Boss);
    assert_eq!(data.run.enemy(&enemy).unwrap().tier, EnemyTier::Boss);
    let duel = state.duel.as_ref().unwrap();
    // +5 for the player; the boss's own +5 and the boon's -5.
    assert_eq!(duel.player(PLAYER).life, 25);
    assert_eq!(duel.player(ENEMY).life, 20);
    assert!(
        duel.emblems
            .iter()
            .any(|emblem| emblem.controller == ENEMY && emblem.def.name == "The Pyre")
    );
}

#[test]
fn the_boss_can_be_fought_without_boons_and_picks_match_the_hearts() {
    let mut state = at_the_boss("firebrand");
    assert_eq!(
        state.phase,
        Phase::Boons {
            picks_left: state.hearts
        }
    );
    act(&mut state, RunAction::StartBoss);
    let duel = state.duel.as_ref().unwrap();
    assert_eq!(duel.player(PLAYER).life, 20);
    assert_eq!(duel.player(ENEMY).life, 25);
    assert_eq!(duel.permanents_of(PLAYER).count(), 0);
}

#[test]
fn beating_the_boss_wins_the_run_and_losing_to_it_ends_the_run() {
    let mut won = at_the_boss("warden");
    act(&mut won, RunAction::StartBoss);
    let events = finish(&mut won, win());
    assert_eq!(won.phase, Phase::Over { won: true });
    assert_eq!(events.last(), Some(&RunEvent::RunEnded { won: true }));
    assert_eq!(run_legal_actions(&won), []);

    let mut lost = at_the_boss("warden");
    act(&mut lost, RunAction::StartBoss);
    finish(&mut lost, loss());
    // Whatever hearts were left.
    assert_eq!(lost.hearts, lost.max_hearts);
    assert_eq!(lost.phase, Phase::Over { won: false });

    let mut conceded = at_the_boss("warden");
    act(&mut conceded, RunAction::StartBoss);
    act(&mut conceded, RunAction::Concede);
    assert_eq!(conceded.phase, Phase::Over { won: false });
}

#[test]
fn a_drawn_boss_duel_is_fought_again_with_the_boons_kept() {
    let mut state = at_the_boss("warden");
    let boon = data::embedded().unwrap().balance.run.boons[0].id.clone();
    act(&mut state, RunAction::TakeBoon { boon });
    act(&mut state, RunAction::StartBoss);
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
    assert_eq!(rematch.player(PLAYER).life, 25);
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
            assert!(state.hearts <= state.max_hearts);
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

    let content = run_content().unwrap();
    assert_eq!(content.planeswalkers.len(), 2);
    assert_eq!(content.boons.len(), 4);
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
