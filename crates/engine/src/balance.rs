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
    pub search: AiSearchBalance,
}

/// How hard the searching AI thinks. For each thing it might do it plays
/// the game forward in `worlds` guesses at the cards it can't see, so its
/// thinking time grows with `worlds` times the number of things it tries.
/// These are counts, never a time limit: the same position is always
/// thought about in the same way, on any machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSearchBalance {
    /// Guesses at the hidden cards per decision.
    pub worlds: u32,
    /// How many spells and abilities are tried besides passing: the ones
    /// that look best at a glance.
    pub candidates: u32,
    /// How many ways of aiming one spell or ability are among them.
    pub targets_per_spell: u32,
    /// How many whole attacks are tried when it declares attackers: v1's
    /// attack, everything, nothing, then changes of one creature to the
    /// best so far.
    pub attack_plans: u32,
    /// How many whole sets of blocks are tried when it declares blockers.
    pub block_plans: u32,
    /// How far each imagined game is played: to the start of the turn this
    /// many turns after the current one.
    pub turns: u32,
    /// An imagined game stops after this many actions, however far it got.
    pub max_actions: u32,
    /// In an imagined game, how many choices of targets, one after another,
    /// a player thinks through when weighing a spell (the real version 1
    /// thinks through two). 0 makes the imagined players cheapest.
    pub rollout_lookahead: u8,
    /// In an imagined game, whether two creatures block one attacker to
    /// kill it, which the real version 1 never does. Without it the
    /// searching AI can't see that danger when it attacks.
    pub rollout_gang_blocks: bool,
    /// What winning is worth, on the scale positions are scored on (a
    /// creature is worth about 5 to 15). Positions are capped just below
    /// it, so that a small chance of winning counts as a small chance.
    pub win_value: i32,
    /// Whether the AI knows which cards are in the opponent's deck (never
    /// which of them are in their hand, or the order of their library).
    /// Without it, it guesses from the cards of the colours they have shown.
    pub knows_decklist: bool,
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
    /// Generic mana a commander costs more for each time it has already
    /// been cast from the command zone this duel.
    pub commander_tax: u32,
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
    #[error(
        "`ai.search`: every count (`worlds`, `candidates`, `attack_plans`, `turns` and so on) and \
         `win_value` must be above zero"
    )]
    AiSearch,
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
    #[error("`run.draft` needs at least one pick, and no more picks than cards")]
    DraftPicks,
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
    let search = &balance.ai.search;
    if search.worlds == 0
        || search.candidates == 0
        || search.targets_per_spell == 0
        || search.attack_plans == 0
        || search.block_plans == 0
        || search.turns == 0
        || search.max_actions == 0
        || search.win_value <= 0
    {
        return Err(BalanceError::AiSearch);
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
    pub draft: DraftBalance,
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
    #[serde(default)]
    pub draft: u32,
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

/// A draft node: a pack of `cards` different cards in the planeswalker's
/// colours, of which up to `picks` are taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftBalance {
    pub cards: u8,
    pub picks: u8,
    pub weights: RarityWeights,
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
        if row.duel + row.elite + row.rest + row.shop + row.draft == 0 {
            return Err(BalanceError::MapRowWeights(index));
        }
    }
    if run.rewards.choices == 0 || run.rewards.own_color_choices > run.rewards.choices {
        return Err(BalanceError::RewardChoices);
    }
    if run.draft.picks == 0 || run.draft.picks > run.draft.cards {
        return Err(BalanceError::DraftPicks);
    }
    for (name, weights) in [
        ("draft", &run.draft.weights),
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
        assert_eq!(
            check(|b| b.ai.search.worlds = 0),
            Err(BalanceError::AiSearch)
        );
        assert_eq!(
            check(|b| b.ai.search.win_value = 0),
            Err(BalanceError::AiSearch)
        );
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
            check(|b| b.run.draft.picks = b.run.draft.cards + 1),
            Err(BalanceError::DraftPicks)
        );
        assert_eq!(
            check(|b| b.run.draft.picks = 0),
            Err(BalanceError::DraftPicks)
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
