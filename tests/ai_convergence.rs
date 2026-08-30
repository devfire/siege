//! AI convergence contract — the marquee requirement, fully offline.
//!
//! With a biased wind estimator (plan hears wind ≈ 11 while truth is 8) the
//! probe → observe → secant loop must land within 3.5 m of the player in ≤ 4
//! shots for at least 45 of 50 seeds.
//!
//! Reality is simulated on `world::ground_height` plus the same castle
//! obstacle set the in-game ball substep collides against — the AI's
//! planning model and the test's "physics" share one world everywhere
//! except the wind estimate, which is exactly the error source under test.

use siege::ai::DefenderAi;
use siege::physics::{Landing, simulate_landing};
use siege::rng::Rng;
use siege::world::{castle_segments, collidables, defender_pivot, ground_height};

/// The missing joint invariant (issue #5): the defender can zero in on the
/// player at *every* wind value the `Wind` model can produce. `K_DRAG`,
/// `MUZZLE_V_MAX`, the 155 m pivot separation, and the ±14 m/s swing are
/// one tuning envelope; nothing else in the suite pins it. Measured worst
/// case over the envelope is 0.12 m; `ZERO_IN` is 3.5 — assert with margin.
#[test]
fn defender_reaches_player_at_every_wind() {
    const WORST_ALLOWED: f32 = 2.0;
    let target = 16.0_f32;
    let muzzle = defender_pivot();
    let obs = obstacles();
    #[allow(clippy::cast_precision_loss)] // integer wind steps, not float-derived
    for w in (-14_i32..=14).step_by(1) {
        let wind = w as f32;
        let mut best = f32::MAX;
        for ai in 0..=40u8 {
            for ci in 0..=40u8 {
                if let Landing::Ground(p) = simulate_landing(
                    muzzle,
                    25.0 + f32::from(ai),
                    0.2 + 0.02 * f32::from(ci),
                    -1.0,
                    wind,
                    ground_height,
                    &obs,
                ) {
                    best = best.min((p.x - target).abs());
                }
            }
        }
        assert!(
            best <= WORST_ALLOWED,
            "wind {wind:+.0}: defender cannot zero in — best |err| {best:.2} m > {WORST_ALLOWED}"
        );
    }
}

const TARGET_X: f32 = 16.0;
const WIND_TRUE: f32 = 8.0;
const ZERO_R: f32 = 3.5;

fn obstacles() -> Vec<siege::physics::Obstacle> {
    collidables(&castle_segments())
}

#[test]
fn secant_zeroes_in_on_player() {
    let obs = obstacles();
    let mut converged = 0;
    for seed in 0..=49u64 {
        let mut rng = Rng::seed(seed);
        let mut ai = DefenderAi::new();
        // Biased estimator: plan itself adds N(0, 2.5), so the estimate is
        // true + 3.0 + noise.
        ai.plan(WIND_TRUE + 3.0, TARGET_X, &mut rng, &castle_segments());
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
                ground_height,
                &obs,
            );
            shots += 1;
            let Landing::Ground(at) = impact else {
                // Non-ground contact (own wall/rubble/cannon): tell the AI.
                ai.observe(f32::NAN, TARGET_X, false);
                continue;
            };
            let err = at.x - TARGET_X;
            ai.observe(at.x, TARGET_X, true);
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

/// The planning model must never select a shot that flies through the
/// defender's own castle: whatever it plans must, under the same obstacle
/// set, actually reach the ground.
#[test]
fn plan_never_shoots_through_own_castle() {
    for seed in 0..=9u64 {
        let mut rng = Rng::seed(seed);
        let mut ai = DefenderAi::new();
        ai.plan(0.0, TARGET_X, &mut rng, &castle_segments());
        let (angle, charge) = ai.current_aim();
        let obs = obstacles();
        let landing = simulate_landing(
            defender_pivot(),
            angle,
            charge,
            -1.0,
            0.0, // calm: the plan's wind estimate is noisy around 0 at seed probes
            ground_height,
            &obs,
        );
        assert!(
            !matches!(landing, Landing::Obstacle(_)),
            "seed {seed}: plan {angle}°/{charge:.2} strikes the defender's own castle: {landing:?}"
        );
    }
}
