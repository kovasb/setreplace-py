//! PCG32 (same generator as the setreplace engine) for deterministic layouts.

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

    /// Uniform in [0, 1).
    pub(crate) fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }
}
