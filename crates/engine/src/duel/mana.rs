//! The mana pool and the auto-payer.
//!
//! Players don't tap for mana by hand yet. Casting a spell is legal only if
//! the auto-payer finds a way to pay; it then taps the sources, which puts
//! mana in the pool, and the cost is paid from the pool. Manual tapping can
//! later add to the same pool.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::GameEvent;
use super::error::InternalError;
use super::ids::{ObjectId, PlayerId};
use super::state::DuelState;
use crate::cards::{AbilityDef, Color, Keyword, ManaCost};

/// Unspent mana. It empties at the end of every step.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct ManaPool(BTreeMap<Color, u32>);

impl ManaPool {
    pub fn amount(&self, color: Color) -> u32 {
        self.0.get(&color).copied().unwrap_or(0)
    }

    pub fn total(&self) -> u32 {
        self.0.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn add(&mut self, color: Color) {
        *self.0.entry(color).or_insert(0) += 1;
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    fn remove(&mut self, color: Color, amount: u32) {
        let left = self.amount(color).saturating_sub(amount);
        if left == 0 {
            self.0.remove(&color);
        } else {
            self.0.insert(color, left);
        }
    }

    /// Pays `cost` from the pool: coloured symbols with their colour, then
    /// generic with whatever is left. Returns false, changing nothing, if
    /// the pool can't cover it.
    pub(crate) fn pay(&mut self, cost: &ManaCost) -> bool {
        let mut after = self.clone();
        for color in cost.colored() {
            if after.amount(color) == 0 {
                return false;
            }
            after.remove(color, 1);
        }
        let mut generic = cost.generic();
        if after.total() < generic {
            return false;
        }
        for color in [Color::W, Color::U, Color::B, Color::R, Color::G] {
            let spent = generic.min(after.amount(color));
            after.remove(color, spent);
            generic -= spent;
        }
        *self = after;
        true
    }
}

/// An untapped permanent that can tap for one mana right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Source {
    id: ObjectId,
    color: Color,
    is_creature: bool,
}

/// The player's usable mana sources: lands first, then creatures, each in
/// the order they entered the battlefield.
fn sources(state: &DuelState, player: PlayerId) -> Result<Vec<Source>, InternalError> {
    let mut sources = Vec::new();
    for permanent in state.permanents_of(player) {
        if permanent.tapped {
            continue;
        }
        let def = state.def(permanent.id)?;
        // Tapping a creature needs it to be past summoning sickness.
        if def.is_creature() && permanent.summoning_sick && !def.has_keyword(Keyword::Haste) {
            continue;
        }
        let color = def.abilities.iter().find_map(|ability| match ability {
            AbilityDef::Mana { cost, produces } if cost.tap => produces.colored().next(),
            _ => None,
        });
        if let Some(color) = color {
            sources.push(Source {
                id: permanent.id,
                color,
                is_creature: def.is_creature(),
            });
        }
    }
    sources.sort_by_key(|source| source.is_creature);
    Ok(sources)
}

/// Chooses which sources to tap to pay `cost`, or `None` if the player
/// can't pay. `casting` is the card being cast, if it is in hand.
///
/// The choice is deterministic. Lands are tapped before creatures. Generic
/// mana comes from the colour the player can best spare: the one with the
/// most untapped sources left over after what the other cards in hand need.
pub(crate) fn plan_payment(
    state: &DuelState,
    player: PlayerId,
    cost: &ManaCost,
    casting: Option<ObjectId>,
) -> Result<Option<Vec<ObjectId>>, InternalError> {
    let mut available = sources(state, player)?;
    let mut chosen = Vec::new();
    for color in cost.colored() {
        let Some(position) = available.iter().position(|source| source.color == color) else {
            return Ok(None);
        };
        chosen.push(available.remove(position).id);
    }

    let generic = cost.generic() as usize;
    if generic > available.len() {
        return Ok(None);
    }
    if generic > 0 {
        let mut needed: BTreeMap<Color, i64> = BTreeMap::new();
        for &card in &state.player(player).hand {
            if Some(card) == casting {
                continue;
            }
            for color in state.def(card)?.mana_cost.colored() {
                *needed.entry(color).or_insert(0) += 1;
            }
        }
        for _ in 0..generic {
            let spare = |color: Color| {
                let untapped = available.iter().filter(|s| s.color == color).count();
                untapped as i64 - needed.get(&color).copied().unwrap_or(0)
            };
            let lands_left = available.iter().any(|source| !source.is_creature);
            let mut best: Option<(usize, i64)> = None;
            for (position, source) in available.iter().enumerate() {
                if lands_left && source.is_creature {
                    continue;
                }
                let spare = spare(source.color);
                if best.is_none_or(|(_, best_spare)| spare > best_spare) {
                    best = Some((position, spare));
                }
            }
            let Some((position, _)) = best else {
                return Ok(None);
            };
            chosen.push(available.remove(position).id);
        }
    }
    Ok(Some(chosen))
}

/// Taps the planned sources for mana and pays `cost` from the pool.
pub(crate) fn pay_cost(
    state: &mut DuelState,
    events: &mut Vec<GameEvent>,
    player: PlayerId,
    cost: &ManaCost,
    plan: &[ObjectId],
) -> Result<(), InternalError> {
    let usable = sources(state, player)?;
    for &id in plan {
        let source = usable
            .iter()
            .find(|source| source.id == id)
            .ok_or_else(|| InternalError::new(format!("{id:?} can't be tapped for mana")))?;
        state.object_mut(id)?.tapped = true;
        events.push(GameEvent::PermanentTapped { permanent: id });
        state.player_mut(player).mana_pool.add(source.color);
    }
    if state.player_mut(player).mana_pool.pay(cost) {
        Ok(())
    } else {
        Err(InternalError::new(format!(
            "the planned payment does not cover {cost}"
        )))
    }
}

pub(crate) fn empty_mana_pools(state: &mut DuelState) {
    for player in &mut state.players {
        player.mana_pool.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(colors: &[Color]) -> ManaPool {
        let mut pool = ManaPool::default();
        for &color in colors {
            pool.add(color);
        }
        pool
    }

    fn cost(text: &str) -> ManaCost {
        text.parse().unwrap()
    }

    #[test]
    fn pool_counts_mana_by_colour() {
        let pool = pool(&[Color::G, Color::G, Color::W]);
        assert_eq!(pool.amount(Color::G), 2);
        assert_eq!(pool.amount(Color::W), 1);
        assert_eq!(pool.amount(Color::U), 0);
        assert_eq!(pool.total(), 3);
        assert!(ManaPool::default().is_empty());
    }

    #[test]
    fn paying_uses_coloured_mana_for_coloured_symbols_and_anything_for_generic() {
        let mut exact = pool(&[Color::G, Color::W]);
        assert!(exact.pay(&cost("{1}{G}")));
        assert!(exact.is_empty());

        let mut spare = pool(&[Color::U, Color::U, Color::B, Color::R]);
        assert!(spare.pay(&cost("{1}{U}{U}")));
        assert_eq!(spare, pool(&[Color::R]));

        let mut free = pool(&[Color::R]);
        assert!(free.pay(&cost("")));
        assert_eq!(free, pool(&[Color::R]));
    }

    #[test]
    fn a_pool_that_cannot_pay_is_left_alone() {
        let mut wrong_colour = pool(&[Color::R, Color::R]);
        assert!(!wrong_colour.pay(&cost("{1}{G}")));
        assert_eq!(wrong_colour, pool(&[Color::R, Color::R]));

        let mut too_little = pool(&[Color::G]);
        assert!(!too_little.pay(&cost("{1}{G}")));
        assert_eq!(too_little, pool(&[Color::G]));
    }

    #[test]
    fn pool_serialises_as_a_map_of_colours() {
        let json = serde_json::to_string(&pool(&[Color::G, Color::G, Color::W])).unwrap();
        assert_eq!(json, r#"{"W":1,"G":2}"#);
        assert_eq!(
            serde_json::from_str::<ManaPool>(&json).unwrap(),
            pool(&[Color::G, Color::G, Color::W])
        );
    }
}
