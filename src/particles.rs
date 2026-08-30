//! Particle system — smoke, debris, sparks, dust, leaves, flash.
//!
//! Pool capped at 1200 (oldest dropped). Smoke rides the wind and is drawn
//! last (over balls); debris tumbles with gravity, bounces once on the
//! ground, then rests half a second and fades. Leaves are the ambient wind
//! tell, spawning at the upwind field edge.

use crate::physics::V2;
use crate::rng::Rng;
use crate::world;
use macroquad::color::Color;
use macroquad::shapes::{DrawRectangleParams, draw_circle, draw_line, draw_rectangle_ex};

#[derive(Copy, Clone, PartialEq)]
pub enum PKind {
    Smoke,
    Debris,
    Spark,
    Dust,
    Leaf,
    Flash,
}

pub struct Particle {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    /// Debris tumble rate (rad/s); negative marks the single bounce spent,
    /// zero marks a resting (fading-in-place) chunk. Leaves use it as the
    /// flutter frequency.
    pub spin: f32,
    pub kind: PKind,
}

const CAP: usize = 1200;
const LEAF_CAP: usize = 14;
const G: f32 = 9.81;

pub struct Particles {
    pool: Vec<Particle>,
}

impl Default for Particles {
    fn default() -> Self {
        Self::new()
    }
}

impl Particles {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: Vec::with_capacity(256),
        }
    }

    fn push(&mut self, p: Particle) {
        if self.pool.len() >= CAP {
            self.pool.remove(0);
        }
        self.pool.push(p);
    }

    /// Muzzle blast: one flash, 10 fast sparks along the barrel, 14 smoke
    /// puffs that drift with the wind (deterministic fan — no rng).
    pub fn spawn_muzzle(&mut self, muzzle: V2, dir: V2) {
        self.push(Particle {
            pos: muzzle + dir * 0.4,
            vel: dir * 2.0,
            life: 0.08,
            max_life: 0.08,
            size: 2.2,
            spin: 0.0,
            kind: PKind::Flash,
        });
        // Perpendicular for the spark fan.
        let perp = V2 {
            x: -dir.y,
            y: dir.x,
        };
        for i in 0..10u8 {
            let t = f32::from(i) / 9.0; // 0..=1
            let off = (t - 0.5) * 0.9; // fan across the barrel mouth
            let speed = 26.0 - 14.0 * (off * 2.0).abs();
            self.push(Particle {
                pos: muzzle + perp * off,
                vel: dir * speed + perp * (off * 6.0),
                life: 0.22 + 0.1 * (1.0 - off.abs()),
                max_life: 0.32,
                size: 0.16,
                spin: 0.0,
                kind: PKind::Spark,
            });
        }
        for i in 0..14u8 {
            let t = f32::from(i) / 14.0;
            // Deterministic pseudo-jitter from the index.
            let j = f32::from((u16::from(i) * 37) % 11) / 11.0 - 0.5;
            let along = dir * (1.5 + 4.5 * t);
            let side = perp * (j * 1.6);
            self.push(Particle {
                pos: muzzle + along * 0.3 + side,
                vel: along + side * 0.8 + V2 { x: 0.0, y: 0.8 },
                life: 0.7 + 0.8 * t,
                max_life: 1.5,
                size: 0.5 + 0.7 * t,
                spin: 0.0,
                kind: PKind::Smoke,
            });
        }
    }

    /// Impact kit: flash, 16 tumbling debris, 22 smoke, 12 sparks, and a
    /// ground-hugging dust ring.
    pub fn spawn_explosion(&mut self, at: V2, rng: &mut Rng) {
        self.push(Particle {
            pos: at + V2 { x: 0.0, y: 0.3 },
            vel: V2::default(),
            life: 0.1,
            max_life: 0.1,
            size: 3.4,
            spin: 0.0,
            kind: PKind::Flash,
        });
        for _ in 0..16 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let s = rng.range(4.0, 14.0);
            self.push(Particle {
                pos: at,
                vel: V2 {
                    x: a.cos() * s,
                    y: a.sin().abs() * s * 0.8 + 3.0,
                },
                life: rng.range(1.2, 2.2),
                max_life: 2.2,
                size: rng.range(0.22, 0.5),
                spin: rng.range(3.0, 10.0),
                kind: PKind::Debris,
            });
        }
        for _ in 0..22 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let s = rng.range(1.0, 6.0);
            let life = rng.range(0.9, 2.2);
            self.push(Particle {
                pos: at
                    + V2 {
                        x: rng.range(-0.8, 0.8),
                        y: rng.range(-0.4, 0.8),
                    },
                vel: V2 {
                    x: a.cos() * s,
                    y: a.sin().abs() * s * 0.6 + 1.5,
                },
                life,
                max_life: life,
                size: rng.range(0.6, 1.4),
                spin: 0.0,
                kind: PKind::Smoke,
            });
        }
        for _ in 0..12 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let s = rng.range(14.0, 30.0);
            self.push(Particle {
                pos: at,
                vel: V2 {
                    x: a.cos() * s,
                    y: a.sin().abs() * s * 0.5 + 4.0,
                },
                life: rng.range(0.15, 0.35),
                max_life: 0.35,
                size: 0.14,
                spin: 0.0,
                kind: PKind::Spark,
            });
        }
        self.spawn_dust(at);
    }

    /// Ground-hugging dust ring for a ground impact.
    pub fn spawn_dust(&mut self, at: V2) {
        for i in 0..12u16 {
            let t = f32::from(i) / 12.0;
            let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
            let speed = 2.5 + 4.5 * f32::from((i * 29) % 7) / 7.0;
            self.push(Particle {
                pos: at
                    + V2 {
                        x: dir * t * 1.2,
                        y: 0.15,
                    },
                vel: V2 {
                    x: dir * speed,
                    y: 0.6 + 1.4 * t,
                },
                life: 0.55 + 0.35 * t,
                max_life: 0.9,
                size: 0.5 + 0.5 * t,
                spin: 0.0,
                kind: PKind::Dust,
            });
        }
    }

    /// Ambient wind tell: leaves enter from the upwind field edge with
    /// velocity proportional to the wind; capped at ~14 alive.
    pub fn spawn_leaves(&mut self, wind: f32, rng: &mut Rng, dt: f32) {
        let alive = self.pool.iter().filter(|p| p.kind == PKind::Leaf).count();
        if alive >= LEAF_CAP || wind.abs() < 0.5 {
            return;
        }
        // Spawn probability scales with |wind|; ~2/s at 8 m/s.
        if rng.f01() > wind.abs() * 0.25 * dt {
            return;
        }
        let from_left = wind > 0.0;
        let x = if from_left {
            rng.range(-4.0, 2.0)
        } else {
            rng.range(198.0, 204.0)
        };
        self.push(Particle {
            pos: V2 {
                x,
                y: rng.range(2.5, 34.0),
            },
            vel: V2 {
                x: wind * rng.range(0.75, 1.1),
                y: rng.range(-0.5, 0.5),
            },
            life: 30.0, // capped by leaving the field instead
            max_life: 30.0,
            size: rng.range(0.28, 0.5),
            spin: rng.range(3.0, 7.0),
            kind: PKind::Leaf,
        });
    }

    /// Advance all particles. Smoke decelerates, grows, and rides the wind;
    /// debris falls under gravity and bounces once; leaves chase the wind.
    pub fn update(&mut self, dt: f32, wind: f32) {
        for p in &mut self.pool {
            p.life -= dt;
            match p.kind {
                PKind::Flash => {}
                PKind::Smoke => {
                    p.vel = p.vel * (1.0 - 1.6 * dt).max(0.0);
                    p.vel.x += (wind * 0.6 - p.vel.x) * (1.4 * dt).min(1.0);
                    p.vel.y += 1.1 * dt; // buoyancy
                    p.size += 1.1 * dt;
                }
                PKind::Spark => {
                    p.vel.y -= 4.0 * dt;
                    p.vel = p.vel * (1.0 - 2.5 * dt).max(0.0);
                }
                PKind::Dust => {
                    p.vel = p.vel * (1.0 - 2.2 * dt).max(0.0);
                    p.vel.x += (wind * 0.3 - p.vel.x) * (1.0 * dt).min(1.0);
                    p.size += 0.8 * dt;
                }
                PKind::Leaf => {
                    p.vel.x += (wind - p.vel.x) * (2.0 * dt).min(1.0);
                    p.vel.y = ((p.max_life - p.life) * p.spin).sin() * 1.3 - 0.35;
                }
                PKind::Debris => {
                    if p.spin == 0.0 {
                        continue; // resting: fade in place
                    }
                    p.vel.y -= G * dt;
                    let gh = world::ground_height(p.pos.x);
                    if p.pos.y <= gh + 0.12 && p.vel.y < 0.0 {
                        if p.spin > 0.0 {
                            // First contact: the single bounce.
                            p.vel.y = -p.vel.y * 0.3;
                            p.vel.x *= 0.55;
                            p.spin = -p.spin;
                            p.pos.y = gh + 0.12;
                        } else {
                            // Second contact: rest for half a second.
                            p.vel = V2::default();
                            p.spin = 0.0;
                            p.pos.y = gh + 0.1;
                            p.life = 0.5;
                            p.max_life = 0.5;
                        }
                    }
                }
            }
            p.pos = p.pos + p.vel * dt;
        }
        self.pool
            .retain(|p| p.life > 0.0 && p.pos.x > -8.0 && p.pos.x < 208.0 && p.pos.y < 120.0);
    }

    /// Two passes: flash/debris/sparks/dust/leaves first, smoke last so it
    /// hangs over everything at the impact site.
    pub fn draw(&self, to_screen: &dyn Fn(V2) -> (f32, f32), scale: f32) {
        for p in &self.pool {
            if p.kind == PKind::Smoke {
                continue;
            }
            Self::draw_one(p, to_screen, scale);
        }
        for p in &self.pool {
            if p.kind == PKind::Smoke {
                Self::draw_one(p, to_screen, scale);
            }
        }
    }

    fn draw_one(pt: &Particle, to_screen: &dyn Fn(V2) -> (f32, f32), scale: f32) {
        let frac = (pt.life / pt.max_life).clamp(0.0, 1.0);
        let (sx, sy) = to_screen(pt.pos);
        match pt.kind {
            PKind::Flash => {
                let alpha = frac * 0.9;
                draw_circle(sx, sy, pt.size * scale, Color::new(1.0, 0.93, 0.72, alpha));
                draw_circle(
                    sx,
                    sy,
                    pt.size * 0.55 * scale,
                    Color::new(1.0, 1.0, 0.95, alpha),
                );
            }
            PKind::Smoke => {
                let alpha = (frac * frac) * 0.34;
                let gray = 0.36 + 0.1 * frac;
                draw_circle(
                    sx,
                    sy,
                    pt.size * scale,
                    Color::new(gray, gray * 0.96, gray * 0.92, alpha),
                );
            }
            PKind::Spark => {
                let tail = to_screen(pt.pos - pt.vel * 0.045);
                draw_line(
                    sx,
                    sy,
                    tail.0,
                    tail.1,
                    (0.12 * scale).max(1.0),
                    Color::new(1.0, 0.72 + 0.2 * frac, 0.3, frac),
                );
            }
            PKind::Dust => {
                draw_circle(
                    sx,
                    sy,
                    pt.size * scale,
                    Color::new(0.72, 0.64, 0.52, frac * 0.5),
                );
            }
            PKind::Leaf => {
                let flutter = 0.55 + 0.45 * ((pt.max_life - pt.life) * pt.spin * 2.0).sin();
                let leaf_color = if (pt.spin * 7.0) % 2.0 > 1.0 {
                    Color::new(0.55, 0.62, 0.30, 0.85)
                } else {
                    Color::new(0.76, 0.52, 0.24, 0.85)
                };
                draw_circle(sx, sy, pt.size * scale * flutter, leaf_color);
            }
            PKind::Debris => {
                let angle = if pt.spin == 0.0 {
                    0.0
                } else {
                    pt.spin.abs() * (pt.max_life - pt.life)
                };
                let size_px = pt.size * scale;
                draw_rectangle_ex(
                    sx,
                    sy,
                    size_px * 1.5,
                    size_px,
                    DrawRectangleParams {
                        rotation: angle.to_degrees(),
                        color: Color::new(0.42, 0.39, 0.37, frac.min(1.0)),
                        ..Default::default()
                    },
                );
            }
        }
    }
}
