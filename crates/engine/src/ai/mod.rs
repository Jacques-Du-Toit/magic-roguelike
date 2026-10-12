//! The opponent's decision-making. See `docs/ARCHITECTURE.md` §7.
//!
//! Version 1 is a set of heuristics. It only ever returns one of the
//! player's legal actions, and it looks only at what that player may see:
//! their own hand and the public zones, never the opponent's hand or either
//! library.
//!
//! - Mulligans: keep a hand with a sensible number of lands.
//! - Lands: play one whenever possible.
//! - Spells and activated abilities: try each one on a copy of the game, let the stack
//!   (and any combat damage still to come) play out with everyone passing,
//!   and cast the one that leaves the best position, if any improves on
//!   doing nothing.
//! - Attacks: attack with creatures that no blocker beats, or with
//!   everything when that is lethal.
//! - Blocks: block where it kills the attacker or costs nothing, and chump
//!   only to survive.
//! - Targets of its triggered abilities: whichever leaves the best position.
//! - Scry: bottom a land it has no use for or a spell it can't cast soon.

mod battle;
mod combat;
mod distinct;
mod evaluate;
mod search;

use crate::cards::{Color, ManaCost};
use crate::data;
use crate::duel::{
    Action, ActionError, Decision, DuelState, InternalError, ObjectId, PlayerId, Step,
    apply_trusted, legal_actions, scry_card,
};

/// How many choices of targets, one after another, version 1 thinks through
/// when it imagines a spell resolving.
const LOOKAHEAD: u8 = 2;

/// How the heuristics think. Version 1 always thinks one way; the players
/// in the searching AI's imagined games think a little differently.
#[derive(Debug, Clone, Copy)]
struct Thinking {
    /// How many choices of targets, one after another, are thought through
    /// when a spell is imagined resolving.
    lookahead: u8,
    /// Whether two creatures block one attacker to kill it.
    gang_blocks: bool,
    /// Whether a card in hand is worth more the more it costs (and a
    /// commander in the command zone something), as the searching AI
    /// judges, so that a spell isn't spent on something worth less than
    /// the card. Version 1 counts every card in hand the same.
    card_values: bool,
}

const VERSION_1: Thinking = Thinking {
    lookahead: LOOKAHEAD,
    gang_blocks: false,
    card_values: false,
};

/// Version 1 with the searching AI's idea of what a card is worth: what
/// the search glances ahead with, and what it does when the imagined games
/// can't separate two plays.
const SEARCH_GLANCE: Thinking = Thinking {
    lookahead: LOOKAHEAD,
    gang_blocks: false,
    card_values: true,
};

/// Which of the engine's AIs makes a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiKind {
    /// Version 1: the heuristics described at the top of this file.
    Heuristic,
    /// Version 2: the search in `search.rs`, set up as `ai.search` in
    /// `balance.json` says.
    Search,
    /// The search, knowing the opponent's decklist whatever the balance
    /// file says. This and the next are for measuring that setting.
    SearchKnowing,
    /// The search, not knowing the opponent's decklist.
    SearchGuessing,
}

/// What the searching AI keeps mana up for when it passes in its own main
/// phase holding an instant or a card with flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// The opponent's turn: it casts nothing more in its main phases.
    ForOpponentsTurn,
    /// This turn's combat: it casts nothing until blocks are declared.
    ForCombat,
}

/// Something the searching AI tried in its imagined games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Considered {
    /// What it would do: one action, or a whole attack or set of blocks
    /// ending in its confirmation.
    pub play: Vec<Action>,
    /// What it would be keeping mana up for, when the play is to pass and
    /// keep it. Kept for combat, the play goes on to say which attack it
    /// imagined making.
    pub holding: Option<Held>,
    /// How it did, added up over the imagined games: higher is better for
    /// the AI. Divide by `Explanation::worlds` for the average, on a scale
    /// where a creature is worth about 5 to 15 and winning `win_value`.
    pub total: i64,
}

/// Why the AI did what it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    pub chosen: Action,
    /// Everything it weighed, in the order it tried them. Empty when the
    /// decision took no weighing.
    pub considered: Vec<Considered>,
    /// How many imagined games each of those was tried in.
    pub worlds: u32,
    /// In words: why nothing was weighed, or how a tie was settled.
    pub note: Option<&'static str>,
}

/// Chooses an action as `ai_choose_action_with` does, and says what was
/// weighed on the way. The choice is the same one.
pub fn ai_explain(
    state: &DuelState,
    player: PlayerId,
    kind: AiKind,
) -> Result<Explanation, ActionError> {
    let (chosen, trace) = search::with_trace(|| ai_choose_action_with(state, player, kind));
    let worlds = data::embedded()
        .map_err(|error| InternalError::new(error.to_string()))?
        .balance
        .ai
        .search
        .worlds;
    Ok(Explanation {
        chosen: chosen?,
        considered: trace.considered,
        worlds,
        note: trace.note,
    })
}

/// Chooses an action for `player`, who the game must be waiting on, with
/// the AI asked for.
pub fn ai_choose_action_with(
    state: &DuelState,
    player: PlayerId,
    kind: AiKind,
) -> Result<Action, ActionError> {
    let Some(decision) = state
        .decision
        .filter(|decision| decision.player() == player)
    else {
        return Err(ActionError::NotWaitingOn(player));
    };
    let actions = legal_actions(state, player);
    let choice = match kind {
        AiKind::Heuristic => heuristic(state, player, decision, &actions, VERSION_1)?,
        AiKind::Search | AiKind::SearchKnowing | AiKind::SearchGuessing => {
            let mut settings = data::embedded()
                .map_err(|error| InternalError::new(error.to_string()))?
                .balance
                .ai
                .search
                .clone();
            match kind {
                AiKind::SearchKnowing => settings.knows_decklist = true,
                AiKind::SearchGuessing => settings.knows_decklist = false,
                AiKind::Heuristic | AiKind::Search => {}
            }
            search::choose(state, player, decision, &actions, &settings)?
        }
    };
    if actions.contains(&choice) {
        Ok(choice)
    } else {
        Err(InternalError::new(format!("the AI chose the illegal action {choice:?}")).into())
    }
}

/// Chooses an action for `player`, who the game must be waiting on, the
/// way version 1 does.
pub fn ai_choose_action(state: &DuelState, player: PlayerId) -> Result<Action, ActionError> {
    ai_choose_action_with(state, player, AiKind::Heuristic)
}

/// Version 1's answer to `decision`, which is `player`'s to make, from
/// their legal `actions`, thinking as `how` says.
fn heuristic(
    state: &DuelState,
    player: PlayerId,
    decision: Decision,
    actions: &[Action],
    how: Thinking,
) -> Result<Action, InternalError> {
    Ok(match decision {
        Decision::Mulligan { .. } => mulligan(state, player, actions)?,
        Decision::BottomCards { .. } => Action::BottomCard {
            card: least_wanted_card(state, player)?,
        },
        Decision::Discard { .. } => Action::Discard {
            card: least_wanted_card(state, player)?,
        },
        Decision::Priority { .. } => priority(state, player, actions, how)?,
        Decision::DeclareAttackers { .. } => combat::declare_attackers(state, player, actions)?,
        Decision::DeclareBlockers { .. } => {
            combat::declare_blockers(state, player, actions, how.gang_blocks)?
        }
        // The division on offer is the automatic one, which kills as many
        // blockers as the damage allows.
        Decision::AssignCombatDamage { .. } => Action::ConfirmDamage,
        Decision::ChooseTargets { .. } => best_choice(state, player, actions, how)?,
        Decision::Scry { kept, surveil, .. } => scry(state, player, kept, surveil)?,
    })
}
fn lands_in_hand(state: &DuelState, player: PlayerId) -> Result<Vec<ObjectId>, InternalError> {
    let mut lands = Vec::new();
    for &card in &state.player(player).hand {
        if state.def(card)?.is_land() {
            lands.push(card);
        }
    }
    Ok(lands)
}

/// Keep a hand whose land count is in the configured range, and never go
/// below the configured hand size.
fn mulligan(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
) -> Result<Action, InternalError> {
    let rule = &data::embedded()
        .map_err(|error| InternalError::new(error.to_string()))?
        .balance
        .ai
        .mulligan;
    let player_state = state.player(player);
    let lands = lands_in_hand(state, player)?.len();
    let lands_ok = (usize::from(rule.min_lands)..=usize::from(rule.max_lands)).contains(&lands);
    let hand_after_mulligan = player_state
        .opening_hand_size
        .saturating_sub(player_state.mulligans_taken.saturating_add(1));
    let may_mulligan =
        actions.contains(&Action::Mulligan) && hand_after_mulligan >= rule.min_hand_size;
    Ok(if !lands_ok && may_mulligan {
        Action::Mulligan
    } else {
        Action::Keep
    })
}

/// The card in hand the player can best do without: a land when the hand
/// has more than it needs, otherwise the most expensive spell.
fn least_wanted_card(state: &DuelState, player: PlayerId) -> Result<ObjectId, InternalError> {
    let rule = &data::embedded()
        .map_err(|error| InternalError::new(error.to_string()))?
        .balance
        .ai
        .mulligan;
    let enough_lands = usize::from(rule.min_lands.midpoint(rule.max_lands));
    let lands = lands_in_hand(state, player)?;

    let mut priciest: Option<(ObjectId, u32)> = None;
    for &card in &state.player(player).hand {
        let def = state.def(card)?;
        let cost = def.mana_cost.mana_value();
        if !def.is_land() && priciest.is_none_or(|(_, highest)| cost > highest) {
            priciest = Some((card, cost));
        }
    }
    match (lands.last(), priciest) {
        (Some(&land), _) if lands.len() > enough_lands => Ok(land),
        (_, Some((spell, _))) => Ok(spell),
        (Some(&land), None) => Ok(land),
        (None, None) => Err(InternalError::new("no card in hand to give up")),
    }
}

/// What to do with priority: play a land, cast the best spell, or pass.
fn priority(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    how: Thinking,
) -> Result<Action, InternalError> {
    if let Some(land) = best_land(state, player, actions)? {
        return Ok(Action::PlayLand { card: land });
    }
    // With nothing to cast or use there is nothing to weigh.
    if !actions.iter().any(|action| {
        matches!(
            action,
            Action::CastSpell { .. } | Action::ActivateAbility { .. }
        )
    }) {
        return Ok(Action::PassPriority);
    }

    let baseline = outcome(state, player, None, how)?;
    let mut best: Option<(&Action, i32)> = None;
    for action in distinct::distinct(state, actions)? {
        if !matches!(
            action,
            Action::CastSpell { .. } | Action::ActivateAbility { .. }
        ) {
            continue;
        }
        let gain = outcome(state, player, Some(action), how)? - baseline;
        if gain > 0 && best.is_none_or(|(_, best_gain)| gain > best_gain) {
            best = Some((action, gain));
        }
    }
    Ok(best.map_or(Action::PassPriority, |(action, _)| action.clone()))
}

/// The action that leaves `player` best off, among actions of which one
/// must be taken: the first of the best.
fn best_choice(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    how: Thinking,
) -> Result<Action, InternalError> {
    let mut best: Option<(&Action, i32)> = None;
    for action in distinct::distinct(state, actions)? {
        let score = outcome(state, player, Some(action), how)?;
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((action, score));
        }
    }
    best.map(|(action, _)| action.clone())
        .ok_or_else(|| InternalError::new("the AI has nothing to choose from"))
}

/// Keep the card on top unless it is a land with plenty of lands already,
/// or a spell that is a long way from castable.
fn scry(
    state: &DuelState,
    player: PlayerId,
    kept: u8,
    surveil: bool,
) -> Result<Action, InternalError> {
    let def = state.def(scry_card(state, player, kept)?)?;
    let mut lands = lands_in_hand(state, player)?.len();
    for permanent in state.permanents_of(player) {
        if state.def(permanent.id)?.is_land() {
            lands += 1;
        }
    }
    let unwanted = if def.is_land() {
        lands >= 6
    } else {
        def.mana_cost.mana_value() as usize > lands + 3
    };
    Ok(match (unwanted, surveil) {
        (false, _) => Action::ScryKeep,
        (true, false) => Action::ScryBottom,
        (true, true) => Action::ScryGraveyard,
    })
}

/// How good the position is for `player` once things settle, after taking
/// `action` now (or after doing nothing).
fn outcome(
    state: &DuelState,
    player: PlayerId,
    action: Option<&Action>,
    how: Thinking,
) -> Result<i32, InternalError> {
    let mut copy = state.clone();
    let settled = (|| {
        if let Some(action) = action {
            apply_trusted(&mut copy, player, action)?;
        }
        settle(&mut copy, how)
    })();
    match settled {
        Ok(()) => evaluate::judged(&copy, player, how.card_values),
        Err(ActionError::Internal(error)) => Err(error),
        Err(error) => Err(InternalError::new(error.to_string())),
    }
}

/// Plays a copy of the game forward with both players passing, until the
/// stack is empty and no combat damage is still to come this combat, or
/// until someone has a real decision to make.
///
/// Passing needs no hidden information, so this stays within what the
/// player may know. A triggered ability that needs targets on the way gets
/// the ones that suit its controller best, `lookahead` times at most.
fn settle(state: &mut DuelState, how: Thinking) -> Result<(), ActionError> {
    let mut how = how;
    // Far more passes than any stack and combat can need.
    for _ in 0..200 {
        let player = match state.decision {
            Some(Decision::Priority { player }) => player,
            Some(Decision::ChooseTargets { player, .. }) if how.lookahead > 0 => {
                how.lookahead -= 1;
                let actions = legal_actions(state, player);
                let choice = best_choice(state, player, &actions, how)?;
                apply_trusted(state, player, &choice)?;
                continue;
            }
            _ => return Ok(()),
        };
        let attackers = state
            .combat
            .as_ref()
            .is_some_and(|combat| !combat.attackers.is_empty());
        let damage_to_come = attackers
            && matches!(
                state.step,
                Step::DeclareAttackers | Step::DeclareBlockers | Step::FirstStrikeDamage
            );
        if state.stack.is_empty() && !damage_to_come {
            return Ok(());
        }
        apply_trusted(state, player, &Action::PassPriority)?;
    }
    Ok(())
}

/// The land to play, if a land can be played: the one whose colour the
/// hand needs most and the battlefield has least of.
fn best_land(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
) -> Result<Option<ObjectId>, InternalError> {
    let mut best: Option<(ObjectId, i64)> = None;
    for action in actions {
        let Action::PlayLand { card } = action else {
            continue;
        };
        let score = match land_color(state, *card)? {
            None => i64::MIN,
            Some(color) => {
                let mut needed = 0i64;
                for &other in &state.player(player).hand {
                    let cost: &ManaCost = &state.def(other)?.mana_cost;
                    needed += cost.colored().filter(|c| *c == color).count() as i64;
                }
                let mut have = 0i64;
                for permanent in state.permanents_of(player) {
                    if land_color(state, permanent.id)? == Some(color) {
                        have += 1;
                    }
                }
                if needed > 0 {
                    100 + needed - 10 * have
                } else {
                    -have
                }
            }
        };
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((*card, score));
        }
    }
    Ok(best.map(|(card, _)| card))
}

/// The colour of mana a land taps for.
fn land_color(state: &DuelState, card: ObjectId) -> Result<Option<Color>, InternalError> {
    let def = state.def(card)?;
    if !def.is_land() {
        return Ok(None);
    }
    Ok(def.abilities.iter().find_map(|ability| match ability {
        crate::cards::AbilityDef::Mana { produces, .. } => produces.colored().next(),
        _ => None,
    }))
}

#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod tests;
