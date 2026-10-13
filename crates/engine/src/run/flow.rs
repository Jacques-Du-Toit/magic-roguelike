//! The run's rules: what may be done in each phase and what it does.

use crate::cards::{CardDef, CardDefId};
use crate::data::{self, GameData};
use crate::duel::{
    Action, CardInstance, DuelRng, GameEvent, GameResult, InternalError, PlayerId, apply_action,
    new_duel,
};

use super::defs::{
    EnemyDef, EnemyId, EnemyTier, EventDef, EventEffect, EventOptionDef, FeatId, ModifierDef,
    ModifierEffect, ModifierId, PlaneswalkerDef,
};
use super::feats::{DuelStats, FeatScore};
use super::loadout::{Loadout, duel_config, starting_deck};
use super::map::generate_map;
use super::rewards::{
    card_reward, discounted, draft_pack, event_cards, list_price, pick_one, random_relic,
    relic_offer, shop_stock, transmutations, treasure_relics,
};
use super::state::{
    DuelKind, DuelOutcome, GoldReason, NodeId, NodeKind, PLAYER, Phase, RunAction, RunConfig,
    RunError, RunEvent, RunState, Ware,
};

/// Starts a run: a map, the planeswalker's starting deck, no wounds.
pub fn new_run(config: RunConfig, seed: u64) -> Result<RunState, RunError> {
    let data = data::embedded()?;
    let planeswalker = data
        .run
        .planeswalker(&config.planeswalker)
        .ok_or_else(|| RunError::UnknownPlaneswalker(config.planeswalker.clone()))?;
    let emblem = config.emblem.or_else(|| planeswalker.emblem.clone());
    if let Some(unknown) = emblem.as_ref().filter(|id| data.run.emblem(id).is_none()) {
        return Err(RunError::UnknownEmblem(unknown.clone()));
    }
    let balance = &data.balance.run;
    let mut rng = DuelRng::from_seed(seed);
    let map = generate_map(&mut rng, &balance.map);
    Ok(RunState {
        seed,
        planeswalker: config.planeswalker,
        emblem,
        wounds: Vec::new(),
        gold: balance.starting_gold,
        deck: starting_deck(planeswalker),
        relics: Vec::new(),
        map,
        path: Vec::new(),
        phase: Phase::Map,
        duel: None,
        duel_stats: None,
        feats: Vec::new(),
        fought: Vec::new(),
        events_met: Vec::new(),
        next_duel: Vec::new(),
        rng,
    })
}

/// The ways an option of an event can be taken right now: one for each
/// thing the player may pick for it, or a single `None` when it asks for no
/// pick. Empty while one of its costs can't be paid or one of its gains
/// can't be had.
fn event_picks(
    state: &RunState,
    data: &GameData,
    option: &EventOptionDef,
    relics: &[ModifierId],
    cards: &[CardDefId],
) -> Result<Vec<Option<usize>>, InternalError> {
    let mut picks = vec![None];
    for effect in &option.effects {
        let possible = match effect {
            // Never the wound that ends the run: an event doesn't do that.
            EventEffect::TakeWound {} => {
                state.wounds.len() + 1 < usize::from(data.balance.run.max_wounds)
            }
            EventEffect::HealWounds {} => !state.wounds.is_empty(),
            EventEffect::PayGold { amount } => state.gold >= *amount,
            EventEffect::GainRelic {} => {
                picks = (0..relics.len()).map(Some).collect();
                !relics.is_empty()
            }
            EventEffect::GainCards {} => !cards.is_empty(),
            EventEffect::Transmute {} => {
                let colors = &planeswalker(state, data)?.colors;
                picks = state
                    .deck
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| !transmutations(data, colors, &card.card_id).is_empty())
                    .map(|(index, _)| Some(index))
                    .collect();
                !picks.is_empty()
            }
            EventEffect::GainGold { .. } | EventEffect::NextDuel { .. } => true,
        };
        if !possible {
            return Ok(Vec::new());
        }
    }
    Ok(picks)
}

fn planeswalker<'a>(
    state: &RunState,
    data: &'a GameData,
) -> Result<&'a PlaneswalkerDef, InternalError> {
    data.run
        .planeswalker(&state.planeswalker)
        .ok_or_else(|| InternalError::new(format!("no planeswalker `{}`", state.planeswalker)))
}

/// Every way to pick `picks` of `offered` things, as places in rising order.
fn combinations(offered: usize, picks: usize) -> Vec<Vec<usize>> {
    let mut all: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..picks {
        all = all
            .into_iter()
            .flat_map(|chosen| {
                let from = chosen.last().map_or(0, |last| last + 1);
                (from..offered).map(move |next| {
                    let mut longer = chosen.clone();
                    longer.push(next);
                    longer
                })
            })
            .collect();
    }
    all
}

/// How many of the feats offered before a duel are picked.
fn feat_picks(data: &GameData, offered: usize) -> usize {
    usize::from(data.balance.run.feats.picks).min(offered)
}

/// Whether the deck is big enough to lose a card.
fn can_remove_a_card(state: &RunState, data: &GameData) -> bool {
    state.deck.len() > data.balance.run.min_deck_cards as usize
}

/// Whether `deck[card]` can be upgraded: it has an upgraded side and this
/// copy isn't one yet.
fn can_upgrade(state: &RunState, data: &GameData, card: usize) -> bool {
    state.deck.get(card).is_some_and(|instance| {
        let has_side = |def: &CardDef| def.upgrade.is_some();
        !instance.upgraded && data.cards.get(&instance.card_id).is_some_and(has_side)
    })
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
        Phase::Feats { offered, .. } => {
            combinations(offered.len(), feat_picks(data, offered.len()))
                .into_iter()
                .map(|feats| RunAction::ChooseFeats { feats })
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
            if !state.wounds.is_empty() {
                actions.push(RunAction::HealWounds);
            }
            actions.extend(
                (0..state.deck.len())
                    .filter(|&card| can_upgrade(state, data, card))
                    .map(|card| RunAction::UpgradeCard { card }),
            );
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
        Phase::Draft { choices, .. } => {
            let mut actions: Vec<RunAction> = (0..choices.len())
                .map(|choice| RunAction::TakeDraft { choice })
                .collect();
            actions.push(RunAction::Leave);
            actions
        }
        // The gold is always there, so a treasure with no relic left to
        // offer still has a way on.
        Phase::Treasure { relics, .. } => {
            let mut actions: Vec<RunAction> = (0..relics.len())
                .map(|choice| RunAction::TakeTreasure { choice })
                .collect();
            actions.push(RunAction::TakeGold);
            actions
        }
        // An event can always be walked away from.
        Phase::Event {
            event,
            relics,
            cards,
        } => {
            let mut actions = Vec::new();
            let options = data.run.event(event).map(|def| def.options.as_slice());
            for (option, def) in options.unwrap_or_default().iter().enumerate() {
                for pick in event_picks(state, data, def, relics, cards)? {
                    actions.push(RunAction::TakeEvent { option, pick });
                }
            }
            actions.push(RunAction::Leave);
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
/// actions come from `ai_choose_action` on `RunState::duel`. What happened
/// is added to the duel's stats, which its feats are scored from.
pub fn apply_duel_action(
    state: &mut RunState,
    player: PlayerId,
    action: Action,
) -> Result<Vec<GameEvent>, RunError> {
    let duel = state.duel.as_mut().ok_or(RunError::NoDuel)?;
    let events = apply_action(duel, player, action)?;
    if let Some(stats) = state.duel_stats.as_mut() {
        stats.observe(duel, &events)?;
    }
    Ok(events)
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

/// The modifiers the player takes into a duel: a starting emblem, relics,
/// wounds, and whatever an event left for the next duel.
fn player_modifiers<'a>(
    state: &RunState,
    data: &'a GameData,
) -> Result<Vec<&'a ModifierDef>, InternalError> {
    let mut modifiers = Vec::new();
    if let Some(id) = &state.emblem {
        modifiers.push(
            data.run
                .emblem(id)
                .ok_or_else(|| InternalError::new(format!("no starting emblem `{id}`")))?,
        );
    }
    for id in &state.relics {
        modifiers.push(
            data.run
                .relic(id)
                .ok_or_else(|| InternalError::new(format!("no relic `{id}`")))?,
        );
    }
    for id in &state.next_duel {
        modifiers.push(
            data.run
                .next_duel_modifier(id)
                .ok_or_else(|| InternalError::new(format!("no event modifier `{id}`")))?,
        );
    }
    for id in &state.wounds {
        modifiers.push(
            data.run
                .wound(id)
                .ok_or_else(|| InternalError::new(format!("no wound `{id}`")))?,
        );
    }
    Ok(modifiers)
}

/// Picks an enemy of the tier, one not met before if possible. With no
/// hard enemies in the data, a normal one stands in.
fn pick_enemy<'a>(
    state: &mut RunState,
    data: &'a GameData,
    tier: EnemyTier,
) -> Result<&'a EnemyDef, InternalError> {
    let tier = if tier == EnemyTier::Hard && data.run.enemies_of(tier).next().is_none() {
        EnemyTier::Normal
    } else {
        tier
    };
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

/// What happens on arriving at a duel: the feats for it are offered, and
/// the duel starts once they are picked. With no feats to offer it starts
/// at once.
fn offer_feats(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
    enemy: &EnemyDef,
    kind: DuelKind,
) -> Result<(), RunError> {
    // Places in the character's pool, taken at random and shown in the
    // pool's order.
    let pool: Vec<&FeatId> = data
        .run
        .feats_for(&state.planeswalker)
        .map(|feat| &feat.id)
        .collect();
    let mut left: Vec<usize> = (0..pool.len()).collect();
    let mut taken = Vec::new();
    for _ in 0..data.balance.run.feats.offered {
        if left.is_empty() {
            break;
        }
        let pick = state
            .rng
            .below(u32::try_from(left.len()).unwrap_or(u32::MAX)) as usize;
        taken.push(left.remove(pick));
    }
    taken.sort_unstable();
    let offered: Vec<FeatId> = taken
        .into_iter()
        .filter_map(|place| pool.get(place).map(|&id| id.clone()))
        .collect();
    if offered.is_empty() {
        return start_duel(state, events, data, enemy, kind, Vec::new());
    }
    state.phase = Phase::Feats {
        enemy: enemy.id.clone(),
        kind,
        offered,
    };
    events.push(RunEvent::FeatsOffered {
        enemy: enemy.id.clone(),
        kind,
    });
    Ok(())
}

fn start_duel(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
    enemy: &EnemyDef,
    kind: DuelKind,
    feats: Vec<FeatId>,
) -> Result<(), RunError> {
    let planeswalker = planeswalker(state, data)?;
    let player = Loadout::player(
        &state.deck,
        &planeswalker.colors,
        planeswalker.commander.as_ref(),
        player_modifiers(state, data)?,
        &data.cards,
        &data.balance,
    )?;
    let config = duel_config(player, Loadout::enemy(enemy, &data.cards)?, &data.balance);
    let seed = state.rng.next_seed();
    let duel = new_duel(config, seed)?;
    state.duel_stats = Some(DuelStats::new(&duel, PLAYER)?);
    state.duel = Some(duel);
    state.feats = feats;
    // What an event left for the next duel has now been used.
    state.next_duel.clear();
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
            offer_feats(state, events, data, enemy, DuelKind::Normal)?;
        }
        NodeKind::Elite => {
            let enemy = pick_enemy(state, data, EnemyTier::Elite)?;
            offer_feats(state, events, data, enemy, DuelKind::Elite)?;
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
            price_shop(state, data);
        }
        NodeKind::Treasure => {
            let relics = treasure_relics(&mut state.rng, data, &state.relics);
            state.phase = Phase::Treasure {
                relics,
                gold: balance.treasure.gold,
            };
        }
        NodeKind::Event => match pick_event(state, data) {
            Some(event) => {
                let colors = &planeswalker(state, data)?.colors;
                let relics = relic_offer(&mut state.rng, data, &state.relics, event.relics);
                let cards = event_cards(&mut state.rng, data, colors, event.cards);
                if !state.events_met.contains(&event.id) {
                    state.events_met.push(event.id.clone());
                }
                state.phase = Phase::Event {
                    event: event.id.clone(),
                    relics,
                    cards,
                };
            }
            // With no events in the data the node is passed.
            None => state.phase = Phase::Map,
        },
        NodeKind::Draft => {
            let colors = &planeswalker(state, data)?.colors;
            let choices = draft_pack(&mut state.rng, data, colors);
            // An empty pack has nothing to choose from: the node is passed.
            state.phase = if choices.is_empty() {
                Phase::Map
            } else {
                Phase::Draft {
                    choices,
                    picks_left: balance.draft.picks,
                }
            };
        }
        // The boss is only to be beaten: no feats.
        NodeKind::Boss => {
            let boss = pick_enemy(state, data, EnemyTier::Boss)?;
            start_duel(state, events, data, boss, DuelKind::Boss, Vec::new())?;
        }
    }
    Ok(())
}

/// Picks an event, one not met before in this run if possible.
fn pick_event<'a>(state: &mut RunState, data: &'a GameData) -> Option<&'a EventDef> {
    let all: Vec<&EventDef> = data.run.events.iter().collect();
    let fresh: Vec<&EventDef> = all
        .iter()
        .copied()
        .filter(|event| !state.events_met.contains(&event.id))
        .collect();
    let pool = if fresh.is_empty() { all } else { fresh };
    if pool.is_empty() {
        return None;
    }
    let pick = state
        .rng
        .below(u32::try_from(pool.len()).unwrap_or(u32::MAX)) as usize;
    pool.get(pick).copied()
}

/// Heals every wound carried, oldest first.
fn heal_wounds(state: &mut RunState, events: &mut Vec<RunEvent>) {
    for wound in state.wounds.drain(..) {
        events.push(RunEvent::WoundHealed { wound });
    }
}

/// The player takes a wound, one of `data/run/wounds.json` at random.
fn take_wound(
    state: &mut RunState,
    events: &mut Vec<RunEvent>,
    data: &GameData,
) -> Result<(), InternalError> {
    let wounds = &data.run.wounds;
    let pick = state
        .rng
        .below(u32::try_from(wounds.len()).unwrap_or(u32::MAX)) as usize;
    let wound = wounds
        .get(pick)
        .ok_or_else(|| InternalError::new("there are no wounds to take"))?;
    state.wounds.push(wound.id.clone());
    events.push(RunEvent::WoundTaken {
        wound: wound.id.clone(),
    });
    Ok(())
}

/// What the player's relics do to the run itself.
fn relic_effects<'a>(
    state: &'a RunState,
    data: &'a GameData,
) -> impl Iterator<Item = &'a ModifierEffect> {
    state
        .relics
        .iter()
        .filter_map(|id| data.run.relic(id))
        .flat_map(|relic| &relic.effects)
}

/// Extra gold from relics for a duel won.
fn relic_gold(state: &RunState, data: &GameData) -> u32 {
    relic_effects(state, data)
        .map(|effect| match effect {
            ModifierEffect::GoldPerWin { amount } => *amount,
            _ => 0,
        })
        .sum()
}

/// How many percent the player's relics take off a shop's prices.
fn shop_discount(state: &RunState, data: &GameData) -> u32 {
    relic_effects(state, data)
        .map(|effect| match effect {
            ModifierEffect::ShopDiscount { percent } => *percent,
            _ => 0,
        })
        .sum::<u32>()
        .min(100)
}

/// Sets what the shop the player is in charges: its list prices less the
/// player's discounts. What is already sold keeps the price paid.
fn price_shop(state: &mut RunState, data: &GameData) {
    let percent = shop_discount(state, data);
    if let Phase::Shop {
        items,
        removal_price,
        ..
    } = &mut state.phase
    {
        for item in items.iter_mut().filter(|item| !item.sold) {
            item.price = discounted(list_price(data, &item.ware), percent);
        }
        *removal_price = discounted(data.balance.run.shop.prices.removal, percent);
    }
}

fn gain_relic(state: &mut RunState, events: &mut Vec<RunEvent>, relic: ModifierId) {
    state.relics.push(relic.clone());
    events.push(RunEvent::RelicGained { relic });
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
    let stats = state.duel_stats.take();
    let feats = std::mem::take(&mut state.feats);
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
            // The boss has to be beaten: fight again.
            let enemy = data
                .run
                .enemy(&enemy)
                .ok_or_else(|| InternalError::new(format!("no enemy `{enemy}`")))?;
            start_duel(state, events, data, enemy, DuelKind::Boss, Vec::new())?;
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
            take_wound(state, events, data)?;
            if state.wounds.len() >= usize::from(balance.max_wounds) {
                end_run(state, events, false);
            } else {
                // A loss still pays a little: fewer cards to pick from.
                let colors = &planeswalker(state, data)?.colors;
                let choices = card_reward(
                    &mut state.rng,
                    data,
                    colors,
                    &balance.rewards.duel,
                    balance.rewards.loss_choices,
                );
                state.phase = Phase::Reward { choices };
            }
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
            if kind == DuelKind::Elite
                && let Some(relic) = random_relic(&mut state.rng, data, &state.relics)
            {
                gain_relic(state, events, relic);
            }
            // The feats set how good the card reward is.
            let points = score_feats(events, data, &feats, stats.as_ref())?;
            let reward = balance
                .feats
                .reward(points)
                .ok_or_else(|| InternalError::new(format!("no reward for {points} points")))?;
            change_gold(state, events, i64::from(reward.gold), GoldReason::Feats);
            let ladder = [
                &balance.rewards.duel,
                &balance.rewards.elite,
                &balance.rewards.top,
            ];
            let step = usize::from(kind == DuelKind::Elite) + usize::from(reward.rarity_steps);
            let weights = ladder[step.min(ladder.len() - 1)];
            let colors = &planeswalker(state, data)?.colors;
            let choices = card_reward(&mut state.rng, data, colors, weights, reward.choices);
            state.phase = Phase::Reward { choices };
        }
    }
    Ok(())
}

/// Scores the feats picked for a duel that was won, reports it, and
/// returns their points in all.
fn score_feats(
    events: &mut Vec<RunEvent>,
    data: &GameData,
    feats: &[FeatId],
    stats: Option<&DuelStats>,
) -> Result<u32, InternalError> {
    if feats.is_empty() {
        return Ok(0);
    }
    let stats = stats.ok_or_else(|| InternalError::new("a duel with feats has no stats"))?;
    let scores = scores(data, feats, stats)?;
    let points = scores.iter().map(|score| u32::from(score.score)).sum();
    events.push(RunEvent::FeatsScored { scores, points });
    Ok(points)
}

/// What each of `feats` scores for a duel with these stats.
fn scores(
    data: &GameData,
    feats: &[FeatId],
    stats: &DuelStats,
) -> Result<Vec<FeatScore>, InternalError> {
    feats
        .iter()
        .map(|id| {
            let feat = data
                .run
                .feat(id)
                .ok_or_else(|| InternalError::new(format!("no feat `{id}`")))?;
            Ok(FeatScore {
                feat: id.clone(),
                score: feat.score(stats),
                value: feat.value(stats),
            })
        })
        .collect()
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
        (
            RunAction::ChooseFeats { feats },
            Phase::Feats {
                enemy,
                kind,
                offered,
            },
        ) => {
            let mut chosen = Vec::new();
            for place in feats {
                chosen.push(offered.get(*place).ok_or_else(mismatch)?.clone());
            }
            let enemy = data
                .run
                .enemy(&enemy)
                .ok_or_else(|| InternalError::new(format!("no enemy `{enemy}`")))?;
            start_duel(state, events, data, enemy, kind, chosen)?;
        }
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
        (RunAction::HealWounds, Phase::Rest) => {
            if state.wounds.is_empty() {
                return Err(mismatch().into());
            }
            heal_wounds(state, events);
            state.phase = Phase::Map;
        }
        (RunAction::UpgradeCard { card }, Phase::Rest) => {
            if !can_upgrade(state, data, *card) {
                return Err(mismatch().into());
            }
            let instance = state.deck.get_mut(*card).ok_or_else(mismatch)?;
            instance.upgraded = true;
            events.push(RunEvent::CardUpgraded {
                card: instance.card_id.clone(),
            });
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
            state.phase = Phase::Shop {
                items,
                removal_price,
                removals_left,
            };
            match ware {
                Ware::Card { card } => add_card(state, events, card),
                Ware::Relic { relic } => {
                    gain_relic(state, events, relic);
                    // A relic that lowers prices does so from now on, in
                    // this shop too.
                    price_shop(state, data);
                }
            }
        }
        (RunAction::TakeTreasure { choice }, Phase::Treasure { relics, .. }) => {
            let relic = relics.get(*choice).ok_or_else(mismatch)?.clone();
            gain_relic(state, events, relic);
            state.phase = Phase::Map;
        }
        (RunAction::TakeGold, Phase::Treasure { gold, .. }) => {
            change_gold(state, events, i64::from(gold), GoldReason::Treasure);
            state.phase = Phase::Map;
        }
        (
            RunAction::TakeDraft { choice },
            Phase::Draft {
                mut choices,
                picks_left,
            },
        ) => {
            if *choice >= choices.len() {
                return Err(mismatch().into());
            }
            add_card(state, events, choices.remove(*choice));
            let picks_left = picks_left.saturating_sub(1);
            state.phase = if picks_left == 0 || choices.is_empty() {
                Phase::Map
            } else {
                Phase::Draft {
                    choices,
                    picks_left,
                }
            };
        }
        (
            RunAction::TakeEvent { option, pick },
            Phase::Event {
                event,
                relics,
                cards,
            },
        ) => {
            let option = data
                .run
                .event(&event)
                .and_then(|def| def.options.get(*option))
                .ok_or_else(mismatch)?;
            state.phase = Phase::Map;
            for effect in &option.effects {
                match effect {
                    EventEffect::TakeWound {} => take_wound(state, events, data)?,
                    EventEffect::HealWounds {} => heal_wounds(state, events),
                    EventEffect::GainGold { amount } => {
                        change_gold(state, events, i64::from(*amount), GoldReason::Event);
                    }
                    EventEffect::PayGold { amount } => {
                        change_gold(state, events, -i64::from(*amount), GoldReason::Event);
                    }
                    EventEffect::GainRelic {} => {
                        let relic = pick.and_then(|pick| relics.get(pick));
                        gain_relic(state, events, relic.ok_or_else(mismatch)?.clone());
                    }
                    EventEffect::GainCards {} => {
                        for card in &cards {
                            add_card(state, events, card.clone());
                        }
                    }
                    EventEffect::Transmute {} => {
                        let index = pick.ok_or_else(mismatch)?;
                        let old = state.deck.get(index).ok_or_else(mismatch)?;
                        let colors = &planeswalker(state, data)?.colors;
                        let candidates = transmutations(data, colors, &old.card_id);
                        let new = pick_one(&mut state.rng, &candidates).ok_or_else(mismatch)?;
                        remove_card(state, events, index)?;
                        add_card(state, events, new);
                    }
                    EventEffect::NextDuel { modifier } => {
                        state.next_duel.push(modifier.id.clone());
                        events.push(RunEvent::NextDuelModified {
                            modifier: modifier.id.clone(),
                        });
                    }
                }
            }
        }
        (
            RunAction::Leave,
            Phase::Rest | Phase::Shop { .. } | Phase::Draft { .. } | Phase::Event { .. },
        ) => {
            state.phase = Phase::Map;
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

/// How the feats picked for the duel in progress stand: what each would
/// score if the duel were won now. Empty when no duel is on, and for the
/// boss.
pub fn feat_scores(state: &RunState) -> Result<Vec<FeatScore>, RunError> {
    let data = data::embedded()?;
    match &state.duel_stats {
        Some(stats) => Ok(scores(data, &state.feats, stats)?),
        None => Ok(Vec::new()),
    }
}
