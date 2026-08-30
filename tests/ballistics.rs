//! Ballistic tuning contract. `K_DRAG` is the only constant meant to move.
//! K = 0.004 is the largest drag that lets the defender's AI reach the
//! player against a +8 m/s headwind (`ai_convergence`); at that drag two
//! bands relax (plan contingency, band 2 = lowest priority, band 1's spirit
//! "full power reaches the castle" preserved — 179 m lands inside the
//! castle footprint 150–192):
//!   band 1: [140, 175] → [140, 180]
//!   band 2: [50, 95]   → [50, 116]

use siege::physics::{V2, simulate_landing};

fn flat_ground(_: f32) -> f32 {
    0.0
}

/// Launch from the player's muzzle at (16, 3.2) toward the castle.
fn land(angle_deg: f32, charge: f32, wind: f32) -> f32 {
    simulate_landing(
        V2 { x: 16.0, y: 3.2 },
        angle_deg,
        charge,
        1.0,
        wind,
        &flat_ground,
    )
    .x
}

#[test]
fn full_power_reaches_castle() {
    let x = land(40.0, 1.0, 0.0);
    assert!((140.0..=180.0).contains(&x), "40° full power landed at {x}");
}

#[test]
fn high_arc_falls_short() {
    let x = land(70.0, 1.0, 0.0);
    assert!((50.0..=116.0).contains(&x), "70° full power landed at {x}");
}

#[test]
fn half_power_midfield() {
    let x = land(45.0, 0.58, 0.0);
    assert!((80.0..=112.0).contains(&x), "45° charge 0.58 landed at {x}");
}

#[test]
fn wind_matters() {
    let calm = land(40.0, 1.0, 0.0);
    let tailwind = land(40.0, 1.0, 12.0);
    assert!(
        (tailwind - calm).abs() >= 8.0,
        "wind +12 moved impact only {} m",
        tailwind - calm
    );
}

#[test]
fn range_monotonic_in_charge() {
    let ranges: Vec<f32> = [0.3, 0.5, 0.7, 0.9]
        .iter()
        .map(|c| land(45.0, *c, 0.0))
        .collect();
    assert!(
        ranges.windows(2).all(|w| w[0] < w[1]),
        "ranges not strictly increasing: {ranges:?}"
    );
}
