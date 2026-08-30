//! Rendering — letterboxed 200 × 112.5 m world inside the window,
//! storybook-dawn painterly pass. World layers shake with `state.shake`
//! (UI never does); deeper parallax layers shake less.
//!
//! All `as` casts here are bounded screen-space/letterbox arithmetic
//! (world ≤ 200 m, canvas ≤ a few thousand px), so the numeric-loss lints
//! are silenced for the whole module rather than annotated per-site.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use crate::game::{GameState, Phase};
use crate::physics::{self, BALL_R, V2};
use crate::world;
use macroquad::color::Color;
use macroquad::math::{Vec2, vec2};
use macroquad::shapes::{
    DrawRectangleParams, draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_rectangle_ex,
};

use macroquad::window::{screen_height, screen_width};

pub const WORLD_W: f32 = 200.0;
pub const WORLD_H: f32 = 112.5;
const PAGE_BG: Color = Color::from_hex(0x1A_14_23);
pub(super) const SKY_TOP: Color = Color::from_hex(0x2B_1B_4D);
pub(super) const GRASS_LOW: Color = Color::from_hex(0x4C_7A_44);
pub(super) const STONE: Color = Color::from_hex(0xA8_A2_9A);
const IRON: Color = Color::from_hex(0x3A_3F_45);
const CARRIAGE: Color = Color::from_hex(0x6B_45_26);
const WHEEL_C: Color = Color::from_hex(0x4E_34_21);
const BALL_C: Color = Color::from_hex(0x2B_2B_30);

/// Pixels per world metre.
#[must_use]
pub fn scale() -> f32 {
    (screen_width() / WORLD_W).min(screen_height() / WORLD_H)
}

/// Screen-space top-left of the letterboxed world rect.
#[must_use]
pub(super) fn origin() -> (f32, f32) {
    let s = scale();
    (
        (screen_width() - WORLD_W * s) * 0.5,
        (screen_height() - WORLD_H * s) * 0.5,
    )
}

/// World (y-up, metres) → screen (y-down, px). Shake-free (input relies on
/// the exact inverse `screen_to_world`).
#[must_use]
pub fn w2s(p: V2) -> Vec2 {
    let s = scale();
    let (ox, oy) = origin();
    vec2(ox + p.x * s, screen_height() - oy - p.y * s)
}

/// Screen px → world (y-up, metres); inverse of `w2s` (for input).
#[must_use]
pub fn screen_to_world(mx: f32, my: f32) -> V2 {
    let s = scale();
    let (ox, oy) = origin();
    V2 {
        x: (mx - ox) / s,
        y: (screen_height() - my - oy) / s,
    }
}

pub(super) fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub(super) fn darken(c: Color, amt: f32) -> Color {
    mix(c, Color::new(0.0, 0.0, 0.0, c.a), amt)
}

/// Deterministic 0..1 hash of two integers (for bricks, tufts, cracks…).
pub(super) fn hash2(a: u32, b: u32) -> f32 {
    let mut h = a
        .wrapping_mul(374_761_393)
        .wrapping_add(b.wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    let v = (h ^ (h >> 16)) & 0xFFFF;
    #[allow(clippy::cast_possible_truncation)] // masked to 16 bits above
    let wide = v as u16;
    f32::from(wide) / 65_535.0
}

pub fn draw(state: &GameState, font: Option<&macroquad::text::Font>) {
    macroquad::window::clear_background(PAGE_BG);
    let s = scale();
    let (ox, oy) = origin();
    // Screen shake: pseudo-noise direction, world layers only, deeper
    // parallax layers shake less.
    let shk = state.shake * s * 0.45;
    let shake = vec2((state.t * 61.7).sin() * shk, (state.t * 53.3).cos() * shk);
    draw_rectangle(ox, oy, WORLD_W * s, WORLD_H * s, SKY_TOP);
    draw_sky(shake);
    draw_sun(shake * 0.1);
    draw_clouds(state, shake);
    draw_mountains(shake);
    draw_birds(state, shake);
    draw_ground(state, shake);
    draw_castle(state, shake);
    actors::draw(state, shake);
    draw_cannons(state, shake);
    draw_markers(state, shake);
    draw_balls(state, shake);
    draw_aim(state, shake);
    state.particles.draw(
        &|p| {
            let v = w2s(p) + shake;
            (v.x, v.y)
        },
        s,
    );
    draw_hurt(state);
    draw_ui(state, font);
}

mod actors;
mod castle;
mod hud;
mod scenery;

use castle::draw_castle;
use hud::draw_ui;
use scenery::{draw_birds, draw_clouds, draw_ground, draw_mountains, draw_sky, draw_sun};

fn draw_cannons(state: &GameState, shake: Vec2) {
    let s = scale();
    let to_px = |pt: V2| w2s(pt) + shake;
    draw_player_cannon(state, &to_px, s);
    draw_defender_cannon(state, &to_px, s);
}

/// Player cannon: grassy mound, carriage, spoked wheel, iron barrel.
fn draw_player_cannon(state: &GameState, w: &dyn Fn(V2) -> Vec2, s: f32) {
    let pp = world::player_pivot();
    let base = w(V2 {
        x: pp.x,
        y: pp.y - 0.9,
    });
    draw_ellipse(
        base.x,
        base.y + 0.15 * s,
        2.4 * s,
        0.6 * s,
        0.0,
        darken(GRASS_LOW, 0.1),
    );
    let reload_tint = if state.player.reload > 0.0 {
        mix(IRON, Color::new(0.9, 0.5, 0.45, 1.0), 0.45)
    } else {
        IRON
    };
    // Carriage bed + trail.
    draw_rectangle(
        base.x - 1.3 * s,
        base.y - 0.75 * s,
        2.4 * s,
        0.45 * s,
        CARRIAGE,
    );
    draw_rectangle(
        base.x - 1.7 * s,
        base.y - 0.45 * s,
        1.2 * s,
        0.3 * s,
        darken(CARRIAGE, 0.2),
    );
    // Spoked wheel.
    let wheel = vec2(base.x + 0.35 * s, base.y - 0.25 * s);
    draw_circle(wheel.x, wheel.y, 0.68 * s, WHEEL_C);
    draw_circle(wheel.x, wheel.y, 0.52 * s, darken(WHEEL_C, 0.25));
    for k in 0..6u32 {
        let ang = std::f32::consts::TAU * k as f32 / 6.0;
        draw_line(
            wheel.x,
            wheel.y,
            wheel.x + ang.cos() * 0.52 * s,
            wheel.y + ang.sin() * 0.52 * s,
            (0.09 * s).max(1.0),
            WHEEL_C,
        );
    }
    draw_circle(wheel.x, wheel.y, 0.16 * s, darken(WHEEL_C, 0.4));
    // Barrel, kicked back along its axis while `recoil` decays.
    let a = state.player.angle_deg.to_radians();
    let dir = V2 {
        x: a.cos(),
        y: a.sin(),
    };
    let bp = pp - dir * (state.player.recoil * 0.38);
    let pps = w(bp);
    draw_rectangle_ex(
        pps.x,
        pps.y,
        2.9 * s,
        0.52 * s,
        DrawRectangleParams {
            offset: vec2(0.0, 0.5),
            rotation: -a,
            color: reload_tint,
        },
    );
    // Muzzle band.
    let muzzle = w(bp + dir * 2.75);
    draw_circle(muzzle.x, muzzle.y, 0.3 * s, darken(IRON, 0.3));
    // Touch-hole fire while the fuse burns: a swelling three-layer glow
    // that ramps up as the shot nears.
    if state.player.fuse > 0.0 {
        fuse_glow(
            w,
            s,
            bp + V2 { x: 0.0, y: 0.42 },
            state.player.fuse_progress(),
            state.t,
        );
    }
}

/// Swelling three-layer touch-hole fire: wide warm halo, molten core,
/// white-hot center. `k` (0 → 1) is the fuse burn progress.
fn fuse_glow(w: &dyn Fn(V2) -> Vec2, s: f32, at: V2, burn: f32, now: f32) {
    let e = w(at);
    let flick = 0.5 + 0.5 * (now * 30.0).sin();
    draw_circle(
        e.x,
        e.y,
        (0.55 + 0.55 * burn + 0.1 * flick) * s,
        Color::new(1.0, 0.45, 0.12, 0.16 + 0.16 * burn),
    );
    draw_circle(
        e.x,
        e.y,
        (0.2 + 0.16 * burn + 0.05 * flick) * s,
        Color::new(1.0, 0.6, 0.2, 0.95),
    );
    draw_circle(
        e.x,
        e.y,
        (0.09 + 0.06 * burn) * s,
        Color::new(1.0, 0.95, 0.7, 0.95),
    );
}

/// Defender cannon on the keep roof with its crew silhouettes.
fn draw_defender_cannon(state: &GameState, w: &dyn Fn(V2) -> Vec2, s: f32) {
    let dp = w(world::defender_pivot());
    let keep_top = w(V2 {
        x: world::DEFENDER_PIVOT_X,
        y: world::KEEP_TOP,
    });
    let dcol = if state.defender.reload_anim > 0.0 {
        mix(IRON, Color::new(0.9, 0.5, 0.45, 1.0), 0.45)
    } else {
        IRON
    };
    draw_rectangle(
        dp.x - 0.9 * s,
        keep_top.y - 0.55 * s,
        1.8 * s,
        0.5 * s,
        CARRIAGE,
    );
    // Barrel, kicked back along its muzzle axis while `recoil` decays.
    let a = state.defender.display_angle.to_radians();
    let mdir = V2 {
        x: -a.cos(),
        y: a.sin(),
    };
    let bp = world::defender_pivot() - mdir * (state.defender.recoil * 0.3);
    let bps = w(bp);
    draw_rectangle_ex(
        bps.x,
        bps.y,
        2.2 * s,
        0.4 * s,
        DrawRectangleParams {
            offset: vec2(0.0, 0.5),
            rotation: std::f32::consts::PI + a,
            color: dcol,
        },
    );
    // Touch-hole fire while the fuse burns (the keep-side telegraph).
    if state.defender.fuse > 0.0 {
        fuse_glow(
            w,
            s,
            bp + V2 { x: 0.0, y: 0.35 },
            state.defender.fuse_progress(),
            state.t,
        );
    }
    // Crew silhouettes, bobbing while they reload.
    let bob = if state.defender.reload_anim > 0.0 {
        (state.t * 10.0).sin() * 0.05
    } else {
        0.0
    };
    for (dx, hh) in [(1.5_f32, 0.75_f32), (2.3, 0.65)] {
        let cp = w(V2 {
            x: world::DEFENDER_PIVOT_X + dx,
            y: world::KEEP_TOP + hh + bob,
        });
        draw_circle(
            cp.x,
            cp.y - 0.32 * s,
            0.17 * s,
            Color::new(0.12, 0.1, 0.12, 0.9),
        );
        draw_rectangle(
            cp.x - 0.13 * s,
            cp.y - 0.2 * s,
            0.26 * s,
            hh * 0.55 * s,
            Color::new(0.12, 0.1, 0.12, 0.9),
        );
    }
}

#[allow(clippy::cast_precision_loss)] // trail indices/lengths are capped at 24
fn draw_balls(state: &GameState, shake: Vec2) {
    let s = scale();
    for b in &state.balls {
        // Grounding shadow, fading with altitude.
        let gy = world::ground_height(b.pos.x);
        let sa = (1.0 - (b.pos.y - gy) / 45.0).clamp(0.0, 1.0) * 0.22;
        if sa > 0.01 {
            let gp = w2s(V2 {
                x: b.pos.x,
                y: gy + 0.05,
            }) + shake;
            draw_ellipse(
                gp.x,
                gp.y,
                (BALL_R * s * 1.1).max(4.0),
                (BALL_R * s * 0.4).max(1.6),
                0.0,
                Color::new(0.1, 0.08, 0.06, sa),
            );
        }
        for (i, tp) in b.trail.iter().enumerate() {
            let f = (i + 1) as f32 / b.trail.len().max(1) as f32;
            let v = w2s(*tp) + shake;
            draw_circle(
                v.x,
                v.y,
                (BALL_R * s * 0.6).max(1.8),
                Color::new(0.16, 0.15, 0.18, 0.4 * f),
            );
        }
        let v = w2s(b.pos) + shake;
        // Visual radius floors keep the ball readable at small window scales.
        let r = (BALL_R * s).max(3.5);
        draw_circle(v.x, v.y, r, BALL_C);
        draw_circle(
            v.x - r * 0.3,
            v.y - r * 0.32,
            r * 0.28,
            Color::new(1.0, 1.0, 1.0, 0.7),
        );
    }
}

/// 12 dashes over the first 18 m of the vacuum arc from the muzzle.
fn draw_aim(state: &GameState, shake: Vec2) {
    if state.phase != Phase::Playing {
        return;
    }
    let a = state.player.angle_deg.to_radians();
    let dir = V2 {
        x: a.cos(),
        y: a.sin(),
    };
    let pivot = world::player_pivot();
    let start = pivot + dir * physics::BARREL_LEN;
    let charge = state.player.charge;
    let v0 = (charge * physics::MUZZLE_V_MAX).max(1.0);
    let wind = state.wind.current();
    // Integrate the real ballistic model (drag + current wind) for the
    // first 18 m — the guide must match the flight, not a vacuum parabola.
    let t_end = 18.0 / v0;
    let steps = 96_u16;
    let dt = t_end / f32::from(steps);
    let (mut p, mut v) = (start, dir * v0);
    let mut prev = p;
    for i in 0..=steps {
        if i > 0 {
            let (np, nv) = physics::step(p, v, wind, dt);
            prev = p;
            p = np;
            v = nv;
        }
        // One dash per half-metre of arc; dash centered on the sample.
        if i % 8 == 0 {
            let s0 = w2s(prev) + shake;
            let s1 = w2s(p) + shake;
            draw_line(s0.x, s0.y, s1.x, s1.y, 2.0, Color::new(1.0, 1.0, 0.95, 0.4));
        }
    }
}

fn draw_markers(state: &GameState, shake: Vec2) {
    let s = scale();
    for &(at, side) in &state.markers {
        let v = w2s(at) + shake;
        let c = if side == physics::Side::Player {
            Color::new(1.0, 0.95, 0.8, 0.9)
        } else {
            Color::new(0.95, 0.35, 0.28, 0.9)
        };
        let r = 0.5 * s;
        draw_line(v.x - r, v.y - r, v.x + r, v.y + r, 2.0, c);
        draw_line(v.x - r, v.y + r, v.x + r, v.y - r, 2.0, c);
    }
}

fn draw_hurt(state: &GameState) {
    if state.hurt <= 0.0 {
        return;
    }
    let (w, h) = (screen_width(), screen_height());
    let a = state.hurt;
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.75, 0.1, 0.08, a * 0.14));
    let band = 70.0;
    let edge = Color::new(0.8, 0.08, 0.05, a * 0.4);
    draw_rectangle(0.0, 0.0, w, band, edge);
    draw_rectangle(0.0, h - band, w, band, edge);
    draw_rectangle(0.0, 0.0, band, h, edge);
    draw_rectangle(w - band, 0.0, band, h, edge);
}
