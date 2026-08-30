//! Wind regression: the original sine swing pinned each round to one
//! deterministic wind "story" and left the live value near-constant over a
//! ball's ~4.5 s flight. Its replacement is two Ornstein–Uhlenbeck layers
//! (slow regime base + fast gusts). The fairness invariants that must hold:
//! round starts spread over both signs, the live wind keeps moving during
//! one flight, |speed| never leaves the ±14 m/s envelope the rest of the
//! suite tunes against, and rounds do not stay one-signed forever.

#![allow(clippy::cast_precision_loss)] // seed/step counts are exact in f32

use siege::game::Wind;
use siege::physics::DT;
use siege::rng::Rng;

const SEEDS: usize = 200;
const HORIZON_STEPS: usize = 120 * 240; // 120 s at the game substep

/// One 120 s trajectory integrated at the game's substep rate.
fn trajectory(seed: u64) -> Vec<f32> {
    let mut rng = Rng::seed(seed);
    let mut wind = Wind::new(&mut rng);
    let mut speeds = Vec::with_capacity(HORIZON_STEPS);
    for _ in 0..HORIZON_STEPS {
        wind.step(DT, &mut rng);
        speeds.push(wind.current());
    }
    speeds
}

/// PCG32 is seed-deterministic, so `Wind::new` is an exact uniform draw on
/// ±12 m/s: the split must sit near 50/50. A biased start (the old bug —
/// rounds always opened headwind for the player) skews this hard.
#[test]
fn round_starts_cover_both_signs() {
    let positive = (0..SEEDS as u64)
        .filter(|seed| Wind::new(&mut Rng::seed(*seed)).current() > 0.0)
        .count();
    let frac = positive as f32 / SEEDS as f32;
    assert!(
        (0.35..=0.65).contains(&frac),
        "start sign balance skewed: {positive}/{SEEDS} positive"
    );
}

/// A shot hangs in the air ~4.5 s; the wind it meets must not be the value
/// it was aimed under. Mean drift over one flight is ~2.7 m/s in validation.
#[test]
fn wind_moves_during_a_flight() {
    const FLIGHT_STEPS: usize = 4 * 240 + 120; // 4.5 s
    let mut deltas = Vec::with_capacity(SEEDS);
    for seed in 0..SEEDS as u64 {
        let mut rng = Rng::seed(seed);
        let mut wind = Wind::new(&mut rng);
        let start = wind.current();
        for _ in 0..FLIGHT_STEPS {
            wind.step(DT, &mut rng);
        }
        deltas.push((wind.current() - start).abs());
    }
    let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
    let max = deltas.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        mean >= 1.5,
        "mean in-flight wind change {mean:.2} m/s < 1.5"
    );
    assert!(max >= 3.0, "worst in-flight wind change {max:.2} m/s < 3.0");
}

/// The AI wind-learning bracket ([−16, 16]) and `reach_probe` both assume
/// the ±14 envelope; gust noise must never escape it.
#[test]
fn wind_stays_inside_envelope() {
    for seed in 0..SEEDS as u64 {
        for w in trajectory(seed) {
            assert!(w.abs() <= 14.0, "seed {seed}: |wind| {w} exceeded ±14");
        }
    }
}

/// The sin swing was introduced because a frozen per-round base left most
/// rounds one-signed forever. The regime layer must actually carry rounds
/// across zero; a regression that re-freezes the base collapses this count.
#[test]
fn rounds_do_not_stay_one_signed() {
    let crossed = (0..SEEDS as u64)
        .filter(|seed| {
            let t = trajectory(*seed);
            t.iter().any(|w| *w > 0.0) && t.iter().any(|w| *w < 0.0)
        })
        .count();
    let frac = crossed as f32 / SEEDS as f32;
    assert!(
        frac >= 0.80,
        "only {crossed}/{SEEDS} rounds changed wind sign within 120 s"
    );
}
