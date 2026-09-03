//! Scenery pass — sky gradient, sun, wind-driven clouds, parallax mountain
//! ridges, ground band, and ground dressing (grass, rocks, craters). Split
//! from `render.rs` to honor the 500-line-per-file convention.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use super::{GRASS_LOW, SKY_TOP, STONE, WORLD_H, WORLD_W, darken, hash2, mix, origin, scale, w2s};
use crate::game::GameState;
use crate::physics::V2;
use crate::rng::Rng;
use crate::world;
use macroquad::color::Color;
use macroquad::math::{Vec2, vec2};
use macroquad::shapes::{draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_triangle};
use macroquad::window::{screen_height, screen_width};

const SKY_MID: Color = Color::from_hex(0x7A_3B_5E);
const SKY_HOR: Color = Color::from_hex(0xF2_A6_5A);
const SUN_C: Color = Color::from_hex(0xFF_E8_B0);
const MOUNT_FAR: Color = Color::from_hex(0x3E_2F_55);
const MOUNT_NEAR: Color = Color::from_hex(0x4F_3B_63);
const TREELINE: Color = Color::from_hex(0x43_58_43);
const GRASS_TOP: Color = Color::from_hex(0x6F_A3_5C);
const DIRT: Color = Color::from_hex(0x7A_5B_3F);
const CRATER_C: Color = Color::from_hex(0x4A_35_27);

pub(super) fn draw_sky(shake: Vec2) {
    let (_, oy) = origin();
    let horizon = w2s(V2 { x: 0.0, y: 0.0 }).y + shake.y;
    // 64 bands kill the stripe steps the old 26-band ramp showed on
    // large windows; a whisper of hash dither breaks up the remaining
    // 8-bit gradient contouring without visible noise.
    let bands = 64u16;
    let band_h = (horizon - oy) / f32::from(bands);
    for i in 0..bands {
        let t = f32::from(i) / f32::from(bands - 1);
        let mut col = if t < 0.55 {
            // Ease the stops so the zenith stays deep and the mid blooms.
            let u = t / 0.55;
            mix(SKY_TOP, SKY_MID, u * u * (3.0 - 2.0 * u))
        } else {
            let u = (t - 0.55) / 0.45;
            mix(SKY_MID, SKY_HOR, u.sqrt())
        };
        let dither = (hash2(u32::from(i), 77) - 0.5) * 0.012;
        col.r = (col.r + dither).clamp(0.0, 1.0);
        col.g = (col.g + dither).clamp(0.0, 1.0);
        col.b = (col.b + dither).clamp(0.0, 1.0);
        draw_rectangle(
            0.0,
            oy + f32::from(i) * band_h + shake.y,
            screen_width(),
            band_h + 1.5,
            col,
        );
    }
    // Warm haze hugging the horizon so the sky melts into the hills.
    let sc = scale();
    draw_rectangle(
        0.0,
        horizon - 7.0 * sc + shake.y,
        screen_width(),
        7.0 * sc,
        Color::new(1.0, 0.72, 0.42, 0.14),
    );
    draw_rectangle(
        0.0,
        horizon - 3.0 * sc + shake.y,
        screen_width(),
        3.0 * sc,
        Color::new(1.0, 0.80, 0.55, 0.12),
    );
}

pub(super) fn draw_sun(shake: Vec2) {
    let s = scale();
    let c = w2s(V2 { x: 60.0, y: 78.0 }) + shake;
    // Wide-to-tight falloff: vast faint bloom, mid glow, hot disc, white core.
    for (rm, alpha) in [(26.0, 0.04), (16.0, 0.07), (10.0, 0.13), (6.5, 0.22)] {
        draw_circle(
            c.x,
            c.y,
            rm * s,
            Color::new(SUN_C.r, SUN_C.g, SUN_C.b, alpha),
        );
    }
    draw_circle(c.x, c.y, 4.0 * s, SUN_C);
    draw_circle(c.x, c.y, 3.1 * s, Color::new(1.0, 0.96, 0.86, 1.0));
    draw_circle(c.x, c.y, 2.1 * s, Color::new(1.0, 1.0, 0.96, 1.0));
    // Horizontal dawn streak through the disc.
    draw_ellipse(
        c.x,
        c.y,
        9.5 * s,
        0.9 * s,
        0.0,
        Color::new(1.0, 0.85, 0.6, 0.18),
    );
}
pub(super) fn draw_clouds(state: &GameState, shake: Vec2) {
    let s = scale();
    for layer in 0..2u32 {
        let par = if layer == 0 { 0.1 } else { 0.18 };
        let count = if layer == 0 { 4u32 } else { 5u32 };
        let alpha = if layer == 0 { 0.42 } else { 0.6 };
        let off = shake * par;
        for i in 0..count {
            let h1 = hash2(layer, i + 1);
            let h2 = hash2(i + 13, layer + 7);
            let base_x = h1 * 260.0 - 30.0;
            let cy = 60.0 + h2 * 34.0 - (layer as f32) * 10.0;
            let size = 3.5 + h1 * 4.5 + (layer as f32) * 1.8;
            // Integrated wind travel, never `wind * t`: that product
            // teleports clouds by Δwind × t on every wind change.
            let drift = state.t * (0.9 + 0.5 * h2) + state.wind.travel() * 0.22;
            let cx = ((base_x + drift + 40.0) % 280.0 + 280.0) % 280.0 - 40.0;
            let p = w2s(V2 { x: cx, y: cy }) + off;
            // Lit top first, then the shaded belly offset below — one extra
            // pass per puff reads as volume instead of flat cutouts.
            let belly = Color::new(0.72, 0.52, 0.55, alpha * 0.85);
            let lit = Color::new(1.0, 0.93, 0.86, alpha);
            let puffs = [
                (0.0, 0.0, 1.0, 0.42),
                (-0.62, 0.1, 0.6, 0.3),
                (0.66, 0.12, 0.55, 0.28),
                (0.1, -0.26, 0.5, 0.3),
            ];
            for (dx, dy, wr, hr) in puffs {
                draw_ellipse(
                    p.x + dx * size * s,
                    p.y + (dy * size + 0.22 * size) * s,
                    size * wr * s,
                    size * hr * s,
                    0.0,
                    belly,
                );
            }
            for (dx, dy, wr, hr) in puffs {
                draw_ellipse(
                    p.x + dx * size * s,
                    p.y + dy * size * s,
                    size * wr * s,
                    size * hr * s,
                    0.0,
                    lit,
                );
            }
            // Sun-kissed rim on the sunward edge.
            draw_ellipse(
                p.x - size * 0.25 * s,
                p.y - size * 0.22 * s,
                size * 0.55 * s,
                size * 0.16 * s,
                -0.15,
                Color::new(1.0, 0.98, 0.94, alpha * 0.9),
            );
        }
    }
}

/// Heightfield parameters for one mountain silhouette layer.
struct Ridge {
    par: f32,
    color: Color,
    amp1: f32,
    f1: f32,
    p1: f32,
    amp2: f32,
    f2: f32,
    p2: f32,
    base: f32,
}

fn mountain_layer(shake: Vec2, r: &Ridge) {
    let off = shake * r.par;
    let bottom = w2s(V2 { x: 0.0, y: 0.0 }).y + off.y + 40.0;
    // 1.8 m steps instead of 5 m: the old facets telegraphed as zigzag
    // polygons on wide windows. Fill quads first, then one highlight
    // polyline along the ridge so the silhouette catches the dawn.
    let mut prev: Option<Vec2> = None;
    let mut ridge: [Vec2; 128] = [vec2(0.0, 0.0); 128];
    let mut n = 0usize;
    let mut ix = 0i32;
    while ix <= 120 {
        let x = -10.0 + 1.833_333_3 * ix as f32;
        let y = r.base + r.amp1 * (r.f1 * x + r.p1).sin() + r.amp2 * (r.f2 * x + r.p2).sin();
        let pt = w2s(V2 { x, y }) + off;
        if n < ridge.len() {
            ridge[n] = pt;
            n += 1;
        }
        if let Some(prev_pt) = prev {
            let bl = vec2(prev_pt.x, bottom);
            let br = vec2(pt.x, bottom);
            draw_triangle(prev_pt, pt, br, r.color);
            draw_triangle(prev_pt, br, bl, r.color);
        }
        prev = Some(pt);
        ix += 1;
    }
    let hl = Color::new(1.0, 0.72, 0.5, 0.28);
    for k in 1..n {
        draw_line(
            ridge[k - 1].x,
            ridge[k - 1].y,
            ridge[k].x,
            ridge[k].y,
            2.0,
            hl,
        );
    }
}

pub(super) fn draw_mountains(shake: Vec2) {
    let far = Ridge {
        par: 0.25,
        color: MOUNT_FAR,
        amp1: 9.0,
        f1: 0.021,
        p1: 1.7,
        amp2: 5.0,
        f2: 0.043,
        p2: 0.4,
        base: 27.0,
    };
    let near = Ridge {
        par: 0.4,
        color: MOUNT_NEAR,
        amp1: 7.0,
        f1: 0.028,
        p1: 4.0,
        amp2: 4.0,
        f2: 0.052,
        p2: 2.2,
        base: 18.0,
    };
    mountain_layer(shake, &far);
    mountain_layer(shake, &near);
    // Distant tree line.
    let off = shake * 0.55;
    let col = Color::new(TREELINE.r, TREELINE.g, TREELINE.b, 0.2);
    let sc = scale();
    let mut ix = 0i32;
    while ix <= 70 {
        let x = -4.0 + 3.0 * ix as f32;
        let y = 9.5 + 2.0 * (0.05 * x + 1.0).sin() + 1.2 * hash2(ix as u32, 5);
        let pt = w2s(V2 { x, y }) + off;
        let rad = (1.4 + 1.6 * hash2(ix as u32, 9)) * sc;
        draw_circle(pt.x, pt.y, rad, col);
        ix += 1;
    }
}

/// Distant birds: flapping chevrons drifting across the sky on the wind.
pub(super) fn draw_birds(state: &GameState, shake: Vec2) {
    let s = scale();
    let off = shake * 0.15;
    let ink = Color::new(0.13, 0.11, 0.16, 0.75);
    for i in 0..4u32 {
        let h1 = hash2(i + 41, 3);
        let h2 = hash2(i + 7, 19);
        // Integrated drift (never `wind * t`), same rule as the clouds.
        let drift = state.t * (1.4 + 1.6 * h1) + state.wind.travel() * 0.4 + h2 * 240.0;
        let x = drift.rem_euclid(240.0) - 20.0;
        let y = 58.0 + h1 * 28.0;
        let flap = (state.t * (5.0 + 3.0 * h2) + h1 * std::f32::consts::TAU).sin();
        let p = w2s(V2 { x, y }) + off;
        let span = (0.5 + 0.3 * h2) * s;
        let lift = flap * 0.45 * span;
        draw_line(p.x - span, p.y - lift, p.x, p.y, 1.5, ink);
        draw_line(p.x, p.y, p.x + span, p.y - lift, 1.5, ink);
    }
}

pub(super) fn draw_ground(state: &GameState, shake: Vec2) {
    let sc = scale();
    let off = shake; // world layer: full shake
    let to_px = |pt: V2| w2s(pt) + off;
    let slices = 200u32;
    let step = WORLD_W / slices as f32;
    let baseline = w2s(V2 { x: 0.0, y: 0.0 }).y + off.y;
    for idx in 0..slices {
        let x0 = idx as f32 * step;
        let x1 = x0 + step;
        let p0 = to_px(V2 {
            x: x0,
            y: world::ground_height(x0),
        });
        let p1 = to_px(V2 {
            x: x1,
            y: world::ground_height(x1),
        });
        // Low-frequency meadow variation (two slow sines) + a whisper of
        // hash so adjacent metres no longer checkerboard.
        let xm = (x0 + x1) * 0.5;
        let smooth = 0.5 + 0.5 * (0.06 * xm + 1.0).sin() * (0.023 * xm + 0.4).sin();
        let grain = hash2(idx, 3) - 0.5;
        let patch = (0.32 + 0.28 * smooth + 0.06 * grain).clamp(0.0, 1.0);
        let col = mix(GRASS_LOW, GRASS_TOP, patch);
        let bl = vec2(p0.x, baseline);
        let br = vec2(p1.x, baseline);
        draw_triangle(p0, p1, br, col);
        draw_triangle(p0, br, bl, col);
    }
    // Grass edge highlight.
    let mut prev: Option<Vec2> = None;
    let mut idx = 0u32;
    while idx <= slices {
        let x = idx as f32 * step;
        let pt = to_px(V2 {
            x,
            y: world::ground_height(x),
        });
        if let Some(prev_pt) = prev {
            draw_line(
                prev_pt.x,
                prev_pt.y,
                pt.x,
                pt.y,
                (0.28 * sc).max(1.0),
                mix(GRASS_TOP, SUN_C, 0.25),
            );
        }
        prev = Some(pt);
        idx += 1;
    }
    // Dirt column with a soft vertical falloff + sun-side warmth, so the
    // old flat brown slab gains depth. Eight strips are cheap and band-free
    // at these low-contrast stops.
    let dirt_h = (screen_height() - baseline).max(0.0);
    for k in 0..8u16 {
        let t0 = f32::from(k) / 8.0;
        let t1 = f32::from(k + 1) / 8.0;
        let mut col = mix(DIRT, darken(DIRT, 0.45), t0 * t0);
        col = mix(col, Color::from_hex(0x8A_6A_48), (1.0 - t0) * 0.18);
        draw_rectangle(
            0.0,
            baseline + dirt_h * t0,
            screen_width(),
            dirt_h * (t1 - t0) + 1.5,
            col,
        );
    }
    // Contact shadow where grass meets soil.
    draw_rectangle(
        0.0,
        baseline,
        screen_width(),
        (0.5 * sc).max(2.0),
        darken(DIRT, 0.35),
    );
    // Craters.
    for cr in &state.craters {
        let gy = world::ground_height(cr.x);
        let ctr = to_px(V2 { x: cr.x, y: gy });
        draw_ellipse(
            ctr.x,
            ctr.y - 0.1 * sc,
            cr.r * 1.35 * sc,
            cr.r * 0.42 * sc,
            0.0,
            darken(DIRT, 0.15),
        );
        draw_ellipse(
            ctr.x,
            ctr.y,
            cr.r * 1.15 * sc,
            cr.r * 0.34 * sc,
            0.0,
            CRATER_C,
        );
    }
    // Deterministic tufts / rocks / bushes.
    draw_ground_dressing(state, &to_px, sc);
}

/// Seeded grass tufts, rocks, and bushes hugging the terrain curve.
fn draw_ground_dressing(state: &GameState, to_px: &dyn Fn(V2) -> Vec2, sc: f32) {
    let mut rng = Rng::seed(7);
    for _ in 0..70 {
        let x = rng.range(0.0, 200.0);
        let gy = world::ground_height(x);
        let pt = to_px(V2 { x, y: gy });
        let tuft_h = (0.5 + rng.range(0.0, 0.5)) * sc;
        let lean = (rng.range(-0.3, 0.3) + wind_lean(state)) * sc;
        let col = mix(GRASS_LOW, GRASS_TOP, rng.range(0.2, 0.9));
        draw_line(pt.x, pt.y, pt.x + lean - 0.25 * sc, pt.y - tuft_h, 1.0, col);
        draw_line(pt.x, pt.y, pt.x + lean, pt.y - tuft_h * 1.15, 1.0, col);
        draw_line(pt.x, pt.y, pt.x + lean + 0.25 * sc, pt.y - tuft_h, 1.0, col);
    }
    for _ in 0..14 {
        let x = rng.range(0.0, 200.0);
        let gy = world::ground_height(x);
        let pt = to_px(V2 { x, y: gy });
        let rad = rng.range(0.25, 0.6) * sc;
        draw_ellipse(
            pt.x,
            pt.y - rad * 0.3,
            rad * 1.3,
            rad * 0.8,
            0.0,
            mix(STONE, DIRT, 0.35),
        );
    }
    for _ in 0..10 {
        let x = rng.range(0.0, 200.0);
        let gy = world::ground_height(x);
        let pt = to_px(V2 { x, y: gy });
        let rad = rng.range(0.7, 1.3) * sc;
        let col = darken(GRASS_LOW, 0.25);
        draw_circle(pt.x - rad * 0.5, pt.y - rad * 0.4, rad * 0.7, col);
        draw_circle(pt.x + rad * 0.5, pt.y - rad * 0.35, rad * 0.65, col);
        draw_circle(pt.x, pt.y - rad * 0.7, rad * 0.8, mix(col, GRASS_TOP, 0.25));
    }
}

/// Shared wind lean for grass (unit-ish, small).
pub(super) fn wind_lean(state: &GameState) -> f32 {
    state.wind.current() * 0.03
}

/// Slowly swaying light shafts fanning from the sun across the dawn
/// sky. Drawn before the clouds so weather passes in front of the rays.
pub(super) fn draw_god_rays(state: &GameState, shake: Vec2) {
    let s = scale();
    let sun = w2s(V2 { x: 60.0, y: 78.0 }) + shake * 0.1;
    for i in 0..5u32 {
        let h = hash2(i + 61, 21);
        let sway = (state.t * 0.05 + h * std::f32::consts::TAU).sin() * 0.05;
        let ang = (i as f32 - 2.0) * 0.22 + sway; // rad off vertical
        let dir = vec2(ang.sin(), ang.cos()); // screen space: +y is down
        let perp = vec2(-dir.y, dir.x);
        let reach = WORLD_H * 1.2 * s;
        let half = (1.8 + 2.8 * h) * s;
        draw_triangle(
            sun,
            sun + dir * reach + perp * half,
            sun + dir * reach - perp * half,
            Color::new(1.0, 0.93, 0.78, 0.05 + 0.03 * h),
        );
    }
}

/// Subtle frame shading: nested dark bands on each edge, drawn after the
/// world and under the HUD — sells the painted-tableau framing.
pub(super) fn draw_vignette() {
    // Six feathered stops instead of three hard bands: no visible step.
    let (w, h) = (screen_width(), screen_height());
    for (band, a) in [
        (150.0_f32, 0.03),
        (110.0, 0.035),
        (78.0, 0.04),
        (52.0, 0.05),
        (30.0, 0.06),
        (14.0, 0.07),
    ] {
        let c = Color::new(0.06, 0.04, 0.1, a);
        draw_rectangle(0.0, 0.0, w, band, c);
        draw_rectangle(0.0, h - band, w, band, c);
        draw_rectangle(0.0, 0.0, band, h, c);
        draw_rectangle(w - band, 0.0, band, h, c);
    }
}
