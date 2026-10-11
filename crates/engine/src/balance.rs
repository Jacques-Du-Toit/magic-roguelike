//! Tunable numbers, loaded from `data/balance.json`. Nothing in the engine
//! hard-codes a balance value (CLAUDE.md rule 5).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::run::{EnemyTier, ModifierDef, ModifierKind, RunDataError, validate_modifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Balance {
    pub duel: DuelBalance,
    pub ai: AiBalance,
    pub sim: SimBalance,
    pub run: RunBalance,
}

/// Limits and thresholds for the `sim` CLI's report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimBalance {
    /// A game still going after this many turns (both players' turns
    /// counted) is stopped and reported.
    pub turn_cap: u32,
    pub land_screw: LandScrewBalance,
    pub land_flood: LandFloodBalance,
}

/// A player is "stuck on lands" if they control `max_lands` or fewer at the
/// end of their own turn number `turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandScrewBalance {
    pub turn: u32,
    pub max_lands: u32,
}

/// A player is "flooded" if, at the end of their own turn number `turn`,
/// lands are at least `min_land_percent` of the cards they have drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandFloodBalance {
    pub turn: u32,
    pub min_land_percent: u32,
}

/// Numbers the heuristic AI plays by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiBalance {
    pub mulligan: AiMulliganBalance,
}

/// The AI keeps a hand with `min_lands..=max_lands` lands, and never
/// mulligans to fewer than `min_hand_size` cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiMulliganBalance {
    pub min_lands: u8,
    pub max_lands: u8,
    pub min_hand_size: u8,
}

/// Defaults for a duel. A `DuelConfig` starts from these and may differ per
/// player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuelBalance {
    pub starting_life: i32,
    pub opening_hand_size: u8,
    pub max_hand_size: u8,
    /// How many candidate opening hands are dealt; 1 turns smoothing off.
    pub smoothing_candidates: u8,
    pub fatigue: FatigueBalance,
}

/// Life lost for drawing from an empty library: `first_life_loss`, growing
/// by `increase` each time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FatigueBalance {
    pub first_life_loss: u32,
    pub increase: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BalanceError {
    #[error("`duel.starting_life` must be above zero")]
    StartingLife,
    #[error("`duel.smoothing_candidates` must be at least 1")]
    SmoothingCandidates,
    #[error("`ai.mulligan.min_lands` must not be above `max_lands`")]
    AiMulliganLands,
    #[error("`sim.turn_cap` must be above zero")]
    TurnCap,
    #[error("`run.hearts` must be above zero")]
    Hearts,
    #[error("`run.lands.land_percent` must be between 1 and 90")]
    LandPercent,
    #[error("`run.map` needs at least one column and one row")]
    MapShape,
    #[error("`run.map.rows[{0}]` gives no node kind a weight")]
    MapRowWeights(usize),
    #[error("`run.rewards` needs at least one choice, and no more own-colour choices than choices")]
    RewardChoices,
    #[error("`run.rewards.{0}` gives no rarity a weight")]
    RarityWeights(&'static str),
    #[error("`run.boons`: {0}")]
    Boon(#[from] RunDataError),
}

pub fn validate(balance: &Balance) -> Result<(), BalanceError> {
    if balance.duel.starting_life <= 0 {
        return Err(BalanceError::StartingLife);
    }
    if balance.duel.smoothing_candidates == 0 {
        return Err(BalanceError::SmoothingCandidates);
    }
    if balance.ai.mulligan.min_lands > balance.ai.mulligan.max_lands {
        return Err(BalanceError::AiMulliganLands);
    }
    if balance.sim.turn_cap == 0 {
        return Err(BalanceError::TurnCap);
    }
    validate_run(&balance.run)
}

/// Every number of the run layer: hearts, gold, prices, rewards, the map's
/// shape and the land share. The boons are here too, because a boon is
/// nothing but its numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBalance {
    /// Hearts at the start of an act.
    pub hearts: u8,
    pub starting_gold: u32,
    /// A card can't be removed from a deck with this many spells and
    /// non-basic lands or fewer.
    pub min_deck_cards: u32,
    pub lands: LandBalance,
    pub map: MapBalance,
    pub gold: GoldBalance,
    pub rewards: RewardBalance,
    pub shop: ShopBalance,
    pub rest: RestBalance,
    /// What a remaining heart can buy before the boss.
    pub boons: Vec<ModifierDef>,
}

/// Automatic basic lands: lands are `land_percent` of the whole deck, split
/// between colours by the coloured mana symbols of the spells. A colour the
/// deck uses at all gets at least `min_per_color` lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandBalance {
    pub land_percent: u32,
    pub min_per_color: u32,
}

/// The act's map: `rows` from the first row to the one before the boss,
/// each `columns` wide at most. The boss is one more node after the last
/// row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapBalance {
    pub columns: u8,
    pub rows: Vec<MapRowBalance>,
}

/// One row: how likely each kind of node is (relative weights), and which
/// enemies its duels use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapRowBalance {
    #[serde(default)]
    pub duel: u32,
    #[serde(default)]
    pub elite: u32,
    #[serde(default)]
    pub rest: u32,
    #[serde(default)]
    pub shop: u32,
    pub duel_tier: EnemyTier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldBalance {
    pub duel_win: u32,
    pub duel_loss: u32,
    pub elite_win: u32,
    pub elite_loss: u32,
    pub draw: u32,
    pub high_life: HighLifeBonus,
    pub fast_win: FastWinBonus,
}

/// Extra gold for winning with at least `min_life` life.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighLifeBonus {
    pub min_life: i32,
    pub gold: u32,
}

/// Extra gold for winning within `max_turns` turns, counting both players'
/// turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FastWinBonus {
    pub max_turns: u32,
    pub gold: u32,
}

/// Card rewards: `choices` different cards, the first `own_color_choices`
/// of them from the planeswalker's colours. `land_percent` is the chance
/// that one of the choices is a non-basic land instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardBalance {
    pub choices: u8,
    pub own_color_choices: u8,
    pub land_percent: u32,
    pub duel: RarityWeights,
    pub elite: RarityWeights,
}

/// Relative weights of the rarities when a card is picked at random.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RarityWeights {
    pub common: u32,
    pub uncommon: u32,
    pub rare: u32,
    pub mythic: u32,
}

impl RarityWeights {
    fn total(&self) -> u32 {
        self.common + self.uncommon + self.rare + self.mythic
    }
}

/// What a shop stocks and what it charges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopBalance {
    pub cards: u8,
    pub lands: u8,
    pub relics: u8,
    /// How many cards one shop will remove.
    pub removals: u8,
    pub card_weights: RarityWeights,
    pub prices: PriceBalance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceBalance {
    pub common: u32,
    pub uncommon: u32,
    pub rare: u32,
    pub mythic: u32,
    pub land: u32,
    pub relic: u32,
    pub removal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestBalance {
    pub hearts_restored: u8,
}

fn validate_run(run: &RunBalance) -> Result<(), BalanceError> {
    if run.hearts == 0 {
        return Err(BalanceError::Hearts);
    }
    if !(1..=90).contains(&run.lands.land_percent) {
        return Err(BalanceError::LandPercent);
    }
    if run.map.columns == 0 || run.map.rows.is_empty() {
        return Err(BalanceError::MapShape);
    }
    for (index, row) in run.map.rows.iter().enumerate() {
        if row.duel + row.elite + row.rest + row.shop == 0 {
            return Err(BalanceError::MapRowWeights(index));
        }
    }
    if run.rewards.choices == 0 || run.rewards.own_color_choices > run.rewards.choices {
        return Err(BalanceError::RewardChoices);
    }
    for (name, weights) in [
        ("duel", &run.rewards.duel),
        ("elite", &run.rewards.elite),
        ("shop", &run.shop.card_weights),
    ] {
        if weights.total() == 0 {
            return Err(BalanceError::RarityWeights(name));
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for boon in &run.boons {
        validate_modifier(boon, ModifierKind::Boon)?;
        if !ids.insert(&boon.id) {
            return Err(RunDataError::DuplicateId(boon.id.0.clone()).into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real numbers, for tests to bend.
    fn balance() -> Balance {
        serde_json::from_str(include_str!("../../../data/balance.json")).unwrap()
    }

    #[test]
    fn the_real_numbers_pass() {
        assert_eq!(validate(&balance()), Ok(()));
    }

    #[test]
    fn rejects_numbers_that_cannot_start_a_duel() {
        let check = |change: fn(&mut Balance)| {
            let mut balance = balance();
            change(&mut balance);
            validate(&balance)
        };
        assert_eq!(
            check(|b| b.duel.starting_life = 0),
            Err(BalanceError::StartingLife)
        );
        assert_eq!(
            check(|b| b.duel.smoothing_candidates = 0),
            Err(BalanceError::SmoothingCandidates)
        );
        assert_eq!(
            check(|b| b.ai.mulligan.min_lands = 6),
            Err(BalanceError::AiMulliganLands)
        );
        assert_eq!(check(|b| b.sim.turn_cap = 0), Err(BalanceError::TurnCap));
    }

    #[test]
    fn rejects_numbers_that_cannot_make_a_run() {
        let check = |change: fn(&mut Balance)| {
            let mut balance = balance();
            change(&mut balance);
            validate(&balance)
        };
        assert_eq!(check(|b| b.run.hearts = 0), Err(BalanceError::Hearts));
        assert_eq!(
            check(|b| b.run.lands.land_percent = 100),
            Err(BalanceError::LandPercent)
        );
        assert_eq!(
            check(|b| b.run.map.rows.clear()),
            Err(BalanceError::MapShape)
        );
        assert_eq!(
            check(|b| {
                let row = &mut b.run.map.rows[1];
                (row.duel, row.elite, row.rest, row.shop) = (0, 0, 0, 0);
            }),
            Err(BalanceError::MapRowWeights(1))
        );
        assert_eq!(
            check(|b| b.run.rewards.own_color_choices = b.run.rewards.choices + 1),
            Err(BalanceError::RewardChoices)
        );
        assert_eq!(
            check(|b| {
                b.run.rewards.elite = RarityWeights {
                    common: 0,
                    uncommon: 0,
                    rare: 0,
                    mythic: 0,
                }
            }),
            Err(BalanceError::RarityWeights("elite"))
        );
        assert_eq!(
            check(|b| {
                let copy = b.run.boons[0].clone();
                b.run.boons.push(copy);
            }),
            Err(BalanceError::Boon(RunDataError::DuplicateId(
                balance().run.boons[0].id.0.clone()
            )))
        );
        assert_eq!(
            check(|b| b.run.boons[0].effects.clear()),
            Err(BalanceError::Boon(RunDataError::NoEffects(
                balance().run.boons[0].id.0.clone()
            )))
        );
    }

    #[test]
    fn unknown_and_missing_fields_fail_to_parse() {
        let good = include_str!("../../../data/balance.json");
        assert!(serde_json::from_str::<Balance>(good).is_ok());
        let typo = good.replace("max_hand_size", "max_hand");
        assert!(serde_json::from_str::<Balance>(&typo).is_err());
        let negative = good.replace("\"increase\": 1", "\"increase\": -1");
        assert!(serde_json::from_str::<Balance>(&negative).is_err());
        let no_hearts = good.replace("\"hearts\": 3,", "");
        assert!(serde_json::from_str::<Balance>(&no_hearts).is_err());
    }
}
