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

// Hash moved to `world` so the sim (runner slots, fling spawns) and the
// renderer share one deterministic layout.
pub(super) use crate::world::hash2;

// The platform layer hit-tests the mute button against the same rect the
// HUD paints.
pub(crate) use hud::mute_button_rect;

pub const WORLD_W: f32 = 200.0;
pub const WORLD_H: f32 = 112.5;
// Leave room below the battlefield for the charge HUD and foreground soil.
const GROUND_MARGIN: f32 = 9.0;
const PAGE_BG: Color = Color::from_hex(0x1A_14_23);
pub(super) const SKY_TOP: Color = Color::from_hex(0x1D_30_4C);
pub(super) const GRASS_LOW: Color = Color::from_hex(0x4C_7A_44);
pub(super) const STONE: Color = Color::from_hex(0xA8_A2_9A);
const IRON: Color = Color::from_hex(0x3A_3F_45);
const CARRIAGE: Color = Color::from_hex(0xA5_65_35);
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
    vec2(
        ox + p.x * s,
        screen_height() - oy - (p.y + GROUND_MARGIN) * s,
    )
}

/// Screen px → world (y-up, metres); inverse of `w2s` (for input).
#[must_use]
pub fn screen_to_world(mx: f32, my: f32) -> V2 {
    let s = scale();
    let (ox, oy) = origin();
    V2 {
        x: (mx - ox) / s,
        y: (screen_height() - my - oy) / s - GROUND_MARGIN,
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

pub fn draw(state: &GameState, muted: bool, font: Option<&macroquad::text::Font>) {
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
    draw_god_rays(state, shake * 0.1);
    draw_clouds(state, shake);
    draw_mountains(shake);
    draw_birds(state, shake);
    draw_ground(state, shake);
    draw_castle(state, shake);
    draw_cannons(state, shake);
    actors::draw(state, shake);
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
    // Restore the letterbox after world effects and camera shake spill outside it.
    draw_rectangle(0.0, 0.0, screen_width(), oy, PAGE_BG);
    draw_rectangle(0.0, screen_height() - oy, screen_width(), oy, PAGE_BG);
    draw_rectangle(0.0, 0.0, ox, screen_height(), PAGE_BG);
    draw_rectangle(screen_width() - ox, 0.0, ox, screen_height(), PAGE_BG);
    draw_vignette();
    draw_hurt(state);
    draw_ui(state, muted, font);
}

mod actors;
mod castle;
mod hud;
mod scenery;

use castle::draw_castle;
use hud::draw_ui;
use scenery::{
    draw_birds, draw_clouds, draw_god_rays, draw_ground, draw_mountains, draw_sky, draw_sun,
    draw_vignette,
};

fn draw_cannons(state: &GameState, shake: Vec2) {
    let s = scale();
    let to_px = |pt: V2| w2s(pt) + shake;
    draw_player_cannon(state, &to_px, s);
    draw_defender_cannon(state, &to_px, s);
}

/// Heavy oak carriage and bronze-banded iron, with a readable recoil stroke.
fn draw_player_cannon(state: &GameState, w: &dyn Fn(V2) -> Vec2, s: f32) {
    let pivot = world::player_pivot();
    let a = state.player.angle_deg.to_radians();
    let dir = V2 {
        x: a.cos(),
        y: a.sin(),
    };
    cannon_carriage(w, s, pivot, 1.0, state.player.recoil);
    let bp = pivot - dir * (state.player.recoil * 0.38);
    barrel_cylinder(w, s, bp, dir, state.player.recoil);
    if state.player.fuse > 0.0 {
        fuse_glow(
            w,
            s,
            bp + V2 { x: 0.0, y: 0.42 },
            state.player.fuse_progress(),
            state.t,
        );
    }
    // A small iron shot pyramid at the gunner's feet.
    for (dx, dy) in [(-4.8, 0.2), (-4.2, 0.2), (-4.5, 0.7)] {
        let p = w(V2 {
            x: pivot.x + dx,
            y: world::ground_height(pivot.x + dx) + dy,
        });
        draw_circle(p.x, p.y, 0.28 * s, IRON);
        draw_circle(
            p.x - 0.07 * s,
            p.y - 0.08 * s,
            0.09 * s,
            Color::from_hex(0xB5_B4_9E),
        );
    }
}

/// Broad oak trail, metal fittings and iron-rimmed wheels, grounded at the pivot.
fn cannon_carriage(w: &dyn Fn(V2) -> Vec2, s: f32, pivot: V2, facing: f32, recoil: f32) {
    let floor = pivot.y - 0.9;
    let base = w(V2 {
        x: pivot.x - facing * 0.65,
        y: floor,
    });
    draw_ellipse(
        base.x,
        base.y,
        3.0 * s,
        0.4 * s,
        0.0,
        Color::new(0.07, 0.10, 0.12, 0.32),
    );
    let rear = w(V2 {
        x: pivot.x - facing * 2.8,
        y: floor + 0.3,
    });
    let axle = w(V2 {
        x: pivot.x + facing * 0.15 - facing * recoil * 0.12,
        y: floor + 0.95,
    });
    draw_line(
        rear.x,
        rear.y,
        axle.x,
        axle.y,
        0.65 * s,
        darken(CARRIAGE, 0.4),
    );
    draw_line(
        rear.x,
        rear.y - 0.16 * s,
        axle.x,
        axle.y - 0.16 * s,
        0.18 * s,
        CARRIAGE,
    );
    draw_rectangle(
        axle.x - 1.6 * s,
        axle.y - 0.22 * s,
        2.7 * s,
        0.48 * s,
        CARRIAGE,
    );
    for dx in [-1.3, 0.7] {
        draw_rectangle(axle.x + dx * s, axle.y - 0.25 * s, 0.18 * s, 0.54 * s, IRON);
        draw_circle(
            axle.x + (dx + 0.09) * s,
            axle.y,
            0.07 * s,
            Color::from_hex(0xDF_C1_7F),
        );
    }
    let radius = 1.02 * s;
    draw_circle(axle.x, axle.y, radius, Color::from_hex(0x25_2D_31));
    draw_circle(axle.x, axle.y, radius * 0.86, CARRIAGE);
    draw_circle(axle.x, axle.y, radius * 0.66, WHEEL_C);
    for k in 0..10u16 {
        let a = f32::from(k) * std::f32::consts::TAU / 10.0 + recoil * 0.2;
        let rim = vec2(
            axle.x + a.cos() * radius * 0.78,
            axle.y + a.sin() * radius * 0.78,
        );
        draw_line(axle.x, axle.y, rim.x, rim.y, (0.12 * s).max(1.0), CARRIAGE);
        draw_circle(rim.x, rim.y, 0.045 * s, Color::from_hex(0xDF_C1_7F));
    }
    draw_circle(axle.x, axle.y, radius * 0.3, IRON);
    draw_circle(
        axle.x - 0.04 * s,
        axle.y - 0.06 * s,
        radius * 0.16,
        Color::from_hex(0xD8_B2_6B),
    );
}

/// Layered metal shading; the lip uses the same muzzle offset as ballistics.
fn barrel_cylinder(w: &dyn Fn(V2) -> Vec2, s: f32, bp: V2, dir: V2, heat: f32) {
    let perp = V2 {
        x: -dir.y,
        y: dir.x,
    };
    let rotation = -dir.y.atan2(dir.x);
    let breech = bp - dir * 0.55;
    let start = w(breech);
    let length = physics::BARREL_LEN + 0.55;
    let bronze = Color::from_hex(0xCE_A4_5B);
    for (offset, thick, color) in [
        (0.0, 1.14, Color::from_hex(0x20_28_2F)),
        (
            0.04,
            0.88,
            mix(IRON, Color::from_hex(0x9F_5D_3A), heat * 0.35),
        ),
        (0.24, 0.25, Color::from_hex(0x86_98_98)),
        (0.36, 0.08, Color::from_hex(0xD6_D9_BF)),
        (-0.31, 0.18, Color::from_hex(0x25_31_39)),
    ] {
        let p = w(breech + perp * offset);
        draw_rectangle_ex(
            p.x,
            p.y,
            length * s,
            thick * s,
            DrawRectangleParams {
                offset: vec2(0.0, 0.5),
                rotation,
                color,
            },
        );
    }
    for along in [0.25, 1.4, length - 0.25] {
        let p = w(breech + dir * along);
        draw_rectangle_ex(
            p.x,
            p.y,
            0.22 * s,
            1.22 * s,
            DrawRectangleParams {
                offset: vec2(0.0, 0.5),
                rotation,
                color: darken(bronze, 0.35),
            },
        );
        let p = w(breech + dir * along + perp * 0.3);
        draw_rectangle_ex(
            p.x,
            p.y,
            0.22 * s,
            0.25 * s,
            DrawRectangleParams {
                offset: vec2(0.0, 0.5),
                rotation,
                color: bronze,
            },
        );
    }
    draw_circle(start.x, start.y, 0.54 * s, darken(IRON, 0.15));
    draw_circle(start.x - 0.08 * s, start.y - 0.14 * s, 0.24 * s, bronze);
    let muzzle = w(bp + dir * physics::BARREL_LEN);
    let axis = vec2(dir.x, -dir.y);
    let side = vec2(-axis.y, axis.x);
    draw_line(
        muzzle.x - side.x * 0.58 * s,
        muzzle.y - side.y * 0.58 * s,
        muzzle.x + side.x * 0.58 * s,
        muzzle.y + side.y * 0.58 * s,
        0.28 * s,
        bronze,
    );
    draw_line(
        muzzle.x - side.x * 0.36 * s,
        muzzle.y - side.y * 0.36 * s,
        muzzle.x + side.x * 0.36 * s,
        muzzle.y + side.y * 0.36 * s,
        0.15 * s,
        Color::from_hex(0x12_1D_27),
    );
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

/// Matching siege gun on the keep's fighting platform.
fn draw_defender_cannon(state: &GameState, w: &dyn Fn(V2) -> Vec2, s: f32) {
    if !state
        .segments
        .iter()
        .any(|seg| seg.kind == world::SegmentKind::Keep && seg.alive())
    {
        return;
    }
    let pivot = world::defender_pivot();
    let a = state.defender.display_angle.to_radians();
    let dir = V2 {
        x: -a.cos(),
        y: a.sin(),
    };
    cannon_carriage(w, s, pivot, -1.0, state.defender.recoil);
    let bp = pivot - dir * (state.defender.recoil * 0.3);
    barrel_cylinder(w, s, bp, dir, state.defender.recoil);
    if state.defender.fuse > 0.0 {
        fuse_glow(
            w,
            s,
            bp + V2 { x: 0.0, y: 0.35 },
            state.defender.fuse_progress(),
            state.t,
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
        let trail_color = if b.side == physics::Side::Player {
            Color::new(1.0, 0.75, 0.35, 1.0)
        } else {
            Color::new(1.0, 0.39, 0.19, 1.0)
        };
        let mut previous = None;
        for (i, tp) in b.trail.iter().enumerate() {
            let f = (i + 1) as f32 / b.trail.len().max(1) as f32;
            let v = w2s(*tp) + shake;
            if let Some(from) = previous {
                let from: Vec2 = from;
                draw_line(
                    from.x,
                    from.y,
                    v.x,
                    v.y,
                    (0.45 * s * f).max(1.0),
                    Color::new(trail_color.r, trail_color.g, trail_color.b, 0.38 * f * f),
                );
                draw_line(
                    from.x,
                    from.y,
                    v.x,
                    v.y,
                    (0.12 * s * f).max(0.8),
                    Color::new(1.0, 0.91, 0.67, 0.65 * f * f),
                );
            }
            previous = Some(v);
            draw_circle(
                v.x,
                v.y,
                (BALL_R * s * (1.0 - f) * 1.8).max(1.0),
                Color::new(0.26, 0.29, 0.32, 0.12 * f),
            );
        }
        let v = w2s(b.pos) + shake;
        // Visual radius floors keep the ball readable at small window scales.
        let r = (BALL_R * s).max(3.5);
        // Dawn rim on the sunward side + deep base so the shot reads as
        // iron, not a flat black dot.
        draw_circle(v.x, v.y, r * 1.18, Color::new(1.0, 0.62, 0.3, 0.25));
        draw_circle(v.x, v.y, r, BALL_C);
        draw_circle(
            v.x - r * 0.18,
            v.y + r * 0.2,
            r * 0.72,
            Color::new(0.23, 0.23, 0.28, 1.0),
        );
        // Surface dimples riding `spin`: the ball visibly rolls with its
        // travel instead of sliding.
        for dimp in 0..3u16 {
            let spot_a = b.spin + f32::from(dimp) * std::f32::consts::TAU / 3.0;
            let spot = vec2(v.x + spot_a.cos() * r * 0.55, v.y + spot_a.sin() * r * 0.55);
            draw_circle(
                spot.x,
                spot.y,
                (r * 0.2).max(1.2),
                Color::new(0.1, 0.1, 0.13, 0.85),
            );
        }
        // Hot sun glint + cool under-shade.
        draw_circle(
            v.x - r * 0.34,
            v.y - r * 0.36,
            r * 0.30,
            Color::new(1.0, 0.9, 0.72, 0.9),
        );
        draw_circle(
            v.x - r * 0.3,
            v.y - r * 0.32,
            r * 0.16,
            Color::new(1.0, 1.0, 0.96, 0.95),
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
        // Soft halo pass under a hot core so the guide glows at dawn.
        if i % 8 == 0 {
            let s0 = w2s(prev) + shake;
            let s1 = w2s(p) + shake;
            draw_line(
                s0.x,
                s0.y,
                s1.x,
                s1.y,
                5.0,
                Color::new(1.0, 0.8, 0.45, 0.16),
            );
            draw_line(
                s0.x,
                s0.y,
                s1.x,
                s1.y,
                2.0,
                Color::new(1.0, 0.97, 0.86, 0.75),
            );
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
        // Halo under the X so fresh impacts pop against grass and stone.
        draw_circle(v.x, v.y, r * 1.6, Color::new(c.r, c.g, c.b, 0.22));
        draw_line(
            v.x - r,
            v.y - r,
            v.x + r,
            v.y + r,
            4.0,
            Color::new(c.r, c.g, c.b, 0.35),
        );
        draw_line(
            v.x - r,
            v.y + r,
            v.x + r,
            v.y - r,
            4.0,
            Color::new(c.r, c.g, c.b, 0.35),
        );
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
