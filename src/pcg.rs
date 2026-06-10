//! Minimal PCG32 random number generator (no external dependencies).
//!
//! libSetReplace uses `std::mt19937` with `std::uniform_int_distribution`, whose
//! output stream is implementation-defined and therefore not reproducible across
//! platforms even in the original. We use PCG32 instead: deterministic for a given
//! seed, everywhere.

pub(crate) struct Pcg32 {
    state: u64,
    inc: u64,
}

const PCG_DEFAULT_STREAM: u64 = 0xda3e_39cb_94b9_5bdb;
const PCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;

impl Pcg32 {
    pub(crate) fn new(seed: u64) -> Self {
        let mut rng = Pcg32 {
            state: 0,
            inc: (PCG_DEFAULT_STREAM << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(PCG_MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform integer in `0..n` without modulo bias. `n` must be nonzero.
    pub(crate) fn gen_index(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        let n = n as u64;
        let range = u64::from(u32::MAX) + 1;
        let zone = range - range % n;
        loop {
            let x = u64::from(self.next_u32());
            if x < zone {
                return (x % n) as usize;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_seed() {
        let mut a = Pcg32::new(42);
        let mut b = Pcg32::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn gen_index_in_range() {
        let mut rng = Pcg32::new(7);
        for n in 1..50 {
            for _ in 0..100 {
                assert!(rng.gen_index(n) < n);
            }
        }
    }
}
