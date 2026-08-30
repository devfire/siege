//! Flung wall defenders — when a tower or curtain collapses, its runners
//! are thrown from the parapet slots they occupied that frame
//! (`world::runner_state`), tumble under gravity, bounce once, settle
//! flat, and fade. Sim twin of the runner drawing in `render::actors`.

use crate::physics::{G, V2};
use crate::rng::Rng;
use crate::world::{self, Segment};

const CAP: usize = 32;

/// One tumbling body. `spin == 0.0` marks the rested state.
pub struct Faller {
    pub pos: V2,
    pub vel: V2,
    /// Body axis angle (rad).
    pub ang: f32,
    /// Tumble rate (rad/s).
    pub spin: f32,
    /// Bounce spent?
    pub bounced: bool,
    /// Seconds since coming to rest (drives the settle ease).
    pub rest_t: f32,
    /// Seconds left before fading out.
    pub life: f32,
}

pub struct Fallers {
    pub list: Vec<Faller>,
}

impl Default for Fallers {
    fn default() -> Self {
        Self::new()
    }
}

impl Fallers {
    #[must_use]
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    fn push(&mut self, f: Faller) {
        if self.list.len() >= CAP {
            self.list.remove(0);
        }
        self.list.push(f);
    }

    /// Throw every runner standing on `seg` (index `ix`) as it dies.
    /// `at` is the impact point; `t` the sim time — spawn positions match
    /// the drawn runners exactly.
    pub fn fling_wall(&mut self, seg: &Segment, ix: usize, at: V2, t: f32, rng: &mut Rng) {
        let top = seg.y0 + seg.h;
        for k in 0..world::runner_count(seg) {
            let (rx, dir, _) = world::runner_state(seg, ix, k, t);
            // Fling away from the impact; a runner right under it just
            // follows their facing.
            let away = if (rx - at.x).abs() < 0.3 {
                dir
            } else {
                (rx - at.x).signum()
            };
            self.push(Faller {
                pos: V2 { x: rx, y: top },
                vel: V2 {
                    x: away * rng.range(2.5, 6.5),
                    y: rng.range(4.5, 9.0),
                },
                ang: rng.range(0.0, std::f32::consts::TAU),
                spin: away * rng.range(5.0, 11.0),
                bounced: false,
                rest_t: 0.0,
                life: 2.8,
            });
        }
    }

    /// Tumble under gravity, bounce once on the terrain, rest, fade.
    pub fn update(&mut self, dt: f32) {
        for f in &mut self.list {
            f.life -= dt;
            if f.spin == 0.0 {
                f.rest_t += dt;
                continue; // resting: settle + fade in place
            }
            f.vel.y -= G * dt;
            f.ang += f.spin * dt;
            let gh = world::ground_height(f.pos.x) + 0.12;
            if f.pos.y <= gh && f.vel.y < 0.0 {
                if f.bounced {
                    // Second contact: rest (settle + fade from here on).
                    f.vel = V2::default();
                    f.spin = 0.0;
                    f.pos.y = gh - 0.02;
                } else {
                    // First contact: the single bounce.
                    f.bounced = true;
                    f.vel.y = -f.vel.y * 0.25;
                    f.vel.x *= 0.5;
                    f.spin *= 0.4;
                    f.pos.y = gh;
                }
            }
            f.pos += f.vel * dt;
        }
        self.list.retain(|f| f.life > 0.0);
    }
}
