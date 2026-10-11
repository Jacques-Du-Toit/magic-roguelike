//! The duel's only source of randomness.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

/// A seeded generator that lives in `DuelState`.
///
/// The bounded draw and the shuffle are written here rather than taken from
/// the `rand` crate, so a dependency upgrade can never change what a seed
/// produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuelRng(ChaCha8Rng);

impl DuelRng {
    pub fn from_seed(seed: u64) -> DuelRng {
        DuelRng(ChaCha8Rng::seed_from_u64(seed))
    }

    /// A uniform number in `0..bound`. Returns 0 when `bound` is 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        // Lemire's method: multiply into 64 bits and reject the few low
        // results that would make some outcomes more likely than others.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let product = u64::from(self.0.next_u32()) * u64::from(bound);
            if (product & u64::from(u32::MAX)) >= u64::from(threshold) {
                return (product >> 32) as u32;
            }
        }
    }

    /// Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for last in (1..items.len()).rev() {
            let bound = u32::try_from(last + 1).unwrap_or(u32::MAX);
            let pick = self.below(bound) as usize;
            items.swap(last, pick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_the_same_sequence() {
        let mut a = DuelRng::from_seed(7);
        let mut b = DuelRng::from_seed(7);
        let mut c = DuelRng::from_seed(8);
        let draws = |rng: &mut DuelRng| (0..50).map(|_| rng.below(1000)).collect::<Vec<_>>();
        let from_a = draws(&mut a);
        assert_eq!(from_a, draws(&mut b));
        assert_ne!(from_a, draws(&mut c));
    }

    #[test]
    fn below_stays_in_range_and_reaches_every_value() {
        let mut rng = DuelRng::from_seed(1);
        let mut seen = [false; 6];
        for _ in 0..500 {
            seen[rng.below(6) as usize] = true;
        }
        assert_eq!(seen, [true; 6]);
        assert_eq!(rng.below(1), 0);
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn shuffle_keeps_every_item() {
        let mut rng = DuelRng::from_seed(3);
        let mut items: Vec<u32> = (0..25).collect();
        rng.shuffle(&mut items);
        assert_ne!(items, (0..25).collect::<Vec<_>>());
        items.sort_unstable();
        assert_eq!(items, (0..25).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_reaches_every_order_of_three() {
        let mut rng = DuelRng::from_seed(11);
        let mut orders = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let mut items = [0, 1, 2];
            rng.shuffle(&mut items);
            orders.insert(items);
        }
        assert_eq!(orders.len(), 6);
    }

    #[test]
    fn survives_a_save_in_the_middle_of_a_sequence() {
        let mut rng = DuelRng::from_seed(42);
        for _ in 0..37 {
            rng.below(100);
        }
        let json = serde_json::to_string(&rng).unwrap();
        let mut restored: DuelRng = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, rng);
        for _ in 0..20 {
            assert_eq!(restored.below(1000), rng.below(1000));
        }
    }
}
