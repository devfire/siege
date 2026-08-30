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
use crate::rng::Rng;
use crate::world::{self, SegmentKind};
use macroquad::color::Color;
use macroquad::math::{Vec2, vec2};
use macroquad::shapes::{
    DrawRectangleParams, draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_rectangle_ex,
    draw_triangle,
};
use macroquad::text::{TextDimensions, TextParams, draw_text_ex, measure_text};
use macroquad::window::{screen_height, screen_width};

pub const WORLD_W: f32 = 200.0;
pub const WORLD_H: f32 = 112.5;

const PAGE_BG: Color = Color::from_hex(0x1A_14_23);
const SKY_TOP: Color = Color::from_hex(0x2B_1B_4D);
const SKY_MID: Color = Color::from_hex(0x7A_3B_5E);
const SKY_HOR: Color = Color::from_hex(0xF2_A6_5A);
const SUN_C: Color = Color::from_hex(0xFF_E8_B0);
const MOUNT_FAR: Color = Color::from_hex(0x3E_2F_55);
const MOUNT_NEAR: Color = Color::from_hex(0x4F_3B_63);
const TREELINE: Color = Color::from_hex(0x43_58_43);
const GRASS_TOP: Color = Color::from_hex(0x6F_A3_5C);
const GRASS_LOW: Color = Color::from_hex(0x4C_7A_44);
const DIRT: Color = Color::from_hex(0x7A_5B_3F);
const CRATER_C: Color = Color::from_hex(0x4A_35_27);
const STONE: Color = Color::from_hex(0xA8_A2_9A);
const MORTAR: Color = Color::from_hex(0x6E_67_5F);
const KEEP_C: Color = Color::from_hex(0x94_90_8B);
const WOOD: Color = Color::from_hex(0x7A_4E_2D);
const ROOF_C: Color = Color::from_hex(0x8C_3B_2E);
const BANNER: Color = Color::from_hex(0xC2_3B_2E);
const RUBBLE: Color = Color::from_hex(0x5C_58_54);
const IRON: Color = Color::from_hex(0x3A_3F_45);
const CARRIAGE: Color = Color::from_hex(0x6B_45_26);
const WHEEL_C: Color = Color::from_hex(0x4E_34_21);
const BALL_C: Color = Color::from_hex(0x2B_2B_30);
const PARCHMENT: Color = Color::from_hex(0xF0_E6_D2);
const INK: Color = Color::from_hex(0x2E_26_20);
const RED: Color = Color::from_hex(0xC2_3B_2E);
const GOOD_HP: Color = Color::from_hex(0x6F_A3_5C);
const MID_HP: Color = Color::from_hex(0xD9_A4_41);
const LOW_HP: Color = Color::from_hex(0xC2_3B_2E);
const DEAD_HP: Color = Color::from_hex(0x3A_36_34);

/// Pixels per world metre.
#[must_use]
pub fn scale() -> f32 {
    (screen_width() / WORLD_W).min(screen_height() / WORLD_H)
}

/// Screen-space top-left of the letterboxed world rect.
#[must_use]
fn origin() -> (f32, f32) {
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

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn darken(c: Color, amt: f32) -> Color {
    mix(c, Color::new(0.0, 0.0, 0.0, c.a), amt)
}

/// Deterministic 0..1 hash of two integers (for bricks, tufts, cracks…).
fn hash2(a: u32, b: u32) -> f32 {
    let mut h = a
        .wrapping_mul(374_761_393)
        .wrapping_add(b.wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    let v = (h ^ (h >> 16)) & 0xFFFF;
    #[allow(clippy::cast_possible_truncation)] // masked to 16 bits above
    let wide = v as u16;
    f32::from(wide) / 65_535.0
}

fn txt(text: &str, x: f32, y: f32, size: u16, color: Color, font: Option<&macroquad::text::Font>) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font,
            font_size: size,
            color,
            ..Default::default()
        },
    );
}

fn txt_centered(
    text: &str,
    x: f32,
    y: f32,
    size: u16,
    color: Color,
    font: Option<&macroquad::text::Font>,
) {
    let d: TextDimensions = measure_text(text, font, size, 1.0);
    txt(text, x - d.width * 0.5, y, size, color, font);
}

pub fn draw(state: &GameState, font: Option<&macroquad::text::Font>) {
    macroquad::window::clear_background(PAGE_BG);
    let s = scale();
    let (ox, oy) = origin();
    // Screen shake: pseudo-noise direction, world layers only, deeper
    // parallax layers shake less.
    let shk = state.shake * s * 0.45;
    let shake = vec2((state.t * 61.7).sin() * shk, (state.t * 53.3).cos() * shk);
    let wind = state.wind.current(state.t);
    draw_rectangle(ox, oy, WORLD_W * s, WORLD_H * s, SKY_TOP);
    draw_sky(shake);
    draw_sun(shake * 0.1);
    draw_clouds(state, wind, shake);
    draw_mountains(shake);
    draw_ground(state, shake);
    draw_castle(state, shake);
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

fn draw_sky(shake: Vec2) {
    let (_, oy) = origin();
    let horizon = w2s(V2 { x: 0.0, y: 0.0 }).y + shake.y;
    let bands = 26u16;
    let band_h = (horizon - oy) / f32::from(bands);
    for i in 0..bands {
        let t = f32::from(i) / f32::from(bands - 1);
        let col = if t < 0.55 {
            mix(SKY_TOP, SKY_MID, t / 0.55)
        } else {
            mix(SKY_MID, SKY_HOR, (t - 0.55) / 0.45)
        };
        draw_rectangle(
            0.0,
            oy + f32::from(i) * band_h + shake.y,
            screen_width(),
            band_h + 1.5,
            col,
        );
    }
}

fn draw_sun(shake: Vec2) {
    let s = scale();
    let c = w2s(V2 { x: 60.0, y: 78.0 }) + shake;
    for (rm, alpha) in [(16.0, 0.07), (10.0, 0.13), (6.5, 0.22)] {
        draw_circle(
            c.x,
            c.y,
            rm * s,
            Color::new(SUN_C.r, SUN_C.g, SUN_C.b, alpha),
        );
    }
    draw_circle(c.x, c.y, 4.0 * s, SUN_C);
}

fn draw_clouds(state: &GameState, wind: f32, shake: Vec2) {
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
            let drift = state.t * (0.9 + 0.5 * h2) + wind * state.t * 0.22;
            let cx = ((base_x + drift + 40.0) % 280.0 + 280.0) % 280.0 - 40.0;
            let col = Color::new(0.98, 0.9, 0.82, alpha);
            let p = w2s(V2 { x: cx, y: cy }) + off;
            draw_ellipse(p.x, p.y, size * s, size * 0.42 * s, 0.0, col);
            draw_ellipse(
                p.x - size * 0.62 * s,
                p.y + size * 0.1 * s,
                size * 0.6 * s,
                size * 0.3 * s,
                0.0,
                col,
            );
            draw_ellipse(
                p.x + size * 0.66 * s,
                p.y + size * 0.12 * s,
                size * 0.55 * s,
                size * 0.28 * s,
                0.0,
                col,
            );
            draw_ellipse(
                p.x + size * 0.1 * s,
                p.y - size * 0.26 * s,
                size * 0.5 * s,
                size * 0.3 * s,
                0.0,
                col,
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

/// One mountain silhouette layer: heightfield = sum of two sines, filled
/// down below the baseline so the ground always covers the seam.
fn mountain_layer(shake: Vec2, r: &Ridge) {
    let off = shake * r.par;
    let bottom = w2s(V2 { x: 0.0, y: 0.0 }).y + off.y + 40.0;
    let mut prev: Option<Vec2> = None;
    let mut ix = 0i32;
    while ix <= 44 {
        let x = -10.0 + 5.0 * ix as f32;
        let y = r.base + r.amp1 * (r.f1 * x + r.p1).sin() + r.amp2 * (r.f2 * x + r.p2).sin();
        let pt = w2s(V2 { x, y }) + off;
        if let Some(prev_pt) = prev {
            let bl = vec2(prev_pt.x, bottom);
            let br = vec2(pt.x, bottom);
            draw_triangle(prev_pt, pt, br, r.color);
            draw_triangle(prev_pt, br, bl, r.color);
        }
        prev = Some(pt);
        ix += 1;
    }
}

fn draw_mountains(shake: Vec2) {
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

fn draw_ground(state: &GameState, shake: Vec2) {
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
        let patch = 0.85 + 0.3 * hash2(idx, 3);
        let col = mix(GRASS_TOP, GRASS_LOW, (0.45 * patch).min(1.0));
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
    // Dirt column from world floor to the bottom of the screen.
    draw_rectangle(
        0.0,
        baseline,
        screen_width(),
        screen_height() - baseline,
        DIRT,
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
fn wind_lean(state: &GameState) -> f32 {
    state.wind.current(state.t) * 0.03
}

fn draw_castle(state: &GameState, shake: Vec2) {
    let s = scale();
    let off = shake;
    let w = |p: V2| w2s(p) + off;
    // Keep first (stands behind the walls), then walls front-to-back.
    for pass in 0..2 {
        for (ix, seg) in state.segments.iter().enumerate() {
            let is_keep = seg.kind == SegmentKind::Keep;
            if (pass == 0) != is_keep {
                continue;
            }
            let frac = if seg.alive() {
                seg.hp / seg.max_hp
            } else {
                0.0
            };
            let seed = ix as u32 + 1;
            if seg.alive() {
                let base = match seg.kind {
                    SegmentKind::Gate => WOOD,
                    SegmentKind::Keep => KEEP_C,
                    _ => STONE,
                };
                let base = if frac < 0.33 {
                    darken(base, 0.15)
                } else {
                    base
                };
                let tl = w(V2 {
                    x: seg.x0,
                    y: seg.y0 + seg.h,
                });
                draw_rectangle(tl.x, tl.y, seg.w * s, seg.h * s, base);
                match seg.kind {
                    SegmentKind::Gate => draw_gate_planks(seg.x0, seg.y0, seg.w, seg.h, &w),
                    _ => draw_stone(seg.x0, seg.y0, seg.w, seg.h, seed, &w),
                }
                if seg.kind != SegmentKind::Gate {
                    draw_crenellations(seg.x0, seg.y0 + seg.h, seg.w, &w);
                }
                if seg.kind == SegmentKind::Keep {
                    draw_keep_roof(state, seg.x0, seg.w, seg.y0 + seg.h, &w);
                } else {
                    draw_arrow_slits(seg.x0, seg.y0, seg.w, seg.h, seed, &w);
                }
                if frac < 0.66 {
                    draw_crack(seg.x0, seg.y0, seg.w, seg.h, seed, 1, &w);
                }
                if frac < 0.33 {
                    draw_crack(seg.x0, seg.y0, seg.w, seg.h, seed + 31, 2, &w);
                }
            } else {
                let (rx, ry, rw, rh) = world::rubble_rect(seg);
                draw_rubble(rx, ry, rw, rh, seed, &w);
            }
        }
    }
    let _ = s;
}

fn draw_stone(x0: f32, y0: f32, width: f32, height: f32, seed: u32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let row_h = 1.1;
    let brick_w = 1.8;
    let rows = (height / row_h) as u32;
    let cols = (width / brick_w) as u32 + 1;
    for r in 0..rows {
        let yy = y0 + r as f32 * row_h;
        let row_top = (yy + row_h).min(y0 + height);
        // Mortar line.
        let ml = w(V2 { x: x0, y: row_top });
        draw_line(
            ml.x,
            ml.y,
            ml.x + width * s,
            ml.y,
            1.0,
            Color::new(MORTAR.r, MORTAR.g, MORTAR.b, 0.55),
        );
        let shift = if r % 2 == 0 { 0.0 } else { brick_w * 0.5 };
        for c in 0..cols {
            let bx = x0 + c as f32 * brick_w - shift;
            let h = hash2(seed * 97 + c, r + 1);
            if h < 0.3 {
                // Sparse per-brick shading ±8%.
                let bw = brick_w.min(x0 + width - bx.max(x0));
                if bw <= 0.0 {
                    continue;
                }
                let tl = w(V2 {
                    x: bx.max(x0),
                    y: row_top,
                });
                let shade = if h < 0.15 {
                    Color::new(0.0, 0.0, 0.0, 0.08)
                } else {
                    Color::new(1.0, 1.0, 1.0, 0.07)
                };
                draw_rectangle(tl.x, tl.y, bw * s, (row_top - yy) * s, shade);
            }
        }
    }
}

fn draw_gate_planks(x0: f32, y0: f32, width: f32, height: f32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let plank_w = 0.75;
    let count = (width / plank_w) as u32 + 1;
    for c in 0..count {
        let px = x0 + c as f32 * plank_w;
        let bw = plank_w.min(x0 + width - px);
        if bw <= 0.0 {
            continue;
        }
        let h = hash2(c + 3, 17);
        let col = darken(WOOD, 0.08 + 0.14 * h);
        let tl = w(V2 {
            x: px,
            y: y0 + height,
        });
        draw_rectangle(tl.x, tl.y, bw * s, height * s, col);
    }
    // Two horizontal iron-studded beams.
    for beam in [0.3, 0.7] {
        let by = y0 + height * beam;
        let tl = w(V2 { x: x0, y: by + 0.4 });
        draw_rectangle(tl.x, tl.y, width * s, 0.4 * s, darken(WOOD, 0.35));
    }
}

fn draw_crenellations(x0: f32, top: f32, width: f32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let merlon = 0.8_f32;
    let pitch = 1.6_f32;
    let count = (width / pitch) as u32 + 1;
    for c in 0..count {
        let mx = x0 + c as f32 * pitch;
        let mw = merlon.min(x0 + width - mx);
        if mw <= 0.0 {
            continue;
        }
        let tl = w(V2 {
            x: mx,
            y: top + 0.9,
        });
        draw_rectangle(tl.x, tl.y, mw * s, 0.9 * s, STONE);
    }
}

fn draw_arrow_slits(x0: f32, y0: f32, width: f32, height: f32, seed: u32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    if height < 10.0 {
        return;
    }
    for k in 0..2u32 {
        let sx = x0 + width * (0.35 + 0.3 * hash2(seed, k + 11));
        let sy = y0 + height * (0.35 + 0.35 * hash2(seed, k + 23));
        let tl = w(V2 { x: sx, y: sy + 1.7 });
        draw_rectangle(
            tl.x,
            tl.y,
            0.28 * s,
            1.7 * s,
            Color::new(0.08, 0.07, 0.09, 0.8),
        );
    }
}

fn draw_keep_roof(state: &GameState, x0: f32, width: f32, top: f32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let cx = x0 + width * 0.5;
    let apex = w(V2 {
        x: cx,
        y: top + 6.5,
    });
    let left = w(V2 {
        x: x0 - 0.7,
        y: top,
    });
    let right = w(V2 {
        x: x0 + width + 0.7,
        y: top,
    });
    draw_triangle(left, apex, right, ROOF_C);
    draw_triangle(left, apex, w(V2 { x: cx, y: top }), darken(ROOF_C, 0.18));
    // Banner pole + waving flag.
    let pole_top = w(V2 {
        x: cx,
        y: top + 10.0,
    });
    draw_line(apex.x, apex.y, pole_top.x, pole_top.y, 2.0, IRON);
    let slices = 8u32;
    let flag_w = 3.4;
    let flag_h = 1.9;
    for i in 0..slices {
        let fx0 = flag_w * i as f32 / slices as f32;
        let fx1 = flag_w * (i + 1) as f32 / slices as f32;
        let wave = |fx: f32| (state.t * 3.0 + fx * 1.4).sin() * 0.22 * (fx / flag_w + 0.25);
        let a0 = w(V2 {
            x: cx + fx0,
            y: top + 9.8 + wave(fx0),
        });
        let a1 = w(V2 {
            x: cx + fx1,
            y: top + 9.8 + wave(fx1),
        });
        let b0 = w(V2 {
            x: cx + fx0,
            y: top + 9.8 - flag_h + wave(fx0) * 1.2,
        });
        let b1 = w(V2 {
            x: cx + fx1,
            y: top + 9.8 - flag_h + wave(fx1) * 1.2,
        });
        let shade = if i % 2 == 0 {
            BANNER
        } else {
            darken(BANNER, 0.1)
        };
        draw_triangle(a0, a1, b1, shade);
        draw_triangle(a0, b1, b0, shade);
    }
    let _ = s;
}

fn draw_crack(
    x0: f32,
    y0: f32,
    width: f32,
    height: f32,
    seed: u32,
    which: u32,
    w: &dyn Fn(V2) -> Vec2,
) {
    let mut px = x0 + width * (0.2 + 0.6 * hash2(seed, which * 3 + 1));
    let mut py = y0 + height;
    let mut prev = w(V2 { x: px, y: py });
    let steps = 7u32;
    for k in 0..steps {
        py -= height / (steps as f32 + 1.0);
        px += (hash2(seed + k, which * 7 + k) - 0.5) * width * 0.22;
        let pt = w(V2 { x: px, y: py });
        draw_line(
            prev.x,
            prev.y,
            pt.x,
            pt.y,
            1.6,
            Color::new(0.1, 0.09, 0.08, 0.7),
        );
        prev = pt;
    }
}

fn draw_rubble(x0: f32, y0: f32, width: f32, height: f32, seed: u32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let tiers = 3u32;
    for tier in 0..tiers {
        let ty = y0 + height * tier as f32 / tiers as f32;
        let th = height / tiers as f32;
        let shrink = 0.15 * tier as f32;
        let bx = x0 + width * shrink * 0.5;
        let bw = width * (1.0 - shrink);
        let blocks = 3u32;
        for b in 0..blocks {
            let h = hash2(seed + tier, b + 1);
            let bxf = bw * b as f32 / blocks as f32;
            let bwf = bw / blocks as f32 * (0.7 + 0.3 * h);
            let tl = w(V2 {
                x: bx + bxf,
                y: ty + th,
            });
            draw_rectangle(tl.x, tl.y, bwf * s, th * s, mix(RUBBLE, STONE, 0.35 * h));
        }
    }
}

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
    // Barrel.
    let pps = w(pp);
    draw_rectangle_ex(
        pps.x,
        pps.y,
        2.9 * s,
        0.52 * s,
        DrawRectangleParams {
            offset: vec2(0.0, 0.5),
            rotation: -state.player.angle_deg.to_radians(),
            color: reload_tint,
        },
    );
    // Muzzle band.
    let a = state.player.angle_deg.to_radians();
    let muzzle = w(pp
        + V2 {
            x: a.cos() * 2.75,
            y: a.sin() * 2.75,
        });
    draw_circle(muzzle.x, muzzle.y, 0.3 * s, darken(IRON, 0.3));
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
    draw_rectangle_ex(
        dp.x,
        dp.y,
        2.2 * s,
        0.4 * s,
        DrawRectangleParams {
            offset: vec2(0.0, 0.5),
            rotation: std::f32::consts::PI + state.defender.display_angle.to_radians(),
            color: dcol,
        },
    );
    // Crew silhouettes.
    for (dx, hh) in [(1.5_f32, 0.75_f32), (2.3, 0.65)] {
        let cp = w(V2 {
            x: world::DEFENDER_PIVOT_X + dx,
            y: world::KEEP_TOP + hh,
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
    let charge = state.player.charging.unwrap_or(state.player.power);
    let v0 = (charge * physics::MUZZLE_V_MAX).max(1.0);
    let t_end = 18.0 / v0;
    for i in 0..12u16 {
        let t0 = t_end * (f32::from(i) / 12.0);
        let t1 = t_end * ((f32::from(i) + 0.5) / 12.0);
        let p0 = start
            + dir * (v0 * t0)
            + V2 {
                x: 0.0,
                y: -0.5 * physics::G * t0 * t0,
            };
        let p1 = start
            + dir * (v0 * t1)
            + V2 {
                x: 0.0,
                y: -0.5 * physics::G * t1 * t1,
            };
        let s0 = w2s(p0) + shake;
        let s1 = w2s(p1) + shake;
        draw_line(s0.x, s0.y, s1.x, s1.y, 2.0, Color::new(1.0, 1.0, 0.95, 0.4));
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

/// Red vignette flash while the player is hurt (UI-space, unshaken).
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

fn hp_color(f: f32) -> Color {
    if f > 0.66 {
        GOOD_HP
    } else if f > 0.33 {
        MID_HP
    } else {
        LOW_HP
    }
}

fn draw_ui(state: &GameState, font: Option<&macroquad::text::Font>) {
    let (w, h) = (screen_width(), screen_height());
    // Player HP bar, top-left.
    draw_rectangle(24.0, 24.0, 220.0, 20.0, PARCHMENT);
    let f = (state.player.hp / 100.0).clamp(0.0, 1.0);
    draw_rectangle(26.0, 26.0, 216.0 * f, 16.0, hp_color(f));
    txt("CANNON", 24.0, 60.0, 22, INK, font);
    if state.player.reload > 0.0 {
        txt(
            &format!("reloading {:.1}s", state.player.reload),
            130.0,
            60.0,
            22,
            INK,
            font,
        );
    }
    // Castle pips + keep bar, top-right.
    let mut px = w - 24.0 - 6.0 * 28.0;
    for seg in &state.segments {
        let frac = if seg.alive() {
            seg.hp / seg.max_hp
        } else {
            0.0
        };
        let c = if seg.alive() { hp_color(frac) } else { DEAD_HP };
        draw_rectangle(px, 24.0, 24.0, 16.0, c);
        px += 28.0;
    }
    let keep = state.segments.iter().find(|s| s.kind == SegmentKind::Keep);
    if let Some(keep) = keep {
        let kf = if keep.alive() {
            keep.hp / keep.max_hp
        } else {
            0.0
        };
        draw_rectangle(w - 24.0 - 172.0, 46.0, 172.0, 10.0, PARCHMENT);
        draw_rectangle(w - 24.0 - 170.0, 48.0, 168.0 * kf, 6.0, hp_color(kf));
    }
    // Wind banner, top-center.
    let wind = state.wind.current(state.t);
    let label = format!("WIND {wind:+.1} m/s");
    txt_centered(&label, w * 0.5, 40.0, 26, PARCHMENT, font);
    let arrow_len = (wind.abs() * 4.0).min(70.0);
    if arrow_len > 2.0 {
        let dirx = wind.signum();
        draw_line(
            w * 0.5 - dirx * arrow_len * 0.5,
            54.0,
            w * 0.5 + dirx * arrow_len * 0.5,
            54.0,
            3.0,
            PARCHMENT,
        );
        let tipx = w * 0.5 + dirx * arrow_len * 0.5;
        draw_line(tipx, 54.0, tipx - dirx * 8.0, 49.0, 3.0, PARCHMENT);
        draw_line(tipx, 54.0, tipx - dirx * 8.0, 59.0, 3.0, PARCHMENT);
    }
    // Aim/power readout, bottom-left. While holding LMB the readout tracks
    // the live charge, not the wheel-set power.
    let shown_power = state.player.charging.unwrap_or(state.player.power);
    let readout = format!(
        "AIM {:.0}\u{00B0}   POWER {:.0}%",
        state.player.angle_deg,
        shown_power * 100.0
    );
    txt(&readout, 24.0, h - 28.0, 24, PARCHMENT, font);
    if let Some(c) = state.player.charging {
        draw_rectangle(24.0, h - 20.0, 200.0, 8.0, PARCHMENT);
        draw_rectangle(26.0, h - 18.0, 196.0 * c, 4.0, MID_HP);
    }
    // Last three shot ranges, bottom-right.
    if !state.last_ranges.is_empty() {
        let panel_w = 150.0;
        let panel_h = 24.0 + 20.0 * state.last_ranges.len() as f32;
        let x0 = w - 24.0 - panel_w;
        let y0 = h - 24.0 - panel_h;
        draw_rectangle(
            x0,
            y0,
            panel_w,
            panel_h,
            Color::new(PARCHMENT.r, PARCHMENT.g, PARCHMENT.b, 0.85),
        );
        txt("LAST SHOTS", x0 + 10.0, y0 + 18.0, 16, INK, font);
        for (i, r) in state.last_ranges.iter().enumerate() {
            txt(
                &format!("{r:.0} m"),
                x0 + 10.0,
                y0 + 40.0 + 20.0 * i as f32,
                18,
                INK,
                font,
            );
        }
    }
    draw_overlays(state, font);
}

fn draw_overlays(state: &GameState, font: Option<&macroquad::text::Font>) {
    let (w, h) = (screen_width(), screen_height());
    match state.phase {
        Phase::Menu => {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.1, 0.08, 0.14, 0.6));
            txt_centered("SIEGE!", w * 0.5, h * 0.42, 84, PARCHMENT, font);
            txt_centered("click to begin", w * 0.5, h * 0.52, 30, PARCHMENT, font);
            txt_centered(
                "aim: mouse \u{00B7} power: wheel / arrows \u{00B7} fire: hold LMB or space",
                w * 0.5,
                h * 0.60,
                22,
                Color::new(0.94, 0.9, 0.82, 0.8),
                font,
            );
            txt_centered(
                "P pause \u{00B7} R restart \u{00B7} destroy the keep before the defenders zero in",
                w * 0.5,
                h * 0.66,
                20,
                Color::new(0.94, 0.9, 0.82, 0.6),
                font,
            );
        }
        Phase::Paused => {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.1, 0.08, 0.14, 0.55));
            txt_centered("PAUSED", w * 0.5, h * 0.45, 64, PARCHMENT, font);
            txt_centered(
                "P to resume \u{00B7} R to restart",
                w * 0.5,
                h * 0.53,
                26,
                PARCHMENT,
                font,
            );
        }
        Phase::Victory | Phase::Defeat => {
            if state.end_t >= crate::game::END_HOLD {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.1, 0.08, 0.14, 0.6));
                let (msg, sub) = if state.phase == Phase::Victory {
                    (
                        "VICTORY!",
                        "the keep has fallen \u{2014} click to play again",
                    )
                } else {
                    (
                        "THE CASTLE STANDS",
                        "your cannon is destroyed \u{2014} click to try again",
                    )
                };
                let col = if state.phase == Phase::Victory {
                    PARCHMENT
                } else {
                    RED
                };
                txt_centered(msg, w * 0.5, h * 0.44, 72, col, font);
                txt_centered(sub, w * 0.5, h * 0.53, 26, PARCHMENT, font);
            }
        }
        Phase::Playing => {}
    }
}
