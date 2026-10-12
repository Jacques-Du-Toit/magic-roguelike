//! Version 2 of the AI: it tries what it might do in imagined games.
//!
//! For each thing it might do it:
//! 1. fills in the cards it can't see with a guess (a "world");
//! 2. does the thing on a copy of that world and lets version 1 play both
//!    sides from there, about one turn cycle on;
//! 3. scores where that leaves it.
//!
//! Then it does whatever scores best on average over several worlds. Every
//! candidate is tried in the same worlds, so luck doesn't pick the winner.
//!
//! It sees only what its player may see. Its guesses use dice of their
//! own, seeded from what the player can see, so the same position is always
//! thought about in the same way and the duel's own dice are never touched.
//!
//! Decisions it doesn't search (mulligans, scrying, discards, and for now
//! attacks and blocks) are version 1's.

use std::collections::BTreeSet;

use super::battle;
use super::distinct::distinct;
use super::{LOOKAHEAD, Thinking, VERSION_1, best_land, evaluate, heuristic, land_color, outcome};
use crate::balance::AiSearchBalance;
use crate::cards::{CardDef, Color, Supertype};
use crate::data;
use crate::duel::{
    Action, Decision, DuelRng, DuelState, InternalError, ObjectId, PlayerId, Zone, apply_trusted,
    legal_actions, player_view, should_auto_pass,
};

pub(super) fn choose(
    state: &DuelState,
    player: PlayerId,
    decision: Decision,
    actions: &[Action],
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    match decision {
        Decision::Priority { .. } => priority(state, player, decision, actions, settings),
        Decision::ChooseTargets { .. } if actions.len() > 1 => {
            let preferred = heuristic(state, player, decision, actions, VERSION_1)?;
            let ranked = ranked_by_glance(state, player, actions, None)?;
            let candidates = pick_candidates(&ranked, settings);
            best_in_rollouts(state, player, &candidates, &preferred, settings)
        }
        Decision::DeclareAttackers { .. } => {
            battle::declare_attackers(state, player, actions, settings)
        }
        Decision::DeclareBlockers { .. } => {
            battle::declare_blockers(state, player, actions, settings)
        }
        Decision::AssignCombatDamage { .. } => battle::assign_damage(state, player, actions),
        _ => heuristic(state, player, decision, actions, VERSION_1),
    }
}

/// What to do with priority: play a land, or whichever of passing and the
/// most promising spells and abilities does best in the imagined games.
fn priority(
    state: &DuelState,
    player: PlayerId,
    decision: Decision,
    actions: &[Action],
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    // It acts where a human player is asked to, and nowhere else.
    if should_auto_pass(state, player, false) {
        return Ok(Action::PassPriority);
    }
    if let Some(land) = best_land(state, player, actions)? {
        return Ok(Action::PlayLand { card: land });
    }
    let baseline = outcome(state, player, None, LOOKAHEAD)?;
    let ranked = ranked_by_glance(state, player, actions, Some(baseline))?;
    if ranked.is_empty() {
        return Ok(Action::PassPriority);
    }
    let preferred = heuristic(state, player, decision, actions, VERSION_1)?;
    let mut candidates = vec![Action::PassPriority];
    candidates.extend(pick_candidates(&ranked, settings));
    best_in_rollouts(state, player, &candidates, &preferred, settings)
}

/// The spells, abilities and target choices among `actions`, best first by
/// what version 1's glance ahead makes of them (less `baseline`, the same
/// glance at doing nothing, when there is one).
fn ranked_by_glance<'a>(
    state: &DuelState,
    player: PlayerId,
    actions: &'a [Action],
    baseline: Option<i32>,
) -> Result<Vec<(i32, &'a Action)>, InternalError> {
    let mut ranked = Vec::new();
    for action in distinct(state, actions)? {
        if !matches!(
            action,
            Action::CastSpell { .. }
                | Action::ActivateAbility { .. }
                | Action::ChooseTargets { .. }
        ) {
            continue;
        }
        let gain = outcome(state, player, Some(action), LOOKAHEAD)? - baseline.unwrap_or(0);
        ranked.push((gain, action));
    }
    // A stable sort: equals stay in the order the engine listed them.
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(ranked)
}

/// The first `candidates` of the ranked actions, with no more than
/// `targets_per_spell` ways of aiming the same spell or ability.
fn pick_candidates(ranked: &[(i32, &Action)], settings: &AiSearchBalance) -> Vec<Action> {
    // What is being cast or used, whatever it is aimed at.
    let spell = |action: &Action| match action {
        Action::CastSpell { card, .. } => Some((*card, None)),
        Action::ActivateAbility {
            source, ability, ..
        } => Some((*source, Some(*ability))),
        _ => None,
    };
    let mut picked: Vec<Action> = Vec::new();
    for (_, action) in ranked {
        if picked.len() >= settings.candidates as usize {
            break;
        }
        let same_spell = match spell(action) {
            Some(spell_cast) => picked
                .iter()
                .filter(|other| spell(other) == Some(spell_cast))
                .count(),
            None => 0,
        };
        if same_spell < settings.targets_per_spell as usize {
            picked.push((*action).clone());
        }
    }
    picked
}

/// Tries each candidate in the same imagined games and returns the one
/// that leaves `player` best off on average. `preferred` wins a tie.
fn best_in_rollouts(
    state: &DuelState,
    player: PlayerId,
    candidates: &[Action],
    preferred: &Action,
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    if let [only] = candidates {
        return Ok(only.clone());
    }
    let imagined = Imagined::new(state, player, settings)?;
    let mut totals = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        totals.push(imagined.total(player, settings, |game| {
            apply_trusted(game, player, candidate).map(|_| ())
        })?);
    }
    let mut best: Option<(&Action, i64)> = None;
    for (candidate, total) in candidates.iter().zip(totals) {
        let better = match best {
            None => true,
            Some((_, best_total)) => {
                total > best_total || (total == best_total && candidate == preferred)
            }
        };
        if better {
            best = Some((candidate, total));
        }
    }
    best.map(|(candidate, _)| candidate.clone())
        .ok_or_else(|| InternalError::new("the AI has nothing to choose from"))
}

/// The same position several times over, each with the cards `player`
/// can't see filled in by a different guess. Everything the AI weighs
/// against something else is tried in all of them.
pub(super) struct Imagined {
    worlds: Vec<DuelState>,
    until_turn: u32,
}

impl Imagined {
    pub(super) fn new(
        state: &DuelState,
        player: PlayerId,
        settings: &AiSearchBalance,
    ) -> Result<Imagined, InternalError> {
        let mut dice = DuelRng::from_seed(seed_from_what_is_seen(state, player)?);
        let pool = if settings.knows_decklist {
            None
        } else {
            Some(GuessPool::new(state, player.opponent())?)
        };
        let mut worlds = Vec::new();
        for _ in 0..settings.worlds {
            worlds.push(imagine_world(state, player, &mut dice, pool.as_ref())?);
        }
        Ok(Imagined {
            worlds,
            until_turn: state.turn.saturating_add(settings.turns),
        })
    }

    /// How well `player` does, added up over the worlds, by making `play`
    /// in each and letting version 1 play both sides on from there.
    pub(super) fn total(
        &self,
        player: PlayerId,
        settings: &AiSearchBalance,
        play: impl Fn(&mut DuelState) -> Result<(), InternalError>,
    ) -> Result<i64, InternalError> {
        let mut total = 0;
        for world in &self.worlds {
            let mut game = world.clone();
            play(&mut game)?;
            play_out(&mut game, self.until_turn, settings)?;
            total += i64::from(evaluate::bounded(&game, player, settings.win_value)?);
        }
        Ok(total)
    }
}

/// Plays an imagined game on with version 1 on both sides, until the turn
/// numbered `until_turn` begins, the game ends, or `max_actions` are taken.
fn play_out(
    game: &mut DuelState,
    until_turn: u32,
    settings: &AiSearchBalance,
) -> Result<(), InternalError> {
    for _ in 0..settings.max_actions {
        if game.result.is_some() || game.turn >= until_turn {
            break;
        }
        let Some(decision) = game.decision else {
            break;
        };
        let player = decision.player();
        let actions = legal_actions(game, player);
        let how = Thinking {
            lookahead: settings.rollout_lookahead,
            gang_blocks: settings.rollout_gang_blocks,
        };
        let action = heuristic(game, player, decision, &actions, how)?;
        apply_trusted(game, player, &action)?;
    }
    Ok(())
}

/// A seed for the AI's own dice, made from everything `player` can see and
/// nothing else: a hash of the player's view.
pub(super) fn seed_from_what_is_seen(
    state: &DuelState,
    player: PlayerId,
) -> Result<u64, InternalError> {
    let view = player_view(state, player)?;
    let bytes = serde_json::to_vec(&view).map_err(|error| InternalError::new(error.to_string()))?;
    Ok(fnv1a(&bytes))
}

/// The 64-bit FNV-1a hash, written out here so that it gives the same
/// number on every machine and with every version of the standard library.
pub(super) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A copy of the game with everything `player` can't see filled in by a
/// guess: the order of their own library, and which of the opponent's
/// unseen cards are in their hand and where the rest are in their library.
/// The copy gets dice of its own, so it knows nothing of the real duel's
/// future either.
///
/// With `pool`, the AI doesn't know the opponent's decklist, and each of
/// their unseen cards is a guess from the pool instead.
fn imagine_world(
    state: &DuelState,
    player: PlayerId,
    dice: &mut DuelRng,
    pool: Option<&GuessPool>,
) -> Result<DuelState, InternalError> {
    let mut world = state.clone();
    let opponent = player.opponent();
    // Dealing a library's cards out again among its own objects reorders it.
    let own_library = world.player(player).library.clone();
    world.redeal_cards(&own_library, dice)?;

    let mut unseen: Vec<ObjectId> = Vec::new();
    for &card in &world.player(opponent).hand {
        // A commander is known wherever it is.
        if !world.is_commander(card) {
            unseen.push(card);
        }
    }
    unseen.extend(&world.player(opponent).library);
    match pool {
        None => world.redeal_cards(&unseen, dice)?,
        Some(pool) => {
            for &card in &unseen {
                world.imagine_card(card, pool.draw(dice))?;
            }
        }
    }
    world.rng = DuelRng::from_seed(dice.next_seed());
    Ok(world)
}

/// What an opponent whose decklist is unknown might hold: the cards of the
/// colours they have shown, and basic lands for those colours.
struct GuessPool {
    spells: Vec<&'static CardDef>,
    lands: Vec<&'static CardDef>,
    /// The share of a deck that is land, in percent.
    land_percent: u32,
    any_card: &'static CardDef,
}

impl GuessPool {
    fn new(state: &DuelState, opponent: PlayerId) -> Result<GuessPool, InternalError> {
        let data = data::embedded().map_err(|error| InternalError::new(error.to_string()))?;
        let mut shown: BTreeSet<Color> = BTreeSet::new();
        for object in state.objects.values() {
            if object.owner != opponent || matches!(object.zone, Zone::Hand | Zone::Library) {
                continue;
            }
            let def = state.def(object.id)?;
            shown.extend(&def.colors);
            shown.extend(land_color(state, object.id)?);
        }
        let fits = |colors: &[Color]| shown.is_empty() || colors.iter().all(|c| shown.contains(c));
        let (mut spells, mut lands) = (Vec::new(), Vec::new());
        for def in data.cards.iter() {
            if def.token || def.is_legendary() {
                continue;
            }
            if !def.is_land() {
                if fits(&def.colors) {
                    spells.push(def);
                }
            } else if def.supertypes.contains(&Supertype::Basic) {
                let makes = def.abilities.iter().find_map(|ability| match ability {
                    crate::cards::AbilityDef::Mana { produces, .. } => produces.colored().next(),
                    _ => None,
                });
                if makes.is_none_or(|color| fits(&[color])) {
                    lands.push(def);
                }
            }
        }
        let (Some(&any_card), false) = (spells.first(), lands.is_empty()) else {
            return Err(InternalError::new(
                "no cards to guess an opponent's hand from",
            ));
        };
        Ok(GuessPool {
            spells,
            lands,
            land_percent: data.balance.run.lands.land_percent,
            any_card,
        })
    }

    fn draw(&self, dice: &mut DuelRng) -> &'static CardDef {
        let from = if dice.below(100) < self.land_percent {
            &self.lands
        } else {
            &self.spells
        };
        let index = dice.below(u32::try_from(from.len()).unwrap_or(u32::MAX)) as usize;
        // Neither list is empty and `below` stays inside it.
        from.get(index).copied().unwrap_or(self.any_card)
    }
}
