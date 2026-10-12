//! How the AI judges a position and a creature. These numbers are the AI's
//! taste, not game balance; they only decide which legal action it picks.

use crate::cards::{
    AbilityDef, Affects, CardDef, Keyword, Restricted, StaticAbility, Trigger, TriggerSubject,
};
use crate::duel::{DuelState, GameResult, InternalError, ObjectId, PlayerId};

/// What a card in hand is worth, in the same units as a creature's value.
const CARD_IN_HAND: i32 = 3;
/// What an enchantment that stays on the battlefield is worth for being
/// there, beyond what it does to creatures: a little more than the card.
const LASTING_PERMANENT: i32 = 4;
/// What a creature is worth for an ability that does something most turns:
/// about a point of power and one of toughness more than its body.
const ENGINE_ABILITY: i32 = 4;
/// And for one that needs something else to go right first.
const SOMETIMES_ABILITY: i32 = 2;
const WIN: i32 = 1_000_000;

/// How much a creature matters: roughly twice its power plus its toughness,
/// plus a little for each useful keyword and more for an ability that
/// keeps working (`ability_value`). Printed stats and lasting
/// bonuses (anthems, Auras) are used, but not what wears off at end of
/// turn, so a pump spell is judged by what it achieves, not by the bigger
/// numbers.
pub(super) fn creature_value(state: &DuelState, creature: ObjectId) -> Result<i32, InternalError> {
    let def = state.def(creature)?;
    let (power, toughness) = state.static_bonus(creature)?;
    let power = (def.power.unwrap_or(0) + power).max(0);
    let toughness = (def.toughness.unwrap_or(0) + toughness).max(0);
    let mut value = 1 + 2 * power + toughness;
    for keyword in &def.keywords {
        value += match keyword {
            // It hits twice.
            Keyword::DoubleStrike => 1 + 2 * power,
            Keyword::Flying | Keyword::Deathtouch => 2,
            Keyword::Indestructible => 3,
            // It never attacks.
            Keyword::Defender => -power,
            Keyword::Flash => 0,
            Keyword::FirstStrike
            | Keyword::Prowess
            | Keyword::Hexproof
            | Keyword::Lifelink
            | Keyword::Menace
            | Keyword::Reach
            | Keyword::Trample
            | Keyword::Vigilance => 1,
            Keyword::Haste => 0,
        };
    }
    if def
        .abilities
        .iter()
        .any(|ability| matches!(ability, AbilityDef::Mana { .. }))
    {
        value += 1;
    }
    // Something it can do every turn is worth having around.
    if def
        .abilities
        .iter()
        .any(|ability| matches!(ability, AbilityDef::Activated { .. }))
    {
        value += 2;
    }
    value += def.abilities.iter().map(ability_value).sum::<i32>();
    // A creature that can neither attack nor block is worth little.
    if state.is_restricted(creature, Restricted::Attack)?
        && state.is_restricted(creature, Restricted::Block)?
    {
        value /= 5;
    }
    Ok(value)
}

/// What an ability that keeps working adds to a creature on the
/// battlefield, beyond its body: the reason not to trade a 2/2 that puts a
/// counter on every creature its controller casts for a plain 3/3.
///
/// It is judged by how often the ability does something, not by what it
/// does: the effects are too varied to price, and how often is most of it.
fn ability_value(ability: &AbilityDef) -> i32 {
    match ability {
        AbilityDef::Triggered {
            trigger,
            once_each_turn,
            condition,
            ..
        } => {
            let value = match trigger {
                // It has entered, and what is left of this is nothing; nor
                // is dying something to keep it alive for.
                Trigger::EntersBattlefield {
                    subject: TriggerSubject::This,
                    ..
                }
                | Trigger::LeavesBattlefield {}
                | Trigger::Dies {
                    subject: TriggerSubject::This,
                } => 0,
                // Most turns, with no more help than a deck gives anyway:
                // every turn of its controller's, or every creature or
                // token of theirs that enters.
                Trigger::BeginningOfUpkeep {}
                | Trigger::BeginningOfCombat {}
                | Trigger::BeginningOfEndStep { .. }
                | Trigger::EntersBattlefield { .. } => ENGINE_ABILITY,
                // When something else goes right: it attacks or connects,
                // or its controller gains life, sacrifices, casts the right
                // spell or loses a creature.
                Trigger::Attacks {}
                | Trigger::DealsDamageToOpponent {}
                | Trigger::GainLife {}
                | Trigger::Sacrifice {}
                | Trigger::CastSpell { .. }
                | Trigger::Dies { .. } => SOMETIMES_ABILITY,
            };
            // Capped at once a turn, or waiting on a condition.
            let held_back = i32::from(*once_each_turn) + i32::from(condition.is_some());
            (value - held_back).max(0)
        }
        AbilityDef::Static(StaticAbility::ExtraCounters { .. }) => ENGINE_ABILITY,
        // What it gives the other creatures is counted on them, but only
        // while it lives.
        AbilityDef::Static(
            StaticAbility::Anthem { affects, .. } | StaticAbility::GrantKeyword { affects, .. },
        ) if *affects != Affects::This => SOMETIMES_ABILITY,
        // Mana and activated abilities are counted in `creature_value`.
        AbilityDef::Static(_)
        | AbilityDef::Spell { .. }
        | AbilityDef::Activated { .. }
        | AbilityDef::Mana { .. } => 0,
    }
}

/// What a life total is worth. Each point matters more the fewer are left.
fn life_value(life: i32) -> i32 {
    (1..=life.max(0))
        .map(|point| match point {
            ..=5 => 4,
            6..=10 => 3,
            11..=15 => 2,
            _ => 1,
        })
        .sum()
}

/// The position from `player`'s side: higher is better. It counts life,
/// creatures and cards in hand (the number of them, which is public).
pub(super) fn position(state: &DuelState, player: PlayerId) -> Result<i32, InternalError> {
    match state.result {
        Some(GameResult::Win { player: winner, .. }) if winner == player => return Ok(WIN),
        Some(GameResult::Win { .. }) => return Ok(-WIN),
        Some(GameResult::Draw) => return Ok(-WIN / 2),
        None => {}
    }
    let side = |who: PlayerId| -> Result<i32, InternalError> {
        let player_state = state.player(who);
        let mut total = life_value(player_state.life);
        total += CARD_IN_HAND * i32::try_from(player_state.hand.len()).unwrap_or(i32::MAX);
        for creature in state.creatures_of(who)? {
            total += creature_value(state, creature)?;
        }
        for permanent in state.permanents_of(who) {
            let def = state.def(permanent.id)?;
            if !def.is_creature() && !def.is_land() && !def.is_aura() {
                total += LASTING_PERMANENT;
            }
        }
        Ok(total)
    };
    Ok(side(player)? - side(player.opponent())?)
}

/// What a card in hand is worth to the searching AI. An instant or a
/// sorcery is worth more the more it costs, so that it is not spent on
/// something worth less than the card: Murder is not for a 1/1 token.
/// Everything else is worth what version 1 says. (A premium on permanents
/// too made a cheap creature look not worth casting.)
fn card_in_hand_value(def: &CardDef) -> i32 {
    if !def.is_instant_or_sorcery() {
        return CARD_IN_HAND;
    }
    let cost = i32::try_from(def.mana_cost.mana_value()).unwrap_or(i32::MAX);
    CARD_IN_HAND + (cost.saturating_add(1) / 2).min(4)
}

/// The position as the searching AI judges it at the end of an imagined
/// game. It is version 1's judgement with two differences: a card in hand
/// is worth what `card_in_hand_value` says, and a commander waiting in the
/// command zone is worth a third of what it would be on the battlefield,
/// since it can be cast again.
fn searched_position(state: &DuelState, player: PlayerId) -> Result<i32, InternalError> {
    let side = |who: PlayerId| -> Result<i32, InternalError> {
        let player_state = state.player(who);
        let mut total = life_value(player_state.life);
        for &card in &player_state.hand {
            total += card_in_hand_value(state.def(card)?);
        }
        for &card in &player_state.command {
            let def = state.def(card)?;
            total +=
                (1 + 2 * def.power.unwrap_or(0).max(0) + def.toughness.unwrap_or(0).max(0)) / 3;
        }
        for creature in state.creatures_of(who)? {
            total += creature_value(state, creature)?;
        }
        for permanent in state.permanents_of(who) {
            let def = state.def(permanent.id)?;
            if !def.is_creature() && !def.is_land() && !def.is_aura() {
                total += LASTING_PERMANENT;
            }
        }
        Ok(total)
    };
    Ok(side(player)? - side(player.opponent())?)
}

/// The position from `player`'s side: version 1's judgement, or with
/// `card_values` the searching AI's. A game that is over is worth the
/// same huge number either way.
pub(super) fn judged(
    state: &DuelState,
    player: PlayerId,
    card_values: bool,
) -> Result<i32, InternalError> {
    if card_values && state.result.is_none() {
        searched_position(state, player)
    } else {
        position(state, player)
    }
}

/// The position from `player`'s side on a scale that ends at `win_value`:
/// a won game is worth that, a lost one its opposite, and a game still
/// going is kept just inside the two. Scores of this kind can be averaged
/// over imagined games, where one unlikely win must not outweigh the rest.
pub(super) fn bounded(
    state: &DuelState,
    player: PlayerId,
    win_value: i32,
) -> Result<i32, InternalError> {
    Ok(match state.result {
        Some(GameResult::Win { player: winner, .. }) if winner == player => win_value,
        Some(GameResult::Win { .. }) => -win_value,
        Some(GameResult::Draw) => -win_value / 2,
        None => searched_position(state, player)?.clamp(1 - win_value, win_value - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn life_is_worth_more_when_there_is_less_of_it() {
        assert_eq!(life_value(0), 0);
        assert_eq!(life_value(-3), 0);
        assert_eq!(life_value(5), 20);
        assert_eq!(life_value(20), 20 + 15 + 10 + 5);
        // The last two points of 20 are worth 2; the last two of 5 are worth 8.
        assert_eq!(life_value(20) - life_value(18), 2);
        assert_eq!(life_value(5) - life_value(3), 8);
    }
}
