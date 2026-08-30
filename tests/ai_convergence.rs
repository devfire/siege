//! AI convergence contract — the marquee requirement, fully offline.
//!
//! With a biased wind estimator (plan hears wind ≈ 11 while truth is 8) the
//! probe → observe → secant loop must land within 3.5 m of the player in ≤ 4
//! shots for at least 45 of 50 seeds.
//!
//! Reality is simulated on `world::ground_height` — the same ground the
//! in-game ball substep collides against — so the AI's planning model and
//! the test's "physics" agree everywhere except the wind estimate, which is
//! exactly the error source under test.

use siege::ai::DefenderAi;
use siege::physics::simulate_landing;
use siege::rng::Rng;
use siege::world::defender_pivot;

const TARGET_X: f32 = 16.0;
const WIND_TRUE: f32 = 8.0;
const ZERO_R: f32 = 3.5;

#[test]
fn secant_zeroes_in_on_player() {
    let mut converged = 0;
    for seed in 0..=49u64 {
        let mut rng = Rng::seed(seed);
        let mut ai = DefenderAi::new();
        // Biased estimator: plan itself adds N(0, 2.5), so the estimate is
        // true + 3.0 + noise.
        ai.plan(WIND_TRUE + 3.0, TARGET_X, &mut rng);
        let mut shots = 0;
        let mut within_budget = false;
        for _ in 0..8 {
            let (angle, charge) = ai.current_aim();
            let impact = simulate_landing(
                defender_pivot(),
                angle,
                charge,
                -1.0,
                WIND_TRUE,
                &siege::world::ground_height,
            );
            shots += 1;
            let err = impact.x - TARGET_X;
            ai.observe(impact.x, TARGET_X);
            if err.abs() <= ZERO_R {
                within_budget = shots <= 4;
                break;
            }
        }
        if within_budget {
            converged += 1;
        }
    }
    assert!(
        converged >= 45,
        "only {converged}/50 seeds converged within 4 shots"
    );
}
