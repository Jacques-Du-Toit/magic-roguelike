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

use std::cell::Cell;
use std::collections::BTreeSet;

use super::battle;
use super::combat;
use super::distinct::distinct;
use super::{
    Considered, Held, SEARCH_GLANCE, Thinking, VERSION_1, best_land, evaluate, heuristic,
    land_color, outcome,
};
use crate::balance::AiSearchBalance;
use crate::cards::{CardDef, CardType, Color, Keyword, Supertype};
use crate::data;
use crate::duel::{
    Action, Decision, DuelRng, DuelState, InternalError, ObjectId, PlayerId, Step, Zone,
    apply_trusted, legal_actions, player_view, should_auto_pass,
};

/// Whether `player`, with priority, loses nothing by doing later what they
/// could do now: later in this turn they will be asked again, and what
/// they can do now they can do then. That is so in combat and in the
/// opponent's turn, but not in the opponent's end step, which is the last
/// stop before their own turn, nor in their own main phases, where the
/// next stop may come too late for a sorcery or a creature.
fn waiting_costs_nothing(state: &DuelState, player: PlayerId) -> bool {
    let own_turn = state.active_player == player;
    match state.step {
        Step::DeclareAttackers | Step::DeclareBlockers => true,
        Step::PrecombatMain | Step::PostcombatMain | Step::BeginCombat => !own_turn,
        Step::Untap
        | Step::Upkeep
        | Step::Draw
        | Step::FirstStrikeDamage
        | Step::CombatDamage
        | Step::EndCombat
        | Step::End
        | Step::Cleanup => false,
    }
}

/// What the AI weighed in its last decision, for `ai_explain`. Nothing is
/// kept unless an explanation was asked for.
#[derive(Debug, Default)]
pub(super) struct Trace {
    pub(super) considered: Vec<Considered>,
    pub(super) note: Option<&'static str>,
}

thread_local! {
    static TRACE: Cell<Option<Trace>> = const { Cell::new(None) };
}

fn traced(change: impl FnOnce(&mut Trace)) {
    TRACE.with(|cell| {
        if let Some(mut trace) = cell.take() {
            change(&mut trace);
            cell.set(Some(trace));
        }
    });
}

/// Records why the decision was made without weighing anything, or how a
/// tie was settled.
pub(super) fn note(note: &'static str) {
    traced(|trace| trace.note = Some(note));
}

/// Records something that was tried in the imagined games, and how it did
/// added up over them.
pub(super) fn consider(play: Vec<Action>, holding: Option<Held>, total: i64) {
    traced(|trace| {
        trace.considered.push(Considered {
            play,
            holding,
            total,
        });
    });
}

/// Runs a decision and returns it with what was weighed on the way.
pub(super) fn with_trace<T>(decide: impl FnOnce() -> T) -> (T, Trace) {
    TRACE.with(|cell| cell.set(Some(Trace::default())));
    let decided = decide();
    let trace = TRACE.with(Cell::take).unwrap_or_default();
    (decided, trace)
}

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
            let preferred = heuristic(state, player, decision, actions, SEARCH_GLANCE)?;
            let ranked = ranked_by_glance(state, player, actions, None)?;
            let candidates: Vec<Candidate> = pick_candidates(&ranked, settings)
                .into_iter()
                .map(Candidate::doing)
                .collect();
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
        note("it passes here without thinking: a human player isn't asked at this point either");
        return Ok(Action::PassPriority);
    }
    if let Some(land) = best_land(state, player, actions)? {
        note("it plays a land whenever it can, before anything else");
        return Ok(Action::PlayLand { card: land });
    }
    let baseline = outcome(state, player, None, SEARCH_GLANCE)?;
    let ranked = ranked_by_glance(state, player, actions, Some(baseline))?;
    if ranked.is_empty() {
        note("it has nothing to cast or use");
        return Ok(Action::PassPriority);
    }
    let preferred = heuristic(state, player, decision, actions, SEARCH_GLANCE)?;
    let mut candidates = vec![Candidate::doing(Action::PassPriority)];
    if could_hold_mana(state, player)? {
        candidates.push(Candidate {
            action: Action::PassPriority,
            holding: Some(Hold::OpponentsTurn),
        });
    }
    for attack in attacks_to_hold_for(state, player, actions, settings)? {
        candidates.push(Candidate {
            action: Action::PassPriority,
            holding: Some(Hold::Combat(attack)),
        });
    }
    candidates.extend(
        pick_candidates(&ranked, settings)
            .into_iter()
            .map(Candidate::doing),
    );
    best_in_rollouts(state, player, &candidates, &preferred, settings)
}

/// What a player who passes in their own main phase is keeping mana up
/// for, as the imagined games play it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Hold {
    /// The opponent's turn: the player casts nothing more in the main
    /// phases of this turn of theirs, so that what they hold (a
    /// counterspell, an instant, a creature with flash) can still be paid
    /// for.
    OpponentsTurn,
    /// This turn's combat: the player casts nothing before blocks are
    /// declared and attacks with these creatures, so that a pump spell or
    /// a removal spell is cast knowing the blocks. After that they play as
    /// they like.
    Combat(Vec<ObjectId>),
}

/// Something the AI might do, as the imagined games try it.
struct Candidate {
    action: Action,
    /// Passing with the intention of keeping mana up. Plain passing doesn't
    /// say that: the imagined player would pass now and cast its creature a
    /// moment later.
    holding: Option<Hold>,
}

impl Candidate {
    fn doing(action: Action) -> Candidate {
        Candidate {
            action,
            holding: None,
        }
    }

    /// The candidate as an explanation lists it.
    fn considered(&self) -> (Vec<Action>, Option<Held>) {
        let mut play = vec![self.action.clone()];
        let held = match &self.holding {
            None => None,
            Some(Hold::OpponentsTurn) => Some(Held::ForOpponentsTurn),
            Some(Hold::Combat(attack)) => {
                play.extend(attack.iter().map(|&creature| Action::Attack { creature }));
                play.push(Action::ConfirmAttackers);
                Some(Held::ForCombat)
            }
        };
        (play, held)
    }
}

/// The attacks worth imagining with mana kept for after blocks: none unless
/// it is the player's first main phase with nothing on the stack and they
/// could cast an instant or a card with flash now. Otherwise version 1's
/// attack, that attack with one more creature (the most valuable first),
/// and everything, `hold_attacks` of them at most. Version 1 attacks only
/// where no blocker beats the attacker, which is just where a trick in
/// hand changes the answer.
fn attacks_to_hold_for(
    state: &DuelState,
    player: PlayerId,
    actions: &[Action],
    settings: &AiSearchBalance,
) -> Result<Vec<Vec<ObjectId>>, InternalError> {
    let budget = settings.hold_attacks as usize;
    if budget == 0
        || state.active_player != player
        || state.step != Step::PrecombatMain
        || !state.stack.is_empty()
    {
        return Ok(Vec::new());
    }
    let mut has_trick = false;
    for action in actions {
        if let Action::CastSpell { card, .. } = action {
            let def = state.def(*card)?;
            has_trick |= def.types.contains(&CardType::Instant) || def.has_keyword(Keyword::Flash);
        }
    }
    if !has_trick {
        return Ok(Vec::new());
    }

    // What could attack, were everyone to pass from here. Passing shows
    // nothing hidden.
    let mut ahead = state.clone();
    let mut able: Vec<ObjectId> = Vec::new();
    for _ in 0..8 {
        match ahead.decision {
            Some(Decision::Priority { player: passing })
                if ahead.turn == state.turn
                    && matches!(ahead.step, Step::PrecombatMain | Step::BeginCombat) =>
            {
                apply_trusted(&mut ahead, passing, &Action::PassPriority)?;
            }
            Some(Decision::DeclareAttackers { player: attacking }) if attacking == player => {
                able.extend(legal_actions(&ahead, player).iter().filter_map(
                    |action| match action {
                        Action::Attack { creature } => Some(*creature),
                        _ => None,
                    },
                ));
                break;
            }
            _ => break,
        }
    }
    if able.is_empty() {
        return Ok(Vec::new());
    }
    able.sort_unstable();

    let preferred = combat::attack_plan(&ahead, player, &able)?;
    let mut attacks: Vec<Vec<ObjectId>> = Vec::new();
    if !preferred.is_empty() {
        attacks.push(preferred.clone());
    }
    // One of each kind of creature version 1 leaves at home.
    let classes = super::distinct::classes(&ahead, &able)?;
    let mut others: Vec<(i32, ObjectId)> = Vec::new();
    let mut kinds: Vec<usize> = Vec::new();
    for (&creature, &class) in able.iter().zip(&classes) {
        if !preferred.contains(&creature) && !kinds.contains(&class) {
            kinds.push(class);
            others.push((evaluate::creature_value(&ahead, creature)?, creature));
        }
    }
    // A stable sort: equals stay in the order of their ids.
    others.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, creature) in others {
        let mut attack = preferred.clone();
        attack.push(creature);
        attack.sort_unstable();
        attacks.push(attack);
    }
    if !attacks.contains(&able) {
        attacks.push(able);
    }
    attacks.truncate(budget);
    Ok(attacks)
}

/// Whether keeping mana up is worth weighing: it is the player's own main
/// phase with nothing on the stack, and they hold something they could
/// cast in the opponent's turn.
fn could_hold_mana(state: &DuelState, player: PlayerId) -> Result<bool, InternalError> {
    if state.active_player != player || !state.step.is_main() || !state.stack.is_empty() {
        return Ok(false);
    }
    for &card in &state.player(player).hand {
        let def = state.def(card)?;
        if def.types.contains(&CardType::Instant) || def.has_keyword(Keyword::Flash) {
            return Ok(true);
        }
    }
    Ok(false)
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
        let gain = outcome(state, player, Some(action), SEARCH_GLANCE)? - baseline.unwrap_or(0);
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
    candidates: &[Candidate],
    preferred: &Action,
    settings: &AiSearchBalance,
) -> Result<Action, InternalError> {
    if let [only] = candidates {
        return Ok(only.action.clone());
    }
    let imagined = Imagined::new(state, player, settings)?;
    let mut totals = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let holding = candidate.holding.as_ref();
        totals.push(imagined.total(player, settings, holding, |game| {
            apply_trusted(game, player, &candidate.action).map(|_| ())
        })?);
    }
    for (candidate, &total) in candidates.iter().zip(&totals) {
        let (play, held) = candidate.considered();
        consider(play, held, total);
    }
    // Something that can be done as well later is done later: an instant
    // held back shows the opponent nothing until they have committed.
    if waiting_costs_nothing(state, player) {
        let best_total = totals.iter().copied().max();
        let certain_win = i64::from(settings.win_value) * i64::from(settings.worlds);
        let passing = candidates.iter().zip(&totals).find(|(candidate, _)| {
            candidate.action == Action::PassPriority && candidate.holding.is_none()
        });
        if let Some((_, &pass_total)) = passing
            && Some(pass_total) == best_total
            && pass_total < certain_win
        {
            note(
                "passing does as well as anything in the imagined games, and it can still act \
                 later this turn, so it waits",
            );
            return Ok(Action::PassPriority);
        }
    }
    let top = totals.iter().copied().max();
    let mut best: Option<(&Candidate, i64)> = None;
    for (candidate, &total) in candidates.iter().zip(&totals) {
        let better = match best {
            None => true,
            Some((_, best_total)) => {
                total > best_total
                    || (total == best_total
                        && candidate.holding.is_none()
                        && candidate.action == *preferred)
            }
        };
        if better {
            best = Some((candidate, total));
        }
    }
    let Some((best, best_total)) = best else {
        return Err(InternalError::new("the AI has nothing to choose from"));
    };
    // A creature with flash is kept for the opponent's turn when keeping
    // mana up does as well as casting it now: it comes down in their turn
    // instead, having shown them nothing. Instants are left as they are:
    // a removal spell that scores the same later is still better cast now.
    let keeping = candidates
        .iter()
        .zip(&totals)
        .find(|(candidate, _)| candidate.holding == Some(Hold::OpponentsTurn));
    if let Some((_, &keeping_total)) = keeping
        && keeping_total == best_total
        && keeping_total < i64::from(settings.win_value) * i64::from(settings.worlds)
        && let Action::CastSpell { card, .. } = &best.action
    {
        let def = state.def(*card)?;
        if def.has_keyword(Keyword::Flash) && !def.types.contains(&CardType::Instant) {
            note(
                "keeping mana up for the opponent's turn does as well in the imagined games as \
                 casting its flash card now, so it keeps its mana up",
            );
            return Ok(Action::PassPriority);
        }
    }
    if totals.iter().filter(|total| Some(**total) == top).count() > 1 {
        note(
            "the best plays came out level in the imagined games, so it does what its quick \
             look ahead prefers, or else the first of them",
        );
    }
    Ok(best.action.clone())
}

/// The same position several times over, each with the cards `player`
/// can't see filled in by a different guess. Everything the AI weighs
/// against something else is tried in all of them.
pub(super) struct Imagined {
    worlds: Vec<DuelState>,
    turn: u32,
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
            turn: state.turn,
            until_turn: state.turn.saturating_add(settings.turns),
        })
    }

    /// How well `player` does, added up over the worlds, by making `play`
    /// in each and letting version 1 play both sides on from there. With
    /// `holding`, the player keeps mana up as it says in the turn the
    /// imagining began in (see `Hold`).
    pub(super) fn total(
        &self,
        player: PlayerId,
        settings: &AiSearchBalance,
        holding: Option<&Hold>,
        play: impl Fn(&mut DuelState) -> Result<(), InternalError>,
    ) -> Result<i64, InternalError> {
        let mut total = 0;
        for world in &self.worlds {
            let mut game = world.clone();
            play(&mut game)?;
            let holding = holding.map(|hold| (player, self.turn, hold));
            play_out(&mut game, self.until_turn, holding, settings)?;
            total += i64::from(evaluate::bounded(&game, player, settings.win_value)?);
        }
        Ok(total)
    }
}

/// Plays an imagined game on with version 1 on both sides, until the turn
/// numbered `until_turn` begins, the game ends, or `max_actions` are taken.
///
/// `holding` names a player who is keeping mana up in the turn with that
/// number, and what for: until then they only pass, and one keeping it for
/// combat makes the attack that goes with it.
fn play_out(
    game: &mut DuelState,
    until_turn: u32,
    holding: Option<(PlayerId, u32, &Hold)>,
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
        if let Some((holder, turn, hold)) = holding
            && holder == player
            && turn == game.turn
            && game.active_player == player
            && keep_holding(game, player, decision, hold)?
        {
            continue;
        }
        let how = Thinking {
            lookahead: settings.rollout_lookahead,
            gang_blocks: settings.rollout_gang_blocks,
            card_values: true,
        };
        // Like the searching AI itself, the imagined players act only
        // where a human would be asked. Version 1 proper thinks again at
        // every step of every turn, and in imagined games that was most
        // of the work.
        if should_auto_pass(game, player, false) {
            apply_trusted(game, player, &Action::PassPriority)?;
            continue;
        }
        if declare_at_once(game, player, decision, how)? {
            continue;
        }
        let actions = legal_actions(game, player);
        let action = heuristic(game, player, decision, &actions, how)?;
        apply_trusted(game, player, &action)?;
    }
    Ok(())
}

/// In an imagined game, answers `decision` for a player keeping mana up in
/// their own turn, where keeping it settles the answer. Returns whether it
/// did.
fn keep_holding(
    game: &mut DuelState,
    player: PlayerId,
    decision: Decision,
    hold: &Hold,
) -> Result<bool, InternalError> {
    match (hold, decision) {
        (_, Decision::Priority { .. }) if game.stack.is_empty() => {
            let waits = match hold {
                Hold::OpponentsTurn => game.step.is_main(),
                Hold::Combat(_) => matches!(
                    game.step,
                    Step::PrecombatMain | Step::BeginCombat | Step::DeclareAttackers
                ),
            };
            if waits {
                apply_trusted(game, player, &Action::PassPriority)?;
            }
            Ok(waits)
        }
        (Hold::Combat(attack), Decision::DeclareAttackers { .. })
            if combat::declared_attackers(game).is_empty() =>
        {
            // In some imagined game a creature may no longer be able to.
            let able = legal_actions(game, player);
            for &creature in attack {
                let attacking = Action::Attack { creature };
                if able.contains(&attacking) {
                    apply_trusted(game, player, &attacking)?;
                }
            }
            apply_trusted(game, player, &Action::ConfirmAttackers)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// In an imagined game, declares version 1's whole attack or all its
/// blocks in one go. Version 1 itself declares a creature at a time and
/// works its plan out again at every step; the plan depends only on the
/// board, so this comes to the same declaration for a fraction of the
/// work on a crowded battlefield. Returns whether it declared anything.
fn declare_at_once(
    game: &mut DuelState,
    player: PlayerId,
    decision: Decision,
    how: Thinking,
) -> Result<bool, InternalError> {
    let has_declared = |game: &DuelState, blocks: bool| {
        game.combat.as_ref().is_some_and(|combat| {
            if blocks {
                combat.attackers.iter().any(|a| !a.blockers.is_empty())
            } else {
                !combat.attackers.is_empty()
            }
        })
    };
    match decision {
        Decision::DeclareAttackers { .. } if !has_declared(game, false) => {
            let mut able: Vec<ObjectId> = legal_actions(game, player)
                .iter()
                .filter_map(|action| match action {
                    Action::Attack { creature } => Some(*creature),
                    _ => None,
                })
                .collect();
            able.sort_unstable();
            for creature in combat::attack_plan(game, player, &able)? {
                apply_trusted(game, player, &Action::Attack { creature })?;
            }
            apply_trusted(game, player, &Action::ConfirmAttackers)?;
            Ok(true)
        }
        Decision::DeclareBlockers { .. } if !has_declared(game, true) => {
            let plan = combat::block_plan(game, player, how.gang_blocks)?;
            let mut declared = game.clone();
            for &(blocker, attacker) in &plan {
                apply_trusted(&mut declared, player, &Action::Block { blocker, attacker })?;
            }
            // Blocks that can't be confirmed are left to the step-by-step
            // way, which takes them back.
            if !legal_actions(&declared, player).contains(&Action::ConfirmBlockers) {
                return Ok(false);
            }
            apply_trusted(&mut declared, player, &Action::ConfirmBlockers)?;
            *game = declared;
            Ok(true)
        }
        _ => Ok(false),
    }
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
