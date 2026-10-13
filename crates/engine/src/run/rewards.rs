//! What a run offers: card rewards, shop stock and relics.

use crate::balance::{RarityWeights, RunBalance};
use crate::cards::{CardDef, CardDefId, Color, Rarity};
use crate::data::{CardDatabase, GameData};
use crate::duel::DuelRng;

use super::defs::ModifierId;
use super::state::{ShopItem, Ware};

const RARITIES: [Rarity; 4] = [
    Rarity::Common,
    Rarity::Uncommon,
    Rarity::Rare,
    Rarity::Mythic,
];

fn weight(weights: &RarityWeights, rarity: Rarity) -> u32 {
    match rarity {
        Rarity::Common => weights.common,
        Rarity::Uncommon => weights.uncommon,
        Rarity::Rare => weights.rare,
        Rarity::Mythic => weights.mythic,
    }
}

fn bound(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Picks one of `candidates`: first a rarity, by the weights of the
/// rarities on offer, then a card of that rarity. If no rarity on offer has
/// any weight, every candidate is equally likely.
fn pick_card(
    rng: &mut DuelRng,
    candidates: &[&CardDef],
    weights: &RarityWeights,
) -> Option<CardDefId> {
    if candidates.is_empty() {
        return None;
    }
    let on_offer = |rarity: Rarity| candidates.iter().any(|card| card.rarity == rarity);
    let total: u32 = RARITIES
        .into_iter()
        .filter(|&rarity| on_offer(rarity))
        .map(|rarity| weight(weights, rarity))
        .sum();
    if total == 0 {
        let pick = rng.below(bound(candidates.len())) as usize;
        return candidates.get(pick).map(|card| card.id.clone());
    }
    let mut roll = rng.below(total);
    for rarity in RARITIES.into_iter().filter(|&rarity| on_offer(rarity)) {
        if roll < weight(weights, rarity) {
            let of_rarity: Vec<&&CardDef> = candidates
                .iter()
                .filter(|card| card.rarity == rarity)
                .collect();
            let pick = rng.below(bound(of_rarity.len())) as usize;
            return of_rarity.get(pick).map(|card| card.id.clone());
        }
        roll -= weight(weights, rarity);
    }
    None
}

/// Whether every colour of the card is one of `colors`.
fn in_colors(card: &CardDef, colors: &[Color]) -> bool {
    !card.colors.is_empty() && card.colors.iter().all(|color| colors.contains(color))
}

fn is_nonbasic_land(card: &CardDef) -> bool {
    card.is_land() && card.supertypes.is_empty()
}

/// The non-basic lands to choose from: those that only make the player's
/// colours if there are any, otherwise all of them.
fn land_pool<'a>(cards: &'a CardDatabase, colors: &[Color]) -> Vec<&'a CardDef> {
    let all: Vec<&CardDef> = cards.iter().filter(|card| is_nonbasic_land(card)).collect();
    let fitting: Vec<&CardDef> = all
        .iter()
        .copied()
        .filter(|card| {
            card.mana_colors()
                .iter()
                .all(|color| colors.contains(color))
        })
        .collect();
    if fitting.is_empty() { all } else { fitting }
}

/// Whether a reward, a draft, a shop or an event may show the card at all:
/// a spell that is not a token, not legendary (a legendary card is a
/// commander, never something in a deck) and not held back.
pub fn is_offered(card: &CardDef) -> bool {
    !card.is_land() && !card.token && !card.is_legendary() && !card.reserved
}

/// Every spell a character of these colours can be offered, in id order.
pub fn offered_spells<'a>(cards: &'a CardDatabase, colors: &[Color]) -> Vec<&'a CardDef> {
    cards
        .iter()
        .filter(|card| is_offered(card) && in_colors(card, colors))
        .collect()
}

/// `count` different spells: the first `own` from the player's colours, the
/// rest from any colour.
fn spell_offer(
    rng: &mut DuelRng,
    cards: &CardDatabase,
    colors: &[Color],
    weights: &RarityWeights,
    count: u8,
    own: u8,
) -> Vec<CardDefId> {
    let mut chosen: Vec<CardDefId> = Vec::new();
    for slot in 0..count {
        let candidates: Vec<&CardDef> = cards
            .iter()
            .filter(|card| is_offered(card))
            .filter(|card| !chosen.contains(&card.id))
            .filter(|card| slot >= own || in_colors(card, colors))
            .collect();
        if let Some(card) = pick_card(rng, &candidates, weights) {
            chosen.push(card);
        }
    }
    chosen
}

/// The `count` cards offered after a duel. Now and then the last of them is
/// a non-basic land instead of a spell.
pub(crate) fn card_reward(
    rng: &mut DuelRng,
    data: &GameData,
    colors: &[Color],
    weights: &RarityWeights,
    count: u8,
) -> Vec<CardDefId> {
    let rewards = &data.balance.run.rewards;
    let mut choices = spell_offer(
        rng,
        &data.cards,
        colors,
        weights,
        count,
        rewards.own_color_choices,
    );
    let lands = land_pool(&data.cards, colors);
    if rng.below(100) < rewards.land_percent && !lands.is_empty() {
        let land = lands[rng.below(bound(lands.len())) as usize].id.clone();
        match choices.last_mut() {
            Some(last) => *last = land,
            None => choices.push(land),
        }
    }
    choices
}

/// The pack a draft node opens: different spells in the player's colours.
pub(crate) fn draft_pack(rng: &mut DuelRng, data: &GameData, colors: &[Color]) -> Vec<CardDefId> {
    let draft = &data.balance.run.draft;
    spell_offer(
        rng,
        &data.cards,
        colors,
        &draft.weights,
        draft.cards,
        draft.cards,
    )
}

/// The cards an event shows: different spells in the player's colours.
pub(crate) fn event_cards(
    rng: &mut DuelRng,
    data: &GameData,
    colors: &[Color],
    count: u8,
) -> Vec<CardDefId> {
    let weights = &data.balance.run.event.card_weights;
    spell_offer(rng, &data.cards, colors, weights, count, count)
}

/// The cards a card of the deck could be reforged into: the spells of the
/// next rarity up in the player's colours. Empty for a land, and for a
/// card of the highest rarity that has any.
pub(crate) fn transmutations<'a>(
    data: &'a GameData,
    colors: &[Color],
    card: &CardDefId,
) -> Vec<&'a CardDef> {
    let Some(from) = data.cards.get(card).filter(|def| !def.is_land()) else {
        return Vec::new();
    };
    let Some(next) = RARITIES
        .iter()
        .position(|rarity| *rarity == from.rarity)
        .and_then(|index| RARITIES.get(index + 1))
    else {
        return Vec::new();
    };
    data.cards
        .iter()
        .filter(|def| is_offered(def))
        .filter(|def| def.rarity == *next && in_colors(def, colors))
        .collect()
}

/// One of `candidates`, each as likely as the next.
pub(crate) fn pick_one(rng: &mut DuelRng, candidates: &[&CardDef]) -> Option<CardDefId> {
    if candidates.is_empty() {
        return None;
    }
    let pick = rng.below(bound(candidates.len())) as usize;
    candidates.get(pick).map(|card| card.id.clone())
}

/// A relic the player doesn't have yet, if there is one.
pub(crate) fn random_relic(
    rng: &mut DuelRng,
    data: &GameData,
    owned: &[ModifierId],
) -> Option<ModifierId> {
    let available: Vec<&ModifierId> = data
        .run
        .relics
        .iter()
        .filter(|relic| !relic.reserved)
        .map(|relic| &relic.id)
        .filter(|id| !owned.contains(id))
        .collect();
    if available.is_empty() {
        return None;
    }
    let pick = rng.below(bound(available.len())) as usize;
    available.get(pick).map(|&id| id.clone())
}

/// The relics a treasure node offers: different ones the player doesn't
/// have, as many as the balance asks for or as there are left.
pub(crate) fn treasure_relics(
    rng: &mut DuelRng,
    data: &GameData,
    owned: &[ModifierId],
) -> Vec<ModifierId> {
    relic_offer(rng, data, owned, data.balance.run.treasure.relics)
}

/// Up to `count` different relics the player doesn't have.
pub(crate) fn relic_offer(
    rng: &mut DuelRng,
    data: &GameData,
    owned: &[ModifierId],
    count: u8,
) -> Vec<ModifierId> {
    let mut taken = owned.to_vec();
    let mut offered = Vec::new();
    for _ in 0..count {
        let Some(relic) = random_relic(rng, data, &taken) else {
            break;
        };
        taken.push(relic.clone());
        offered.push(relic);
    }
    offered
}

/// What a shop asks for a ware before any discount.
pub(crate) fn list_price(data: &GameData, ware: &Ware) -> u32 {
    let prices = &data.balance.run.shop.prices;
    match ware {
        Ware::Relic { .. } => prices.relic,
        Ware::Card { card } => match data.cards.get(card) {
            Some(def) if def.is_land() => prices.land,
            Some(def) => match def.rarity {
                Rarity::Common => prices.common,
                Rarity::Uncommon => prices.uncommon,
                Rarity::Rare => prices.rare,
                Rarity::Mythic => prices.mythic,
            },
            None => prices.common,
        },
    }
}

/// A price with `percent` taken off, rounded down.
pub(crate) fn discounted(price: u32, percent: u32) -> u32 {
    let keep = u64::from(100 - percent.min(100));
    u32::try_from(u64::from(price) * keep / 100).unwrap_or(price)
}

/// What a shop has for sale, at its list prices: spells, then non-basic
/// lands, then relics.
pub(crate) fn shop_stock(
    rng: &mut DuelRng,
    data: &GameData,
    colors: &[Color],
    owned_relics: &[ModifierId],
) -> Vec<ShopItem> {
    let balance: &RunBalance = &data.balance.run;
    let shop = &balance.shop;
    let mut wares = Vec::new();
    let own = balance.rewards.own_color_choices.min(shop.cards);
    for card in spell_offer(
        rng,
        &data.cards,
        colors,
        &shop.card_weights,
        shop.cards,
        own,
    ) {
        wares.push(Ware::Card { card });
    }

    let mut lands = land_pool(&data.cards, colors);
    for _ in 0..shop.lands {
        if lands.is_empty() {
            break;
        }
        let land = lands.remove(rng.below(bound(lands.len())) as usize);
        wares.push(Ware::Card {
            card: land.id.clone(),
        });
    }

    let mut relics = owned_relics.to_vec();
    for _ in 0..shop.relics {
        let Some(relic) = random_relic(rng, data, &relics) else {
            break;
        };
        relics.push(relic.clone());
        wares.push(Ware::Relic { relic });
    }
    wares
        .into_iter()
        .map(|ware| ShopItem {
            price: list_price(data, &ware),
            ware,
            sold: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::data;

    const GW: [Color; 2] = [Color::G, Color::W];

    fn card(id: &CardDefId) -> &'static CardDef {
        data::embedded().unwrap().cards.get(id).unwrap()
    }

    #[test]
    fn a_reward_is_three_different_cards_in_the_players_colours() {
        let data = data::embedded().unwrap();
        let weights = &data.balance.run.rewards.duel;
        let mut lands = 0;
        for seed in 0..400 {
            let mut rng = DuelRng::from_seed(seed);
            let choices = card_reward(&mut rng, data, &GW, weights, 3);
            assert_eq!(choices.len(), 3);
            assert!(choices[0] != choices[1] && choices[1] != choices[2]);
            assert_ne!(choices[0], choices[2]);
            for choice in &choices[..2] {
                assert!(in_colors(card(choice), &GW), "{choice}");
            }
            let last = card(&choices[2]);
            if last.is_land() {
                assert_eq!(last.id.as_str(), "selesnya_guildgate");
                lands += 1;
            } else {
                assert!(in_colors(last, &GW), "{}", last.id);
            }
            assert!(choices.iter().all(|id| card(id).supertypes.is_empty()));
        }
        // 15% of rewards have a land.
        assert!((30..=90).contains(&lands), "{lands} lands in 400 rewards");
    }

    #[test]
    fn slots_past_the_own_colour_ones_offer_any_colour() {
        let data = data::embedded().unwrap();
        let weights = &data.balance.run.rewards.duel;
        let mut off_colour = 0;
        for seed in 0..400 {
            let mut rng = DuelRng::from_seed(seed);
            let offer = spell_offer(&mut rng, &data.cards, &GW, weights, 3, 2);
            assert!(offer[..2].iter().all(|id| in_colors(card(id), &GW)));
            off_colour += u32::from(!in_colors(card(&offer[2]), &GW));
        }
        assert!(off_colour > 100, "{off_colour} off-colour cards");
    }

    #[test]
    fn rarity_weights_decide_how_often_each_rarity_is_offered() {
        let data = data::embedded().unwrap();
        let count = |weights: &RarityWeights| {
            let mut seen: BTreeMap<Rarity, u32> = BTreeMap::new();
            for seed in 0..500 {
                let mut rng = DuelRng::from_seed(seed);
                for id in spell_offer(&mut rng, &data.cards, &GW, weights, 3, 2) {
                    *seen.entry(card(&id).rarity).or_insert(0) += 1;
                }
            }
            seen
        };
        let duel = count(&data.balance.run.rewards.duel);
        let elite = count(&data.balance.run.rewards.elite);
        let share = |seen: &BTreeMap<Rarity, u32>, rarity| {
            seen.get(&rarity).copied().unwrap_or(0) * 100 / 1500
        };
        assert!(share(&duel, Rarity::Common) > 60, "{duel:?}");
        assert!(share(&elite, Rarity::Common) < 55, "{elite:?}");
        assert!(share(&elite, Rarity::Uncommon) > share(&duel, Rarity::Uncommon));
        assert!(share(&elite, Rarity::Rare) > share(&duel, Rarity::Rare));

        // A single rarity with weight: nothing else is offered while cards
        // of that rarity last.
        let only_uncommon = RarityWeights {
            common: 0,
            uncommon: 1,
            rare: 0,
            mythic: 0,
        };
        let mut rng = DuelRng::from_seed(1);
        for id in spell_offer(&mut rng, &data.cards, &GW, &only_uncommon, 2, 2) {
            assert_eq!(card(&id).rarity, Rarity::Uncommon);
        }
    }

    #[test]
    fn a_shop_stocks_cards_a_land_and_a_relic_at_the_listed_prices() {
        let data = data::embedded().unwrap();
        let prices = &data.balance.run.shop.prices;
        for seed in 0..100 {
            let mut rng = DuelRng::from_seed(seed);
            let owned = vec![data.run.relics[0].id.clone()];
            let items = shop_stock(&mut rng, data, &GW, &owned);
            assert_eq!(items.len(), 5);
            for item in &items[..3] {
                let Ware::Card { card: id } = &item.ware else {
                    panic!("expected a card, got {item:?}");
                };
                let expected = match card(id).rarity {
                    Rarity::Common => prices.common,
                    Rarity::Uncommon => prices.uncommon,
                    Rarity::Rare => prices.rare,
                    Rarity::Mythic => prices.mythic,
                };
                assert!(!card(id).is_land());
                assert_eq!(item.price, expected);
            }
            assert_eq!(
                items[3].ware,
                Ware::Card {
                    card: CardDefId("selesnya_guildgate".to_string())
                }
            );
            assert_eq!(items[3].price, prices.land);
            let Ware::Relic { relic } = &items[4].ware else {
                panic!("expected a relic, got {:?}", items[4]);
            };
            assert!(!owned.contains(relic));
            assert_eq!(items[4].price, prices.relic);
            assert!(items.iter().all(|item| !item.sold));
        }
    }

    #[test]
    fn a_shop_has_no_relic_for_a_player_who_owns_them_all() {
        let data = data::embedded().unwrap();
        let owned: Vec<ModifierId> = data.run.relics.iter().map(|r| r.id.clone()).collect();
        let mut rng = DuelRng::from_seed(1);
        assert_eq!(random_relic(&mut rng, data, &owned), None);
        let items = shop_stock(&mut rng, data, &GW, &owned);
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn a_treasure_offers_two_different_relics_or_what_is_left() {
        let data = data::embedded().unwrap();
        let all: Vec<ModifierId> = data.run.relics.iter().map(|r| r.id.clone()).collect();
        assert_eq!(data.balance.run.treasure.relics, 2);
        for seed in 0..100 {
            let mut rng = DuelRng::from_seed(seed);
            let owned = vec![all[0].clone(), all[3].clone()];
            let offered = treasure_relics(&mut rng, data, &owned);
            assert_eq!(offered.len(), 2);
            assert_ne!(offered[0], offered[1]);
            assert!(offered.iter().all(|relic| !owned.contains(relic)));
        }
        let mut rng = DuelRng::from_seed(1);
        assert_eq!(
            treasure_relics(&mut rng, data, &all[1..]),
            std::slice::from_ref(&all[0])
        );
        assert_eq!(treasure_relics(&mut rng, data, &all), []);
    }

    #[test]
    fn a_discount_takes_a_percentage_off_and_rounds_down() {
        assert_eq!(discounted(40, 20), 32);
        assert_eq!(discounted(110, 20), 88);
        assert_eq!(discounted(45, 20), 36);
        assert_eq!(discounted(1, 20), 0);
        assert_eq!(discounted(120, 0), 120);
        assert_eq!(discounted(120, 100), 0);
        assert_eq!(discounted(120, 250), 0);
    }
}
