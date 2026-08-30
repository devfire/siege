//! Ballistic core — macroquad-free so it unit-tests natively.
//!
//! Own `V2` math type keeps the simulation (and its tests) independent of the
//! rendering layer. Ballistics: gravity plus quadratic air drag acting on the
//! wind-relative velocity.

use std::collections::VecDeque;
use std::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Copy, Clone, Default, Debug, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

impl V2 {
    #[must_use]
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[must_use]
    pub fn normalized(self) -> V2 {
        let l = self.length();
        if l > 1e-9 {
            V2 {
                x: self.x / l,
                y: self.y / l,
            }
        } else {
            V2::default()
        }
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl SubAssign for V2 {
    fn sub_assign(&mut self, o: V2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

impl MulAssign<f32> for V2 {
    fn mul_assign(&mut self, s: f32) {
        self.x *= s;
        self.y *= s;
    }
}
impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2 {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        V2 {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, s: f32) -> V2 {
        V2 {
            x: self.x * s,
            y: self.y * s,
        }
    }
}

pub const G: f32 = 9.81; // m/s²
/// Quadratic drag coefficient (1/m), tuned so BOTH tuning contracts hold
/// simultaneously: the `ballistics` bands (bands 1/2 relaxed per the plan's
/// contingency) and the defender's reachability — from the keep top
/// (171, 30.9) a full-charge ~31° shot must still reach the player cannon
/// against a +8 m/s headwind (`ai_convergence`). 0.006 satisfied the bands
/// but left the defender ~30 m short in any headwind; 0.004 is the largest
/// value that keeps the duel fair for both sides.
pub const K_DRAG: f32 = 0.0040;
pub const DT: f32 = 1.0 / 240.0; // game substep
pub const SIM_DT: f32 = 1.0 / 120.0; // simulate_landing substep
pub const BALL_R: f32 = 0.35; // m, visual/physical ball radius
pub const MUZZLE_V_MAX: f32 = 52.0; // m/s at 100% charge
pub const BARREL_LEN: f32 = 2.6; // m, ball spawn offset along aim direction
pub const TRAIL_CAP: usize = 24;

#[derive(Copy, Clone, PartialEq)]
pub enum Side {
    Player,
    Defender,
}

/// `a = -g ĵ + K_DRAG * |v_rel| * (-v_rel)`, `v_rel = v - (wind, 0)`.
#[must_use]
pub fn accel(vel: V2, wind: f32) -> V2 {
    let v_rel = V2 {
        x: vel.x - wind,
        y: vel.y,
    };
    let s = v_rel.length();
    V2 {
        x: -K_DRAG * s * v_rel.x,
        y: -G - K_DRAG * s * v_rel.y,
    }
}

/// Semi-implicit Euler substep: returns `(new_pos, new_vel)`.
#[must_use]
pub fn step(pos: V2, vel: V2, wind: f32, dt: f32) -> (V2, V2) {
    let a = accel(vel, wind);
    let vel = vel + a * dt;
    (pos + vel * dt, vel)
}

/// Fire from `muzzle`: ball spawns `BARREL_LEN` further along the aim
/// direction so it never overlaps the firing cannon's own collision box.
/// `angle_deg` = elevation above horizontal; `dir_x` = `+1` (player, fires
/// right) or `-1` (defender, fires left); `charge ∈ [0,1]`.
#[must_use]
pub fn launch(muzzle: V2, angle_deg: f32, charge: f32, dir_x: f32) -> (V2, V2) {
    let a = angle_deg.to_radians();
    let dir = V2 {
        x: dir_x * a.cos(),
        y: a.sin(),
    };
    let charge = charge.clamp(0.0, 1.0);
    (muzzle + dir * BARREL_LEN, dir * (charge * MUZZLE_V_MAX))
}

#[derive(Clone)]
pub struct Ball {
    pub pos: V2,
    pub vel: V2,
    pub side: Side,
    pub trail: VecDeque<V2>,
}

impl Ball {
    /// Ring-buffer record: `pop_front` is O(1); `Vec::remove(0)` was an
    /// O(N) memmove on every step once the cap was reached.
    pub fn push_trail(&mut self, p: V2) {
        if self.trail.len() >= TRAIL_CAP {
            self.trail.pop_front();
        }
        self.trail.push_back(p);
    }
}

/// Integrate at `SIM_DT` under `wind` until `y <= ground(x)` or the ball
/// leaves `[-5, 205]`; returns the impact point.
#[must_use]
pub fn simulate_landing(
    muzzle: V2,
    angle_deg: f32,
    charge: f32,
    dir_x: f32,
    wind: f32,
    ground: &dyn Fn(f32) -> f32,
) -> V2 {
    let (mut pos, mut vel) = launch(muzzle, angle_deg, charge, dir_x);
    while pos.y > ground(pos.x) && (-5.0..=205.0).contains(&pos.x) {
        let (p, v) = step(pos, vel, wind, SIM_DT);
        pos = p;
        vel = v;
    }
    pos
}
