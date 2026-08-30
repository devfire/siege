//! Ballistic tuning contract. The projectile is a 12-pounder iron shot:
//! `K_DRAG` is *derived* from `SHOT_R`/`SHOT_M`/`C_D` (issue #6) and
//! `MUZZLE_V_MAX` is sized so a full-power 40° shot lands on the keep
//! (169.7 m, inside the 150–192 castle footprint). Bands below are
//! re-derived from that projectile — measured values with ±10 % slack —
//! not relaxed to fit a hand-picked drag constant.

use siege::physics::{Landing, V2, simulate_landing};

fn flat_ground(_: f32) -> f32 {
    0.0
}

/// Launch from the player's muzzle at (16, 3.2) toward the castle. No
/// obstacles: these bands measure raw ballistic range over terrain.
fn land(angle_deg: f32, charge: f32, wind: f32) -> f32 {
    match simulate_landing(
        V2 { x: 16.0, y: 3.2 },
        angle_deg,
        charge,
        1.0,
        wind,
        flat_ground,
        &[],
    ) {
        Landing::Ground(p) => p.x,
        other => panic!("expected a ground landing, got {other:?}"),
    }
}

/// Full power at 40° must land on the keep: measured 169.7 m.
#[test]
fn full_power_reaches_castle() {
    let x = land(40.0, 1.0, 0.0);
    assert!((155.0..=185.0).contains(&x), "40° full power landed at {x}");
}

/// High arc falls well short: measured 115.1 m.
#[test]
fn high_arc_falls_short() {
    let x = land(70.0, 1.0, 0.0);
    assert!((105.0..=125.0).contains(&x), "70° full power landed at {x}");
}

/// Mid charge, mid arc: measured 75.1 m.
#[test]
fn half_power_midfield() {
    let x = land(45.0, 0.58, 0.0);
    assert!((68.0..=83.0).contains(&x), "45° charge 0.58 landed at {x}");
}

/// Wind authority for a 1.3-tonne iron sphere is physically modest
/// (~5 m over a full flight for a 12 m/s swing) but must stay material.
#[test]
fn wind_matters() {
    let calm = land(40.0, 1.0, 0.0);
    let tailwind = land(40.0, 1.0, 12.0);
    assert!(
        (tailwind - calm).abs() >= 3.0,
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

/// The reported impact must sit on the terrain surface: linear
/// interpolation of the crossing step leaves no substep-sized penetration
/// bias (was up to 0.14 m before the fix).
#[test]
fn impact_has_no_penetration_bias() {
    use siege::physics::{DT, launch, step};
    use siege::world::{ground_height, player_pivot};

    let (mut pos, mut vel) = launch(player_pivot(), 45.0, 1.0, 1.0);
    while pos.y > ground_height(pos.x) && (-5.0..=205.0).contains(&pos.x) {
        let (p, v) = step(pos, vel, 0.0, DT);
        pos = p;
        vel = v;
    }
    let depth = ground_height(pos.x) - pos.y;
    assert!(depth < 0.08, "raw crossing penetrates {depth:.3} m");
}

/// A defender full-charge 45° shot in calm wind flies past the −5 m edge
/// (muzzle height 30.9 m adds ~25 m of carry): the old `V2` return
/// disguised that as a "landing" at x ≈ −5.
#[test]
fn off_field_is_structural_not_a_sentinel() {
    use siege::physics::Landing;
    use siege::world::{castle_segments, collidables, defender_pivot, ground_height};

    let landing = simulate_landing(
        defender_pivot(),
        45.0,
        1.0,
        -1.0,
        0.0,
        ground_height,
        &collidables(&castle_segments()),
    );
    assert!(
        matches!(landing, Landing::OffField),
        "expected OffField, got {landing:?}"
    );
}
