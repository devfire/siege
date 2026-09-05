//! Bounded smoke, fire, shockwaves, debris, sparks, dust, and wind leaves.
//!
//! Cool particles sit behind luminous fire and spark trails. All effect
//! animation follows particle age, so pausing freezes every layer together.
//! New blast details use deterministic index patterns, never gameplay RNG.

use crate::physics::V2;
use crate::rng::Rng;
use crate::world;
use macroquad::color::Color;
use macroquad::math::vec2;
use macroquad::shapes::{
    DrawRectangleParams, draw_circle, draw_line, draw_rectangle_ex, draw_triangle,
};
use std::collections::VecDeque;

#[derive(Copy, Clone, PartialEq)]
pub enum PKind {
    Smoke,
    Debris,
    Spark,
    Dust,
    Leaf,
    Flash,
    Fireball,
    Shockwave,
    Muzzle,
}

pub struct Particle {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    /// Debris tumble rate (rad/s); negative marks the single bounce spent,
    /// zero marks a resting (fading-in-place) chunk. Leaves use it as the
    /// flutter frequency; fire uses a lobe phase, muzzle blasts an angle.
    pub spin: f32,
    pub kind: PKind,
}

const CAP: usize = 1200;
const LEAF_CAP: usize = 14;
const G: f32 = 9.81;

pub struct Particles {
    pool: VecDeque<Particle>,
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
            pool: VecDeque::with_capacity(CAP),
        }
    }

    /// Ring-buffer eviction: `pop_front` is O(1); `Vec::remove(0)` memmoved
    /// up to 1199 particles per spawn when the pool was full.
    fn push(&mut self, p: Particle) {
        if self.pool.len() >= CAP {
            self.pool.pop_front();
        }
        self.pool.push_back(p);
    }

    /// Directional flame, sparks, and drifting powder smoke; no RNG draws.
    pub fn spawn_muzzle(&mut self, muzzle: V2, dir: V2) {
        self.push(Particle {
            pos: muzzle + dir * 0.4,
            vel: dir * 2.0,
            life: 0.32,
            max_life: 0.32,
            size: 5.4,
            spin: dir.y.atan2(dir.x),
            kind: PKind::Muzzle,
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
                life: 0.42 + 0.1 * (1.0 - off.abs()),
                max_life: 0.52,
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

    /// Touch-hole prime puff: fat smoke wisps and popping embers while a
    /// fuse burns. Called at ~30 Hz; `k` (0 → 1) is the burn progress —
    /// the closer to firing, the angrier the vent.
    pub fn spawn_prime_puff(&mut self, at: V2, k: f32) {
        self.push(Particle {
            pos: at,
            vel: V2 { x: 0.15, y: 1.6 },
            life: 0.6,
            max_life: 0.6,
            size: 0.24 + 0.2 * k,
            spin: 0.0,
            kind: PKind::Smoke,
        });
        for i in 0..2u8 {
            let side = if i == 0 { 1.0 } else { -1.0 };
            self.push(Particle {
                pos: at,
                vel: V2 {
                    x: side * (0.6 + k),
                    y: 2.0 + 1.2 * k,
                },
                life: 0.2,
                max_life: 0.2,
                size: 0.12,
                spin: 0.0,
                kind: PKind::Spark,
            });
        }
    }

    /// Hot core and pressure ring, then rolling fire, charcoal, and embers.
    /// Keep the original debris/smoke/spark RNG calls in their original order.
    pub fn spawn_explosion(&mut self, at: V2, rng: &mut Rng) {
        self.push(Particle {
            pos: at + V2 { x: 0.0, y: 0.3 },
            vel: V2::default(),
            life: 0.3,
            max_life: 0.3,
            size: 2.7,
            spin: 0.0,
            kind: PKind::Flash,
        });
        self.push(Particle {
            pos: at,
            vel: V2::default(),
            life: 0.58,
            max_life: 0.58,
            size: 8.0,
            spin: 0.0,
            kind: PKind::Shockwave,
        });
        for i in 0..9u16 {
            let phase = f32::from(i) * 2.399_963;
            let spread = 1.0 + f32::from((i * 7) % 5) * 0.24;
            let life = 1.8 + f32::from((i * 11) % 7) * 0.12;
            self.push(Particle {
                pos: at
                    + V2 {
                        x: phase.cos() * 0.7,
                        y: 0.4 + phase.sin() * 0.45,
                    },
                vel: V2 {
                    x: phase.cos() * spread * 3.2,
                    y: 2.0 + phase.sin().abs() * 3.6,
                },
                life,
                max_life: life,
                size: 0.95 + spread * 0.45,
                spin: phase,
                kind: PKind::Fireball,
            });
        }
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
                    y: a.sin() * s * 0.7 + 3.0,
                },
                life: rng.range(0.15, 0.35) + 0.4,
                max_life: 0.75,
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
                PKind::Flash | PKind::Shockwave | PKind::Muzzle => {}
                PKind::Fireball => {
                    p.vel *= (1.0 - 1.5 * dt).max(0.0);
                    p.vel.x += (wind * 0.45 - p.vel.x) * (0.7 * dt).min(1.0);
                    p.vel.y += 1.8 * dt;
                    p.size += 0.85 * dt;
                }
                PKind::Smoke => {
                    p.vel *= (1.0 - 1.6 * dt).max(0.0);
                    p.vel.x += (wind * 0.6 - p.vel.x) * (1.4 * dt).min(1.0);
                    p.vel.y += 1.1 * dt; // buoyancy
                    p.size += 1.1 * dt;
                }
                PKind::Spark => {
                    p.vel.y -= 4.0 * dt;
                    p.vel *= (1.0 - 2.5 * dt).max(0.0);
                }
                PKind::Dust => {
                    p.vel *= (1.0 - 2.2 * dt).max(0.0);
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
            p.pos += p.vel * dt;
        }
        self.pool
            .retain(|p| p.life > 0.0 && p.pos.x > -8.0 && p.pos.x < 208.0 && p.pos.y < 120.0);
    }

    /// Dust/smoke, fragments/rings, fire, then hot cores/sparks. No sorting or
    /// temporary buffers: even a full pool takes only four bounded passes.
    pub fn draw(&self, to_screen: &dyn Fn(V2) -> (f32, f32), scale: f32) {
        for layer in 0..4 {
            for p in &self.pool {
                let particle_layer = match p.kind {
                    PKind::Smoke | PKind::Dust => 0,
                    PKind::Fireball if p.max_life - p.life >= 0.85 => 0,
                    PKind::Debris | PKind::Leaf | PKind::Shockwave => 1,
                    PKind::Fireball | PKind::Muzzle => 2,
                    PKind::Flash | PKind::Spark => 3,
                };
                if particle_layer == layer {
                    Self::draw_one(p, to_screen, scale);
                }
            }
        }
    }

    fn draw_one(pt: &Particle, to_screen: &dyn Fn(V2) -> (f32, f32), scale: f32) {
        let frac = (pt.life / pt.max_life).clamp(0.0, 1.0);
        let age = pt.max_life - pt.life;
        let (sx, sy) = to_screen(pt.pos);
        match pt.kind {
            PKind::Flash => {
                let alpha = frac * frac;
                let radius = pt.size * scale * (1.0 + (1.0 - frac) * 0.35);
                draw_circle(
                    sx,
                    sy,
                    radius * 1.6,
                    Color::new(1.0, 0.38, 0.08, alpha * 0.13),
                );
                draw_circle(sx, sy, radius, Color::new(1.0, 0.76, 0.3, alpha * 0.8));
                draw_circle(sx, sy, radius * 0.55, Color::new(1.0, 1.0, 0.95, alpha));
            }
            PKind::Shockwave => Self::draw_shockwave(pt, to_screen, scale, frac),
            PKind::Muzzle => Self::draw_muzzle(pt, to_screen, scale, frac, (sx, sy)),
            PKind::Fireball => Self::draw_fireball(pt, scale, frac, age, (sx, sy)),
            PKind::Smoke => {
                let alpha = (frac * frac) * 0.22;
                let gray = 0.29 + 0.12 * frac;
                draw_circle(
                    sx,
                    sy,
                    pt.size * scale,
                    Color::new(gray, gray * 0.96, gray * 0.92, alpha),
                );
                let roll = age * 0.9 + pt.pos.x * 0.7;
                draw_circle(
                    sx + roll.cos() * pt.size * scale * 0.3,
                    sy - pt.size * scale * 0.22,
                    pt.size * scale * 0.68,
                    Color::new(gray + 0.09, gray + 0.07, gray + 0.04, alpha * 0.75),
                );
            }
            PKind::Spark => Self::draw_spark(pt, to_screen, scale, frac, (sx, sy)),
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
                        rotation: angle,
                        color: Color::new(0.42, 0.39, 0.37, frac.min(1.0)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_shockwave(pt: &Particle, to_screen: &dyn Fn(V2) -> (f32, f32), scale: f32, frac: f32) {
        let progress = 1.0 - frac;
        let radius = pt.size * (1.0 - frac * frac);
        for i in 0..32u16 {
            let angle = f32::from(i) * std::f32::consts::TAU / 32.0;
            let end_angle = angle + std::f32::consts::TAU / 32.0 * 0.84;
            let start = to_screen(
                pt.pos
                    + V2 {
                        x: angle.cos() * radius,
                        y: angle.sin() * radius * 0.72,
                    },
            );
            let end = to_screen(
                pt.pos
                    + V2 {
                        x: end_angle.cos() * radius,
                        y: end_angle.sin() * radius * 0.72,
                    },
            );
            draw_line(
                start.0,
                start.1,
                end.0,
                end.1,
                ((0.16 - progress * 0.09) * scale).max(0.7),
                Color::new(1.0, 0.79, 0.47, frac * frac * 0.65),
            );
        }
    }

    fn draw_muzzle(
        pt: &Particle,
        to_screen: &dyn Fn(V2) -> (f32, f32),
        scale: f32,
        frac: f32,
        (sx, sy): (f32, f32),
    ) {
        let dir = V2 {
            x: pt.spin.cos(),
            y: pt.spin.sin(),
        };
        let perp = V2 {
            x: -dir.y,
            y: dir.x,
        };
        let reach = pt.size * (0.65 + 0.35 * (1.0 - frac));
        let width = 0.8 * frac + 0.15;
        for i in 0..3u8 {
            let side = f32::from(i) - 1.0;
            let tip =
                to_screen(pt.pos + dir * (reach * (1.0 - side.abs() * 0.25)) + perp * (side * 0.9));
            let left = to_screen(pt.pos + perp * (width + side * 0.25));
            let right = to_screen(pt.pos - perp * (width - side * 0.25));
            draw_triangle(
                vec2(left.0, left.1),
                vec2(right.0, right.1),
                vec2(tip.0, tip.1),
                Color::new(1.0, 0.42 + 0.15 * frac, 0.06, frac * 0.62),
            );
        }
        let tip = to_screen(pt.pos + dir * (reach * 0.7));
        let left = to_screen(pt.pos + perp * width * 0.5);
        let right = to_screen(pt.pos - perp * width * 0.5);
        draw_triangle(
            vec2(left.0, left.1),
            vec2(right.0, right.1),
            vec2(tip.0, tip.1),
            Color::new(1.0, 0.96, 0.68, frac),
        );
        draw_circle(
            sx,
            sy,
            width * scale * 0.7,
            Color::new(1.0, 0.98, 0.8, frac),
        );
    }

    fn draw_fireball(pt: &Particle, scale: f32, frac: f32, age: f32, (sx, sy): (f32, f32)) {
        let heat = (1.0 - age / 0.85).clamp(0.0, 1.0);
        let fade = (frac * 2.4).min(1.0);
        let swell = (age * 8.0).min(1.0);
        let radius = pt.size * scale * (0.45 + 0.55 * swell);
        for i in 0..3u8 {
            let angle = pt.spin + f32::from(i) * 2.094_395 + age * 0.65;
            let x = sx + angle.cos() * radius * 0.38;
            let y = sy + angle.sin() * radius * 0.3;
            let lobe = radius * (0.65 + f32::from(i) * 0.09);
            draw_circle(
                x,
                y,
                lobe,
                Color::new(0.19, 0.18, 0.17, fade * (0.19 + (1.0 - heat) * 0.15)),
            );
            if heat > 0.0 {
                draw_circle(
                    x,
                    y,
                    lobe * 0.87,
                    Color::new(1.0, 0.26 + heat * 0.32, 0.035, heat * 0.82),
                );
                draw_circle(
                    x - lobe * 0.12,
                    y + lobe * 0.15,
                    lobe * 0.5,
                    Color::new(
                        1.0,
                        0.69 + heat * 0.23,
                        0.23 + heat * 0.4,
                        heat * heat * 0.85,
                    ),
                );
            }
        }
    }

    fn draw_spark(
        pt: &Particle,
        to_screen: &dyn Fn(V2) -> (f32, f32),
        scale: f32,
        frac: f32,
        (sx, sy): (f32, f32),
    ) {
        let tail = to_screen(pt.pos - pt.vel * 0.085);
        draw_line(
            sx,
            sy,
            tail.0,
            tail.1,
            (pt.size * scale * 3.0).max(2.0),
            Color::new(1.0, 0.29, 0.035, frac * 0.22),
        );
        draw_line(
            sx,
            sy,
            tail.0,
            tail.1,
            (pt.size * scale).max(1.0),
            Color::new(1.0, 0.64 + 0.3 * frac, 0.22 + 0.5 * frac, frac),
        );
        draw_circle(
            sx,
            sy,
            (pt.size * scale * 0.65).max(0.7),
            Color::new(1.0, 0.97, 0.7, frac),
        );
    }
}
