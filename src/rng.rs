//! Deterministic PCG32 random number generator — no external deps.
//!
//! Identical seeds produce identical sequences on native and wasm, so AI
//! behavior and world decoration are reproducible from a single `u64` seed.

/// PCG32 state (permuted congruential generator, XSH-RR output).
#[derive(Clone)]
pub struct Rng {
    state: u64,
    inc: u64,
}

const PCG_MULT: u64 = 6_364_136_223_846_793_005;
const PCG_INC: u64 = 0xdead_beef;

impl Rng {
    /// Deterministically scramble `seed` into an initial PCG32 state.
    #[must_use]
    pub fn seed(seed: u64) -> Self {
        Self {
            state: seed
                .wrapping_mul(PCG_MULT)
                .wrapping_add(1_442_695_040_888_963_407),
            inc: PCG_INC,
        }
    }

    /// Standard PCG32 output word.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // truncation is the hash
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(PCG_MULT).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `[0, 1)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // 2^24 mantissa quantization is fine here
    pub fn f01(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
    }

    /// Uniform in `[a, b)`.
    #[must_use]
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f01()
    }

    /// Standard normal via Box–Muller (log(0) guarded at 1e-9).
    #[must_use]
    pub fn gauss(&mut self) -> f32 {
        let u1 = self.f01().max(1e-9);
        let u2 = self.f01();
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }
}
