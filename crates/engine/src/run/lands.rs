//! Automatic basic lands. The player's deck is their spells and non-basic
//! lands; the basics are worked out from it every time it is used, so they
//! always fit the deck as it changes.

use std::collections::BTreeMap;

use crate::balance::LandBalance;
use crate::cards::{CardDefId, Color, Supertype};
use crate::data::CardDatabase;
use crate::duel::{CardInstance, InternalError};

const COLORS: [Color; 5] = [Color::W, Color::U, Color::B, Color::R, Color::G];

/// How many coloured mana symbols of each colour the non-land cards have.
pub fn color_weights(
    deck: &[CardInstance],
    cards: &CardDatabase,
) -> Result<BTreeMap<Color, u32>, InternalError> {
    let mut weights = BTreeMap::new();
    for instance in deck {
        let def = card(cards, &instance.card_id)?;
        if def.is_land() {
            continue;
        }
        for color in def.mana_cost.colored() {
            *weights.entry(color).or_insert(0) += 1;
        }
    }
    Ok(weights)
}

/// The colour the deck leans on most (the first in WUBRG order on a tie), or
/// the first fallback colour if its spells have no coloured symbols.
pub fn main_color(
    deck: &[CardInstance],
    fallback: &[Color],
    cards: &CardDatabase,
) -> Result<Option<Color>, InternalError> {
    let weights = color_weights(deck, cards)?;
    let mut best: Option<(Color, u32)> = None;
    for (&color, &weight) in &weights {
        if best.is_none_or(|(_, most)| weight > most) {
            best = Some((color, weight));
        }
    }
    Ok(best.map(|(color, _)| color).or(fallback.first().copied()))
}

/// The basic land that taps for `color`.
pub fn basic_land(color: Color, cards: &CardDatabase) -> Result<CardDefId, InternalError> {
    cards
        .iter()
        .find(|def| {
            def.is_land()
                && def.supertypes.contains(&Supertype::Basic)
                && def.mana_colors() == [color]
        })
        .map(|def| def.id.clone())
        .ok_or_else(|| InternalError::new(format!("no basic land for {color:?}")))
}

/// The basic lands that go with `deck` (spells and non-basic lands): how
/// many of each, in WUBRG order.
///
/// Lands are `land_percent` of the finished deck. They are split between
/// colours in proportion to the coloured mana symbols of the spells, and a
/// colour the deck uses at all gets at least `min_per_color`. A non-basic
/// land takes the place of a basic of one of its colours. `fallback` is the
/// colours to use when no spell has a coloured symbol.
pub fn basic_lands(
    deck: &[CardInstance],
    fallback: &[Color],
    cards: &CardDatabase,
    balance: &LandBalance,
) -> Result<Vec<(CardDefId, u32)>, InternalError> {
    let mut spells = 0u32;
    let mut nonbasic: Vec<Vec<Color>> = Vec::new();
    for instance in deck {
        let def = card(cards, &instance.card_id)?;
        if def.is_land() {
            nonbasic.push(def.mana_colors());
        } else {
            spells += 1;
        }
    }
    let percent = balance.land_percent.min(99);
    // lands / (lands + spells) = percent / 100, rounded to nearest.
    let rest = 100 - percent;
    let total = (spells * percent + rest / 2) / rest;

    let mut weights = color_weights(deck, cards)?;
    if weights.is_empty() {
        for &color in fallback {
            weights.insert(color, 1);
        }
    }
    let mut targets = apportion(total, &weights);

    // A colour in the deck gets a workable minimum, taken from the colour
    // with the most, if there are enough lands to go round.
    let colors_used = u32::try_from(weights.len()).unwrap_or(u32::MAX);
    if total >= balance.min_per_color.saturating_mul(colors_used) {
        for &color in weights.keys() {
            while targets.get(&color).copied().unwrap_or(0) < balance.min_per_color {
                let Some(richest) = richest(&targets, &COLORS, balance.min_per_color) else {
                    break;
                };
                *targets.entry(richest).or_insert(0) -= 1;
                *targets.entry(color).or_insert(0) += 1;
            }
        }
    }

    // Each non-basic land replaces a basic of one of its colours: the one
    // with the most basics, or failing that any colour's.
    for colors in &nonbasic {
        let replaced = richest(&targets, colors, 0).or_else(|| richest(&targets, &COLORS, 0));
        if let Some(color) = replaced {
            *targets.entry(color).or_insert(0) -= 1;
        }
    }

    let mut lands = Vec::new();
    for color in COLORS {
        let count = targets.get(&color).copied().unwrap_or(0);
        if count > 0 {
            lands.push((basic_land(color, cards)?, count));
        }
    }
    Ok(lands)
}

/// The colour among `among` with the most lands, if it has more than
/// `above`. Earlier colours win ties.
fn richest(targets: &BTreeMap<Color, u32>, among: &[Color], above: u32) -> Option<Color> {
    let mut best: Option<(Color, u32)> = None;
    for &color in among {
        let count = targets.get(&color).copied().unwrap_or(0);
        if count > above && best.is_none_or(|(_, most)| count > most) {
            best = Some((color, count));
        }
    }
    best.map(|(color, _)| color)
}

/// Splits `total` in proportion to the weights (largest remainder; the
/// heavier colour, then the earlier one, wins a tie).
fn apportion(total: u32, weights: &BTreeMap<Color, u32>) -> BTreeMap<Color, u32> {
    let sum: u32 = weights.values().sum();
    let mut shares = BTreeMap::new();
    if sum == 0 {
        return shares;
    }
    let mut remainders = Vec::new();
    let mut given = 0;
    for (&color, &weight) in weights {
        let share = total * weight / sum;
        shares.insert(color, share);
        given += share;
        remainders.push((total * weight % sum, weight, color));
    }
    remainders.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)).then(a.2.cmp(&b.2)));
    for (_, _, color) in remainders.into_iter().take((total - given) as usize) {
        *shares.entry(color).or_insert(0) += 1;
    }
    shares
}

fn card<'a>(
    cards: &'a CardDatabase,
    id: &CardDefId,
) -> Result<&'a crate::cards::CardDef, InternalError> {
    cards
        .get(id)
        .ok_or_else(|| InternalError::new(format!("no card definition `{id}`")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    fn deck(entries: &[(&str, u32)]) -> Vec<CardInstance> {
        entries
            .iter()
            .flat_map(|&(card, count)| {
                (0..count).map(move |_| CardInstance::new(CardDefId(card.to_string())))
            })
            .collect()
    }

    fn lands(entries: &[(&str, u32)]) -> Vec<(String, u32)> {
        let data = data::embedded().unwrap();
        basic_lands(
            &deck(entries),
            &[Color::G, Color::W],
            &data.cards,
            &data.balance.run.lands,
        )
        .unwrap()
        .into_iter()
        .map(|(id, count)| (id.0, count))
        .collect()
    }

    fn named(expected: &[(&str, u32)]) -> Vec<(String, u32)> {
        expected
            .iter()
            .map(|&(name, count)| (name.to_string(), count))
            .collect()
    }

    #[test]
    fn fifteen_spells_get_ten_lands_split_by_mana_symbols() {
        // Deck B's spells: 13 black symbols and 9 blue.
        let deck_b = [
            ("typhoid_rats", 3),
            ("wind_drake", 3),
            ("murder", 3),
            ("vampire_nighthawk", 2),
            ("cancel", 2),
            ("divination", 2),
        ];
        assert_eq!(lands(&deck_b), named(&[("island", 4), ("swamp", 6)]));
        // One colour takes them all.
        assert_eq!(lands(&[("shock", 15)]), named(&[("mountain", 10)]));
    }

    #[test]
    fn the_land_count_follows_the_deck_size() {
        let count = |spells: u32| -> u32 {
            lands(&[("grizzly_bears", spells)])
                .iter()
                .map(|(_, count)| count)
                .sum()
        };
        // 40% of the finished deck.
        assert_eq!(count(15), 10);
        assert_eq!(count(16), 11);
        assert_eq!(count(18), 12);
        assert_eq!(count(12), 8);
        assert_eq!(count(1), 1);
        assert_eq!(count(0), 0);
    }

    #[test]
    fn a_splash_gets_the_minimum_lands() {
        // 14 green symbols and 1 red: by proportion red would get 1 land.
        assert_eq!(
            lands(&[("grizzly_bears", 14), ("shock", 1)]),
            named(&[("mountain", 2), ("forest", 8)])
        );
        // With too few lands to give every colour the minimum, the split is
        // left as the proportions give it.
        assert_eq!(
            lands(&[("grizzly_bears", 2), ("shock", 1), ("savannah_lions", 1)]),
            named(&[("plains", 1), ("mountain", 1), ("forest", 1)])
        );
    }

    #[test]
    fn ties_are_broken_the_same_way_every_time() {
        // 8 white and 7 green symbols: 5.33 and 4.67 lands.
        assert_eq!(
            lands(&[("savannah_lions", 8), ("llanowar_elves", 7)]),
            named(&[("plains", 5), ("forest", 5)])
        );
        // An exact tie for the odd land goes to the earlier colour (WUBRG).
        assert_eq!(
            lands(&[("savannah_lions", 1), ("llanowar_elves", 1)]),
            named(&[("plains", 1)])
        );
    }

    #[test]
    fn a_dual_land_replaces_a_basic_of_one_of_its_colours() {
        let base = [("savannah_lions", 8), ("llanowar_elves", 7)];
        let with_gate = [
            ("savannah_lions", 8),
            ("llanowar_elves", 7),
            ("selesnya_guildgate", 1),
        ];
        assert_eq!(lands(&base), named(&[("plains", 5), ("forest", 5)]));
        // Still ten lands in all; the Gate stands in for a Forest (on a tie,
        // the first colour the land makes).
        assert_eq!(lands(&with_gate), named(&[("plains", 5), ("forest", 4)]));
        // An off-colour dual still counts as a land.
        let off_colour = [("savannah_lions", 15), ("rakdos_guildgate", 1)];
        assert_eq!(lands(&off_colour), named(&[("plains", 9)]));
    }

    #[test]
    fn a_deck_without_coloured_symbols_uses_the_fallback_colours() {
        assert_eq!(lands(&[]), named(&[]));
        let data = data::embedded().unwrap();
        assert_eq!(
            main_color(&[], &[Color::G, Color::W], &data.cards).unwrap(),
            Some(Color::G)
        );
        assert_eq!(
            main_color(
                &deck(&[("shock", 1), ("murder", 1)]),
                &[Color::G],
                &data.cards
            )
            .unwrap(),
            Some(Color::B)
        );
    }

    #[test]
    fn every_colour_has_a_basic_land() {
        let data = data::embedded().unwrap();
        let names: Vec<String> = COLORS
            .iter()
            .map(|&color| basic_land(color, &data.cards).unwrap().0)
            .collect();
        assert_eq!(names, ["plains", "island", "swamp", "mountain", "forest"]);
    }
}
