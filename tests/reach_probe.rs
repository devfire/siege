//! Temporary reachability probe (deleted after diagnosis).
use siege::physics::simulate_landing;
use siege::world::{ground_height, player_pivot};

#[test]
fn reach_envelope() {
    let muzzle = player_pivot();
    for wind in [-14.0_f32, -12.0, -10.0, -8.0, -6.0, -4.0, 0.0, 8.0] {
        let mut best = (0.0_f32, f32::MIN);
        for a in (10..=160).map(|i| i as f32 * 0.5) {
            let x = match simulate_landing(muzzle, a, 1.0, 1.0, wind, ground_height, &[]) {
                siege::physics::Landing::Ground(p) => p.x,
                _ => f32::MIN,
            };
            if x > best.1 {
                best = (a, x);
            }
        }
        println!(
            "wind {wind:+5.1}  best angle {:4.1}°  max impact x = {:6.2}  {}",
            best.0,
            best.1,
            if best.1 >= 148.0 {
                "REACHES castle"
            } else {
                "FALLS SHORT"
            }
        );
    }
}
