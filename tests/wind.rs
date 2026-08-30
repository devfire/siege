//! Wind regime regression: the old model froze a per-round base in
//! [-12, 12] with only a ±2 m/s wobble on top, so most rounds stayed
//! one-signed forever (a negative wind never turned positive again). The
//! swing model must sweep from strongly positive to strongly negative
//! within one slow period (~209 s) from any phase pair.

#![allow(clippy::cast_precision_loss)] // sample indices ≤ 1_040 are exact in f32

use std::f32::consts::TAU;

use siege::game::Wind;

const PHASE_STEPS: usize = 16;
const SAMPLES: usize = 1_041; // 260 s at 0.25 s steps; horizon > slow period
const SAMPLE_DT: f32 = 0.25;

#[test]
fn wind_sweeps_both_signs() {
    for i in 0..PHASE_STEPS {
        for j in 0..PHASE_STEPS {
            let wind = Wind {
                slow_phase: TAU * i as f32 / PHASE_STEPS as f32,
                fast_phase: TAU * j as f32 / PHASE_STEPS as f32,
            };
            let (max_w, min_w) = (0..SAMPLES)
                .map(|k| wind.current(k as f32 * SAMPLE_DT))
                .fold((f32::MIN, f32::MAX), |(mx, mn), w| (mx.max(w), mn.min(w)));
            assert!(
                max_w > 1.0 && min_w < -1.0,
                "phases ({}, {}): wind stuck in [{min_w}, {max_w}]",
                wind.slow_phase,
                wind.fast_phase,
            );
        }
    }
}
