//! Tunable numbers, loaded from `data/balance.json`. Nothing in the engine
//! hard-codes a balance value (CLAUDE.md rule 5).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::run::EnemyTier;

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
    /// A game with more permanents than this on the battlefield is stopped
    /// and reported the same way: a board that crowded is a stalemate, and
    /// the searching AI takes seconds over each decision on it.
    pub board_cap: u32,
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
    /// How many attacks are tried, in its first main phase, for passing
    /// with an instant kept for after blocks: v1's attack, that with one
    /// creature more, everything. 0 never weighs keeping one for combat.
    pub hold_attacks: u32,
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
    #[error("`sim.board_cap` must be above zero")]
    BoardCap,
    #[error("`sim.turn_cap` must be above zero")]
    TurnCap,
    #[error("`run.max_wounds` must be above zero")]
    MaxWounds,
    #[error("`run.lands.land_percent` must be between 1 and 90")]
    LandPercent,
    #[error("`run.map` needs at least one column and one row")]
    MapShape,
    #[error("`run.map.rows[{0}]` gives no node kind a weight")]
    MapRowWeights(usize),
    #[error("`run.rewards.loss_choices` must be above zero")]
    RewardChoices,
    #[error("`run.feats` needs at least one pick, and no more picks than feats offered")]
    FeatPicks,
    #[error(
        "`run.feats.rewards` needs a first row for 0 points, rows in rising order of points, \
         and at least one card to choose from in each"
    )]
    FeatRewards,
    #[error("`run.draft` needs at least one pick, and no more picks than cards")]
    DraftPicks,
    #[error("`run.rewards.{0}` gives no rarity a weight")]
    RarityWeights(&'static str),
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
    if balance.sim.board_cap == 0 {
        return Err(BalanceError::BoardCap);
    }
    if balance.sim.turn_cap == 0 {
        return Err(BalanceError::TurnCap);
    }
    validate_run(&balance.run)
}

/// Every number of the run layer: wounds, gold, prices, rewards, the map's
/// shape and the land share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBalance {
    /// The wound that ends the run: with 3, a player can carry two.
    pub max_wounds: u8,
    pub starting_gold: u32,
    /// A card can't be removed from a deck with this many spells and
    /// non-basic lands or fewer.
    pub min_deck_cards: u32,
    pub lands: LandBalance,
    pub map: MapBalance,
    pub gold: GoldBalance,
    pub rewards: RewardBalance,
    pub feats: FeatBalance,
    pub draft: DraftBalance,
    pub treasure: TreasureBalance,
    pub event: EventBalance,
    pub shop: ShopBalance,
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
    #[serde(default)]
    pub treasure: u32,
    #[serde(default)]
    pub event: u32,
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

/// Card rewards: different cards to choose one from, the first
/// `own_color_choices` of them from the planeswalker's colours.
/// `land_percent` is the chance that one of the choices is a non-basic land
/// instead. How many cards a won duel offers depends on the feats
/// (`FeatBalance`); a lost one offers `loss_choices`. The rarity weights
/// are a ladder: `duel` for a duel, `elite` for an elite, and each step up
/// that the feats earn moves one along, `top` being the last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardBalance {
    pub loss_choices: u8,
    pub own_color_choices: u8,
    pub land_percent: u32,
    pub duel: RarityWeights,
    pub elite: RarityWeights,
    pub top: RarityWeights,
}

/// Feats: before a duel `offered` are shown and `picks` of them taken.
/// `rewards` says what a won duel pays for the points they scored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatBalance {
    pub offered: u8,
    pub picks: u8,
    pub rewards: Vec<FeatReward>,
}

/// What a won duel pays when its feats scored `points` or more (and fewer
/// than the next row's): `choices` cards to choose from, their rarity
/// weights `rarity_steps` up the ladder, and `gold` on top of the duel's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FeatReward {
    pub points: u32,
    pub choices: u8,
    #[serde(default)]
    pub rarity_steps: u8,
    #[serde(default)]
    pub gold: u32,
}

impl FeatBalance {
    /// The row for a duel that scored `points`.
    pub fn reward(&self, points: u32) -> Option<&FeatReward> {
        self.rewards.iter().rfind(|row| row.points <= points)
    }
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

/// A treasure node: a choice of up to `relics` relics the player doesn't
/// own, or `gold` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureBalance {
    pub relics: u8,
    pub gold: u32,
}

/// Event nodes. What each event offers and costs is beside its text in
/// `data/run/events.json`; `card_weights` pick the cards an event shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventBalance {
    pub card_weights: RarityWeights,
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

fn validate_run(run: &RunBalance) -> Result<(), BalanceError> {
    if run.max_wounds == 0 {
        return Err(BalanceError::MaxWounds);
    }
    if !(1..=90).contains(&run.lands.land_percent) {
        return Err(BalanceError::LandPercent);
    }
    if run.map.columns == 0 || run.map.rows.is_empty() {
        return Err(BalanceError::MapShape);
    }
    for (index, row) in run.map.rows.iter().enumerate() {
        let weights = [
            row.duel,
            row.elite,
            row.rest,
            row.shop,
            row.draft,
            row.treasure,
            row.event,
        ];
        if weights.iter().sum::<u32>() == 0 {
            return Err(BalanceError::MapRowWeights(index));
        }
    }
    if run.rewards.loss_choices == 0 {
        return Err(BalanceError::RewardChoices);
    }
    if run.feats.picks == 0 || run.feats.picks > run.feats.offered {
        return Err(BalanceError::FeatPicks);
    }
    let rows = &run.feats.rewards;
    if rows.first().is_none_or(|row| row.points != 0)
        || !rows.windows(2).all(|pair| pair[0].points < pair[1].points)
        || rows.iter().any(|row| row.choices == 0)
    {
        return Err(BalanceError::FeatRewards);
    }
    if run.draft.picks == 0 || run.draft.picks > run.draft.cards {
        return Err(BalanceError::DraftPicks);
    }
    for (name, weights) in [
        ("draft", &run.draft.weights),
        ("event", &run.event.card_weights),
        ("duel", &run.rewards.duel),
        ("elite", &run.rewards.elite),
        ("top", &run.rewards.top),
        ("shop", &run.shop.card_weights),
    ] {
        if weights.total() == 0 {
            return Err(BalanceError::RarityWeights(name));
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
        assert_eq!(check(|b| b.sim.board_cap = 0), Err(BalanceError::BoardCap));
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
        assert_eq!(
            check(|b| b.run.max_wounds = 0),
            Err(BalanceError::MaxWounds)
        );
        assert_eq!(
            check(|b| b.run.rewards.loss_choices = 0),
            Err(BalanceError::RewardChoices)
        );
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
        // A row of nothing but treasure is a row.
        assert_eq!(
            check(|b| {
                let row = &mut b.run.map.rows[1];
                (row.duel, row.treasure) = (0, 1);
            }),
            Ok(())
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
            check(|b| b.run.feats.picks = b.run.feats.offered + 1),
            Err(BalanceError::FeatPicks)
        );
        assert_eq!(
            check(|b| b.run.feats.picks = 0),
            Err(BalanceError::FeatPicks)
        );
        // The feats' rewards start at 0 points, rise, and always offer a card.
        assert_eq!(
            check(|b| b.run.feats.rewards[0].points = 1),
            Err(BalanceError::FeatRewards)
        );
        assert_eq!(
            check(|b| b.run.feats.rewards.swap(1, 2)),
            Err(BalanceError::FeatRewards)
        );
        assert_eq!(
            check(|b| b.run.feats.rewards[1].choices = 0),
            Err(BalanceError::FeatRewards)
        );
        assert_eq!(
            check(|b| b.run.feats.rewards.clear()),
            Err(BalanceError::FeatRewards)
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
    }

    #[test]
    fn a_duels_points_pick_the_last_row_they_reach() {
        let feats = balance().run.feats;
        let choices = |points: u32| feats.reward(points).map(|row| row.choices);
        assert_eq!(choices(0), Some(2));
        assert_eq!((choices(1), choices(2)), (Some(3), Some(3)));
        assert_eq!((choices(3), choices(4)), (Some(3), Some(3)));
        assert_eq!((choices(5), choices(6)), (Some(4), Some(4)));
        assert_eq!(feats.reward(2).map(|row| row.rarity_steps), Some(0));
        assert_eq!(
            feats.reward(3).map(|row| (row.rarity_steps, row.gold)),
            Some((1, 15))
        );
        assert_eq!(
            feats.reward(6).map(|row| (row.rarity_steps, row.gold)),
            Some((1, 30))
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
        let no_wounds = good.replace("\"max_wounds\": 3,", "");
        assert!(serde_json::from_str::<Balance>(&no_wounds).is_err());
    }
}
