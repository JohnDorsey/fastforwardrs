//! Helpers shared by the unit tests in several modules.

/// A tiny deterministic random number generator, so that randomised tests are reproducible
/// without adding a dependency. (Marsaglia's xorshift64.)
pub struct DeterministicRandomNumbers(u64);

impl DeterministicRandomNumbers {
    /// Different seeds give different sequences; any seed is fine (zero is replaced).
    pub fn with_seed(seed: u64) -> Self {
        let scrambled = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        Self(if scrambled == 0 { 1 } else { scrambled })
    }

    /// A number from 0 up to but not including `upper_bound`.
    pub fn next_below(&mut self, upper_bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % upper_bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_sequence_and_different_seeds_differ() {
        let sequence = |seed| {
            let mut random = DeterministicRandomNumbers::with_seed(seed);
            (0..20).map(|_| random.next_below(1000)).collect::<Vec<_>>()
        };
        assert_eq!(sequence(7), sequence(7));
        assert_ne!(sequence(7), sequence(8));
    }

    #[test]
    fn numbers_stay_below_the_bound_and_cover_the_range() {
        let mut random = DeterministicRandomNumbers::with_seed(1);
        let mut seen = [false; 5];
        for _ in 0..200 {
            let number = random.next_below(5);
            assert!(number < 5);
            seen[number as usize] = true;
        }
        assert!(seen.iter().all(|&was_seen| was_seen), "{seen:?}");
    }

    #[test]
    fn a_seed_of_zero_still_works() {
        let mut random = DeterministicRandomNumbers::with_seed(0);
        assert!((0..50).map(|_| random.next_below(1000)).any(|number| number != 0));
    }
}
