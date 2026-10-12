//! The run's rules: what may be done in each phase and what it does.

use crate::cards::CardDefId;
use crate::data::{self, GameData};
use crate::duel::{
    Action, CardInstance, DuelRng, GameEvent, GameResult, InternalError, PlayerId, apply_action,
    new_duel,
};

use super::defs::{EnemyDef, EnemyId, EnemyTier, ModifierDef, ModifierEffect, PlaneswalkerDef};
use super::loadout::{Loadout, duel_config, starting_deck};
use super::map::generate_map;
use super::rewards::{card_reward, random_relic, shop_stock};
use super::state::{
    DuelKind, DuelOutcome, GoldReason, NodeId, NodeKind, PLAYER, Phase, RunAction, RunConfig,
    RunError, RunEvent, RunState, Ware,
};

/// Starts a run: a map, the planeswalker's starting deck, full hearts.
pub fn new_run(config: RunConfig, seed: u64) -> Result<RunState, RunError> {
    let data = data::embedded()?;
    let planeswalker = data
        .run
        .planeswalker(&config.planeswalker)
        .ok_or_else(|| RunError::UnknownPlaneswalker(config.planeswalker.clone()))?;
    let emblem = config.emblem.unwrap_or_else(|| planeswalker.emblem.clone());
    if data.run.emblem(&emblem).is_none() {
        return Err(RunError::UnknownEmblem(emblem));
    }
    let balance = &data.balance.run;
    let mut rng = DuelRng::from_seed(seed);
    let map = generate_map(&mut rng, &balance.map);
    Ok(RunState {
        seed,
        planeswalker: config.planeswalker,
        emblem,
        hearts: balance.hearts,
        max_hearts: balance.hearts,
        gold: balance.starting_gold,
        deck: starting_deck(planeswalker),
        relics: Vec::new(),
        boons: Vec::new(),
        map,
        path: Vec::new(),
        phase: Phase::Map,
        duel: None,
        fought: Vec::new(),
        rng,
    })
}

fn planeswalker<'a>(
    state: &RunState,
    data: &'a GameData,
) -> Result<&'a PlaneswalkerDef, InternalError> {
    data.run
        .planeswalker(&state.planeswalker)
        .ok_or_else(|| InternalError::new(format!("no planeswalker `{}`", state.planeswalker)))
}

/// Whether the deck is big enough to lose a card.
fn can_remove_a_card(state: &RunState, data: &GameData) -> bool {
    state.deck.len() > data.balance.run.min_deck_cards as usize
}

/// Every action the player may take in the run right now. While a duel is
/// being played its own actions come from the duel; the run only offers to
/// concede it, or to leave it once it is over. Empty when the run has
/// ended.
pub fn run_legal_actions(state: &RunState) -> Vec<RunAction> {
    // An error here means the embedded data is broken; no action is legal.
    try_legal_actions(state).unwrap_or_default()
}

fn try_legal_actions(state: &RunState) -> Result<Vec<RunAction>, RunError> {
    let data = data::embedded()?;
    let removals = || -> Vec<RunAction> {
        if !can_remove_a_card(state, data) {
            return Vec::new();
        }
        (0..state.deck.len())
            .map(|card| RunAction::RemoveCard { card })
            .collect()
    };
    Ok(match &state.phase {
        Phase::Map => {
            let next: Vec<NodeId> = match state.position() {
                Some(position) => state
                    .map
                    .node(position)
                    .map(|node| node.next.clone())
                    .unwrap_or_default(),
                None => state
                    .map
                    .nodes
                    .iter()
                    .filter(|node| node.row == 0)
                    .map(|node| node.id)
                    .collect(),
            };
            next.into_iter()
                .map(|node| RunAction::Travel { node })
                .collect()
        }
        Phase::Duel { .. } => match &state.duel {
            Some(duel) if duel.result.is_none() => vec![RunAction::Concede],
            _ => vec![RunAction::FinishDuel],
        },
        Phase::Reward { choices } => {
            let mut actions: Vec<RunAction> = (0..choices.len())
                .map(|choice| RunAction::TakeReward { choice })
                .collect();
            actions.push(RunAction::SkipReward);
            actions
        }
        Phase::Rest => {
            let mut actions = Vec::new();
            if state.hearts < state.max_hearts {
                actions.push(RunAction::RestHeal);
            }
            actions.extend(removals());
            actions.push(RunAction::Leave);
            actions
        }
        Phase::Shop {
            items,
            removal_price,
            removals_left,
        } => {
            let mut actions: Vec<RunAction> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| !item.sold && item.price <= state.gold)
                .map(|(item, _)| RunAction::Buy { item })
                .collect();
            if *removals_left > 0 && *removal_price <= state.gold {
                actions.extend(removals());
            }
            actions.push(RunAction::Leave);
            actions
        }
        Phase::Boons { picks_left } => {
            let mut actions = Vec::new();
            if *picks_left > 0 {
                for boon in &data.balance.run.boons {
                    if !state.boons.contains(&boon.id) {
                        actions.push(RunAction::TakeBoon {
                            boon: boon.id.clone(),
                        });
                    }
                }
            }
            actions.push(RunAction::StartBoss);
            actions
        }
        Phase::Over { .. } => Vec::new(),
    })
}

/// Applies one of the legal run actions and returns what happened. On an
/// error the state is left exactly as it was.
pub fn apply_run_action(
    state: &mut RunState,
    action: RunAction,
) -> Result<Vec<RunEvent>, RunError> {
    if !try_legal_actions(state)?.contains(&action) {
        return Err(RunError::IllegalAction(action));
    }
    // Work on a copy, so a failure part-way can't leave a half-updated run.
    let mut next = state.clone();
    let mut events = Vec::new();
    perform(&mut next, &mut events, &action)?;
    *state = next;
    Ok(events)
}

/// Applies an action of the duel in progress for `player`. The enemy's
/// actions come from `ai_choose_action` on `RunState::duel`.
pub fn apply_duel_action(
    state: &mut RunState,
    player: PlayerId,
    action: Action,
) -> Result<Vec<GameEvent>, RunError> {
    let duel = state.duel.as_mut().ok_or(RunError::NoDuel)?;
    Ok(apply_action(duel, player, action)?)
}

fn change_gold(state: &mut RunState, events: &mut Vec<RunEvent>, change: i64, reason: GoldReason) {
    if change == 0 {
        return;
    }
    let gold = (i64::from(state.gold) + change).clamp(0, i64::from(u32::MAX));
    state.gold = u32::try_from(gold).unwrap_or(u32::MAX);
    events.push(RunEvent::GoldChanged {
        change,
        gold: state.gold,
        reason,
    });
}

fn end_run(state: &mut RunState, events: &mut Vec<RunEvent>, won: bool) {
    state.phase = Phase::Over { won };
    events.push(RunEvent::RunEnded { won });
}

/// The modifiers the player takes into a duel: the starting emblem, relics,
/// and the boons too if it is the boss.
fn player_modifiers<'a>(
    state: &RunState,
    data: &'a GameData,
    kind: DuelKind,
) -> Result<Vec<&'a ModifierDef>, InternalError> {
    let emblem = data
        .run
        .emblem(&state.emblem)
        .ok_or_else(|| InternalError::new(format!("no starting emblem `{}`", state.emblem)))?;
    let mut modifiers = vec![emblem];
    for id in &state.relics {
        modifiers.push(
            data.run
                .relic(id)
                .ok_or_else(|| InternalError::new(format!("no relic `{id}`")))?,
        );
    }
    if kind == DuelKind::Boss {
        for id in &state.boons {
            modifiers.push(
                data.balance
                    .run
                    .boons
                    .iter()
                    .find(|boon| boon.id == *id)
                    .ok_or_else(|| InternalError::new(format!("no boon `{id}`")))?,
            );
        }
    }
    Ok(modifiers)
}

/// Picks an enemy of the tier, one not met before if possible.
fn pick_enemy<'a>(
    state: &mut RunState,
    data: &'a GameData,
    tier: EnemyTier,
) -> Result<&'a EnemyDef, InternalError> {
    let all: Vec<&EnemyDef> = data.run.enemies_of(tier).collect();
    let fresh: Vec<&EnemyDef> = all
        .iter()
        .copied()
        .filter(|enemy| !state.fought.contains(&enemy.id))
        .collect();
    let pool = if fresh.is_empty() { all } else { fresh };
    let pick = state
        .rng
        .below(u32::try_from(pool.len()).unwrap_or(u32::MAX)) as usize;
    pool.get(pick)
        .copied()
        .ok_or_else(|| InternalError::new(format!("no enemy of tier {tier:?}")))
}

fn start_duel(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
    enemy: &EnemyDef,
    kind: DuelKind,
) -> Result<(), RunError> {
    let planeswalker = planeswalker(state, data)?;
    let player = Loadout::player(
        &state.deck,
        &planeswalker.colors,
        planeswalker.commander.as_ref(),
        player_modifiers(state, data, kind)?,
        &data.cards,
        &data.balance,
    )?;
    let config = duel_config(player, Loadout::enemy(enemy, &data.cards)?, &data.balance);
    let seed = state.rng.next_seed();
    state.duel = Some(new_duel(config, seed)?);
    state.phase = Phase::Duel {
        enemy: enemy.id.clone(),
        kind,
    };
    if !state.fought.contains(&enemy.id) {
        state.fought.push(enemy.id.clone());
    }
    events.push(RunEvent::DuelStarted {
        enemy: enemy.id.clone(),
        kind,
    });
    Ok(())
}

fn travel(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
    node: NodeId,
) -> Result<(), RunError> {
    let (kind, row) = {
        let node = state
            .map
            .node(node)
            .ok_or_else(|| InternalError::new(format!("no node {node:?}")))?;
        (node.kind, usize::from(node.row))
    };
    state.path.push(node);
    events.push(RunEvent::Travelled { node, kind });
    let balance = &data.balance.run;
    match kind {
        NodeKind::Duel => {
            let tier = balance
                .map
                .rows
                .get(row)
                .map_or(EnemyTier::Normal, |row| row.duel_tier);
            let enemy = pick_enemy(state, data, tier)?;
            start_duel(state, events, data, enemy, DuelKind::Normal)?;
        }
        NodeKind::Elite => {
            let enemy = pick_enemy(state, data, EnemyTier::Elite)?;
            start_duel(state, events, data, enemy, DuelKind::Elite)?;
        }
        NodeKind::Rest => state.phase = Phase::Rest,
        NodeKind::Shop => {
            let colors = &planeswalker(state, data)?.colors;
            let items = shop_stock(&mut state.rng, data, colors, &state.relics);
            state.phase = Phase::Shop {
                items,
                removal_price: balance.shop.prices.removal,
                removals_left: balance.shop.removals,
            };
        }
        NodeKind::Boss => {
            state.phase = Phase::Boons {
                picks_left: state.hearts,
            };
        }
    }
    Ok(())
}

/// Extra gold from relics for a duel won.
fn relic_gold(state: &RunState, data: &GameData) -> u32 {
    state
        .relics
        .iter()
        .filter_map(|id| data.run.relic(id))
        .flat_map(|relic| &relic.effects)
        .map(|effect| match effect {
            ModifierEffect::GoldPerWin { amount } => *amount,
            _ => 0,
        })
        .sum()
}

fn lose_heart(state: &mut RunState, events: &mut Vec<RunEvent>) {
    state.hearts = state.hearts.saturating_sub(1);
    events.push(RunEvent::HeartsChanged {
        change: -1,
        hearts: state.hearts,
    });
    if state.hearts == 0 {
        end_run(state, events, false);
    } else {
        state.phase = Phase::Map;
    }
}

/// Ends the duel with `outcome` and applies what follows from it.
fn finish_duel(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
    outcome: DuelOutcome,
) -> Result<(), RunError> {
    let Phase::Duel { enemy, kind } = state.phase.clone() else {
        return Err(RunError::NoDuel);
    };
    state.duel.take().ok_or(RunError::NoDuel)?;
    events.push(RunEvent::DuelEnded {
        enemy: enemy.clone(),
        kind,
        outcome,
    });
    let balance = &data.balance.run;
    let gold = &balance.gold;

    match (kind, outcome) {
        (DuelKind::Boss, DuelOutcome::Won) => end_run(state, events, true),
        (DuelKind::Boss, DuelOutcome::Lost) => end_run(state, events, false),
        (DuelKind::Boss, DuelOutcome::Drawn) => {
            // The boss has to be beaten: fight again, boons kept.
            let enemy = data
                .run
                .enemy(&enemy)
                .ok_or_else(|| InternalError::new(format!("no enemy `{enemy}`")))?;
            start_duel(state, events, data, enemy, DuelKind::Boss)?;
        }
        (_, DuelOutcome::Drawn) => {
            change_gold(state, events, i64::from(gold.draw), GoldReason::DuelDrawn);
            state.phase = Phase::Map;
        }
        (_, DuelOutcome::Lost) => {
            let consolation = match kind {
                DuelKind::Elite => gold.elite_loss,
                _ => gold.duel_loss,
            };
            change_gold(state, events, i64::from(consolation), GoldReason::DuelLost);
            lose_heart(state, events);
        }
        (_, DuelOutcome::Won) => {
            let prize = match kind {
                DuelKind::Elite => gold.elite_win,
                _ => gold.duel_win,
            };
            change_gold(state, events, i64::from(prize), GoldReason::DuelWon);
            change_gold(
                state,
                events,
                i64::from(relic_gold(state, data)),
                GoldReason::Relic,
            );
            let weights = if kind == DuelKind::Elite {
                if let Some(relic) = random_relic(&mut state.rng, data, &state.relics) {
                    state.relics.push(relic.clone());
                    events.push(RunEvent::RelicGained { relic });
                }
                &balance.rewards.elite
            } else {
                &balance.rewards.duel
            };
            let colors = &planeswalker(state, data)?.colors;
            let choices = card_reward(&mut state.rng, data, colors, weights);
            state.phase = Phase::Reward { choices };
        }
    }
    Ok(())
}

fn add_card(state: &mut RunState, events: &mut Vec<RunEvent>, card: CardDefId) {
    state.deck.push(CardInstance::new(card.clone()));
    events.push(RunEvent::CardAdded { card });
}

fn remove_card(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    index: usize,
) -> Result<(), InternalError> {
    if index >= state.deck.len() {
        return Err(InternalError::new(format!("no card {index} in the deck")));
    }
    let removed = state.deck.remove(index);
    events.push(RunEvent::CardRemoved {
        card: removed.card_id,
    });
    Ok(())
}

/// Carries out an action already known to be legal.
fn perform(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    action: &RunAction,
) -> Result<(), RunError> {
    let data = data::embedded()?;
    let mismatch = || InternalError::new(format!("{action:?} does not fit the phase"));
    match (action, state.phase.clone()) {
        (RunAction::Travel { node }, Phase::Map) => travel(state, events, data, *node)?,
        (RunAction::Concede, Phase::Duel { .. }) => {
            finish_duel(state, events, data, DuelOutcome::Lost)?;
        }
        (RunAction::FinishDuel, Phase::Duel { .. }) => {
            let result = state.duel.as_ref().and_then(|duel| duel.result);
            let outcome = match result.ok_or_else(mismatch)? {
                GameResult::Win { player, .. } if player == PLAYER => DuelOutcome::Won,
                GameResult::Win { .. } => DuelOutcome::Lost,
                GameResult::Draw => DuelOutcome::Drawn,
            };
            finish_duel(state, events, data, outcome)?;
        }
        (RunAction::TakeReward { choice }, Phase::Reward { choices }) => {
            let card = choices.get(*choice).ok_or_else(mismatch)?.clone();
            add_card(state, events, card);
            state.phase = Phase::Map;
        }
        (RunAction::SkipReward, Phase::Reward { .. }) => state.phase = Phase::Map,
        (RunAction::RestHeal, Phase::Rest) => {
            let restored = data
                .balance
                .run
                .rest
                .hearts_restored
                .min(state.max_hearts - state.hearts);
            state.hearts += restored;
            events.push(RunEvent::HeartsChanged {
                change: i8::try_from(restored).unwrap_or(i8::MAX),
                hearts: state.hearts,
            });
            state.phase = Phase::Map;
        }
        (RunAction::RemoveCard { card }, Phase::Rest) => {
            remove_card(state, events, *card)?;
            state.phase = Phase::Map;
        }
        (
            RunAction::RemoveCard { card },
            Phase::Shop {
                items,
                removal_price,
                removals_left,
            },
        ) => {
            remove_card(state, events, *card)?;
            change_gold(
                state,
                events,
                -i64::from(removal_price),
                GoldReason::Removal,
            );
            state.phase = Phase::Shop {
                items,
                removal_price,
                removals_left: removals_left.saturating_sub(1),
            };
        }
        (
            RunAction::Buy { item },
            Phase::Shop {
                mut items,
                removal_price,
                removals_left,
            },
        ) => {
            let bought = items.get_mut(*item).ok_or_else(mismatch)?;
            bought.sold = true;
            let (ware, price) = (bought.ware.clone(), bought.price);
            change_gold(state, events, -i64::from(price), GoldReason::Purchase);
            match ware {
                Ware::Card { card } => add_card(state, events, card),
                Ware::Relic { relic } => {
                    state.relics.push(relic.clone());
                    events.push(RunEvent::RelicGained { relic });
                }
            }
            state.phase = Phase::Shop {
                items,
                removal_price,
                removals_left,
            };
        }
        (RunAction::Leave, Phase::Rest | Phase::Shop { .. }) => state.phase = Phase::Map,
        (RunAction::TakeBoon { boon }, Phase::Boons { picks_left }) => {
            state.boons.push(boon.clone());
            events.push(RunEvent::BoonTaken { boon: boon.clone() });
            state.phase = Phase::Boons {
                picks_left: picks_left.saturating_sub(1),
            };
        }
        (RunAction::StartBoss, Phase::Boons { .. }) => {
            let boss = pick_enemy(state, data, EnemyTier::Boss)?;
            start_duel(state, events, data, boss, DuelKind::Boss)?;
        }
        _ => return Err(mismatch().into()),
    }
    Ok(())
}

/// The id of the enemy being fought, if a duel is on.
pub fn current_enemy(state: &RunState) -> Option<&EnemyId> {
    match &state.phase {
        Phase::Duel { enemy, .. } => Some(enemy),
        _ => None,
    }
}
