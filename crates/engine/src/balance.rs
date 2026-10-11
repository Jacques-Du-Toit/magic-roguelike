//! Tunable numbers, loaded from `data/balance.json`. Nothing in the engine
//! hard-codes a balance value (CLAUDE.md rule 5).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Balance {
    pub duel: DuelBalance,
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
}

pub fn validate(balance: &Balance) -> Result<(), BalanceError> {
    if balance.duel.starting_life <= 0 {
        return Err(BalanceError::StartingLife);
    }
    if balance.duel.smoothing_candidates == 0 {
        return Err(BalanceError::SmoothingCandidates);
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
    }

    #[test]
    fn unknown_and_missing_fields_fail_to_parse() {
        let good = r#"{ "duel": { "starting_life": 20, "opening_hand_size": 7, "max_hand_size": 7,
            "smoothing_candidates": 3, "fatigue": { "first_life_loss": 1, "increase": 1 } } }"#;
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
