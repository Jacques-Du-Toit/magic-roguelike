//! Tunable numbers, loaded from `data/balance.json`. Nothing in the engine
//! hard-codes a balance value (CLAUDE.md rule 5).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Balance {
    pub duel: DuelBalance,
    pub ai: AiBalance,
    pub sim: SimBalance,
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balance(starting_life: i32, smoothing_candidates: u8) -> Balance {
        Balance {
            duel: DuelBalance {
                starting_life,
                opening_hand_size: 7,
                max_hand_size: 7,
                smoothing_candidates,
                fatigue: FatigueBalance {
                    first_life_loss: 1,
                    increase: 1,
                },
            },
            ai: AiBalance {
                mulligan: AiMulliganBalance {
                    min_lands: 2,
                    max_lands: 5,
                    min_hand_size: 5,
                },
            },
            sim: SimBalance {
                turn_cap: 100,
                land_screw: LandScrewBalance {
                    turn: 4,
                    max_lands: 2,
                },
                land_flood: LandFloodBalance {
                    turn: 6,
                    min_land_percent: 60,
                },
            },
        }
    }

    #[test]
    fn sensible_numbers_pass() {
        assert_eq!(validate(&balance(20, 3)), Ok(()));
        assert_eq!(validate(&balance(1, 1)), Ok(()));
    }

    #[test]
    fn rejects_numbers_that_cannot_start_a_duel() {
        assert_eq!(validate(&balance(0, 3)), Err(BalanceError::StartingLife));
        assert_eq!(
            validate(&balance(20, 0)),
            Err(BalanceError::SmoothingCandidates)
        );
        let mut backwards = balance(20, 3);
        backwards.ai.mulligan.min_lands = 6;
        assert_eq!(validate(&backwards), Err(BalanceError::AiMulliganLands));
        let mut endless = balance(20, 3);
        endless.sim.turn_cap = 0;
        assert_eq!(validate(&endless), Err(BalanceError::TurnCap));
    }

    #[test]
    fn unknown_and_missing_fields_fail_to_parse() {
        let good = r#"{ "duel": { "starting_life": 20, "opening_hand_size": 7, "max_hand_size": 7,
            "smoothing_candidates": 3, "fatigue": { "first_life_loss": 1, "increase": 1 } },
            "ai": { "mulligan": { "min_lands": 2, "max_lands": 5, "min_hand_size": 5 } },
            "sim": { "turn_cap": 100, "land_screw": { "turn": 4, "max_lands": 2 },
                "land_flood": { "turn": 6, "min_land_percent": 60 } } }"#;
        assert_eq!(
            serde_json::from_str::<Balance>(good).unwrap(),
            balance(20, 3)
        );
        let typo = good.replace("max_hand_size", "max_hand");
        assert!(serde_json::from_str::<Balance>(&typo).is_err());
        let negative = good.replace("\"increase\": 1", "\"increase\": -1");
        assert!(serde_json::from_str::<Balance>(&negative).is_err());
    }
}
