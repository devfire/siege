//! Ballistic core — macroquad-free so it unit-tests natively.
//!
//! Own `V2` math type keeps the simulation (and its tests) independent of the
//! rendering layer. Ballistics: gravity plus quadratic air drag acting on the
//! wind-relative velocity.

use std::collections::VecDeque;
use std::f32::consts::PI;
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

/// Projectile: a 12-pounder iron shot. The drag constant is *derived* from
/// the projectile, not hand-tuned — a balance knob documented as physics is
/// how `K_DRAG` drifted 47× off (issue #6).
const SHOT_R: f32 = 0.06; // m
const SHOT_M: f32 = 5.44; // kg
const C_D: f32 = 0.47; // sphere
const RHO_AIR: f32 = 1.225; // kg/m³
/// Quadratic-drag constant for the iron shot above, in 1/m:
/// `rho * C_d * A / (2 * m)`.
pub const K_DRAG: f32 = RHO_AIR * C_D * PI * SHOT_R * SHOT_R / (2.0 * SHOT_M);
pub const DT: f32 = 1.0 / 240.0; // game substep
/// Collision/render radius — deliberately larger than `SHOT_R` so the ball
/// is visible across the 200 m field. A readability choice, not physics.
pub const BALL_R: f32 = 0.35;
pub const MUZZLE_V_MAX: f32 = 40.0; // m/s at 100% charge — sized so a full-power 40° shot lands on the keep (≈170 m) under the derived K_DRAG
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

/// A collidable rectangle (`world::Segment` live rect or rubble mound) the
/// planner's model must respect. Kept in `physics` so the simulation core
/// stays macroquad-free and `world` may depend on it, never the reverse.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Obstacle {
    pub x0: f32,
    pub y0: f32,
    pub w: f32,
    pub h: f32,
}

/// Circle-vs-rect contact point (nearest point on the rect to `p`).
#[must_use]
pub fn hit(p: V2, r: f32, o: &Obstacle) -> Option<V2> {
    let cx = p.x.clamp(o.x0, o.x0 + o.w);
    let cy = p.y.clamp(o.y0, o.y0 + o.h);
    let dx = p.x - cx;
    let dy = p.y - cy;
    (dx * dx + dy * dy <= r * r).then_some(V2 { x: cx, y: cy })
}

/// Where a simulated shot ended up. A coordinate alone cannot express "no
/// landing"; callers used to read the off-field sentinel `x ≈ ±5` as data.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Landing {
    /// Ball met the ground inside the field at this point.
    Ground(V2),
    /// Ball struck a live segment or rubble mound at this point.
    Obstacle(V2),
    /// Ball left the playfield; no impact point exists.
    OffField,
}

/// Integrate at [`DT`] under `wind` until ground or obstacle contact, or
/// the ball leaves `[-5, 205]`. The final ground step is linearly
/// interpolated against `ground`, so the reported impact carries no
/// substep-sized penetration bias.
#[must_use]
pub fn simulate_landing(
    muzzle: V2,
    angle_deg: f32,
    charge: f32,
    dir_x: f32,
    wind: f32,
    ground: impl Fn(f32) -> f32,
    obstacles: &[Obstacle],
) -> Landing {
    let (mut pos, mut vel) = launch(muzzle, angle_deg, charge, dir_x);
    let mut prev = pos;
    while (-5.0..=205.0).contains(&pos.x) && pos.y > ground(pos.x) {
        let (p, v) = step(pos, vel, wind, DT);
        for o in obstacles {
            // Cheap x-range prefilter: the ball spends most of its flight
            // far from the obstacle cluster; skip before the clamp math.
            if p.x < o.x0 - BALL_R || p.x > o.x0 + o.w + BALL_R {
                continue;
            }
            if let Some(c) = hit(p, BALL_R, o) {
                return Landing::Obstacle(c);
            }
        }
        prev = pos;
        pos = p;
        vel = v;
    }
    if !(-5.0..=205.0).contains(&pos.x) {
        return Landing::OffField;
    }
    // Linear interpolation of the crossing step: the true impact lies
    // between `prev` (above ground) and `pos` (below it).
    let h_prev = prev.y - ground(prev.x);
    let h_pos = pos.y - ground(pos.x);
    let denom = h_prev - h_pos;
    let t = if denom > f32::EPSILON {
        (h_prev / denom).clamp(0.0, 1.0)
    } else {
        0.0 // degenerate first iteration: no crossing to interpolate
    };
    Landing::Ground(prev + (pos - prev) * t)
}
