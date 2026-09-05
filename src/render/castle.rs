//! Castle pass — keep, towers, gate, crenellations, arrow slits, roofs,
//! banners, cracks, and rubble piles.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use super::{IRON, STONE, darken, hash2, mix, scale, w2s};
use crate::game::GameState;
use crate::physics::V2;
use crate::world;
use crate::world::SegmentKind;
use macroquad::color::Color;
use macroquad::math::Vec2;
use macroquad::shapes::{draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_triangle};

const MORTAR: Color = Color::from_hex(0x6E_67_5F);
const KEEP_C: Color = Color::from_hex(0x94_90_8B);
const WOOD: Color = Color::from_hex(0x7A_4E_2D);
const ROOF_C: Color = Color::from_hex(0x8C_3B_2E);
const BANNER: Color = Color::from_hex(0xC2_3B_2E);
const RUBBLE: Color = Color::from_hex(0x5C_58_54);

pub(super) fn draw_castle(state: &GameState, shake: Vec2) {
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
                // Dawn key from the left (sun at x=60): sunlit left edge,
                // shaded right edge, hot top rim, AO pooling at the base.
                // Four thin overlays turn the flat slabs into lit volumes.
                let edge = 0.35 * s;
                draw_rectangle(
                    tl.x,
                    tl.y,
                    edge,
                    seg.h * s,
                    Color::new(1.0, 0.85, 0.62, 0.20),
                );
                draw_rectangle(
                    tl.x + seg.w * s - edge,
                    tl.y,
                    edge,
                    seg.h * s,
                    Color::new(0.08, 0.06, 0.12, 0.28),
                );
                draw_rectangle(
                    tl.x,
                    tl.y,
                    seg.w * s,
                    (0.22 * s).max(1.5),
                    Color::new(1.0, 0.9, 0.7, 0.30),
                );
                let ao_h = (seg.h * s * 0.22).min(2.2 * s);
                draw_rectangle(
                    tl.x,
                    tl.y + seg.h * s - ao_h,
                    seg.w * s,
                    ao_h,
                    Color::new(0.05, 0.04, 0.08, 0.30),
                );
                match seg.kind {
                    SegmentKind::Gate => draw_gate_planks(seg.x0, seg.y0, seg.w, seg.h, &w),
                    _ => draw_stone(seg.x0, seg.y0, seg.w, seg.h, seed, &w),
                }
                if seg.kind != SegmentKind::Gate {
                    draw_crenellations(seg.x0, seg.y0 + seg.h, seg.w, &w);
                }
                if seg.kind == SegmentKind::Keep {
                    draw_keep_windows(seg.x0, seg.y0, seg.w, seg.h, state.t, &w);
                    draw_keep_roof(state, seg.x0, seg.w, seg.y0 + seg.h, &w);
                } else {
                    draw_arrow_slits(seg.x0, seg.y0, seg.w, seg.h, seed, &w);
                }
                if seg.kind == SegmentKind::Tower {
                    draw_pennant(seg.x0 + seg.w * 0.5, seg.y0 + seg.h, state.t, seed, &w);
                }
                if seg.kind == SegmentKind::Gate {
                    draw_gate_torches(seg.x0, seg.y0, seg.w, seg.h, state.t, seed, &w);
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

/// Two lit arched windows on the keep body plus a round loft window,
/// with warm candle flicker from `t`.
fn draw_keep_windows(x0: f32, y0: f32, width: f32, height: f32, t: f32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let cx = x0 + width * 0.5;
    let frame = Color::new(0.14, 0.12, 0.1, 1.0);
    let arched = |wx: f32, wy: f32, k: f32| {
        let flick = 0.72 + 0.28 * (t * 6.0 + k * 2.1).sin();
        let glow = Color::new(1.0, 0.7, 0.32, 0.35 + 0.4 * flick);
        let tl = w(V2 {
            x: wx - 0.5,
            y: wy + 1.8,
        });
        draw_rectangle(tl.x, tl.y, 1.0 * s, 1.8 * s, frame);
        let arch = w(V2 { x: wx, y: wy + 1.8 });
        draw_circle(arch.x, arch.y, 0.5 * s, frame);
        let tl = w(V2 {
            x: wx - 0.32,
            y: wy + 1.55,
        });
        draw_rectangle(tl.x, tl.y, 0.64 * s, 1.35 * s, glow);
        let arch = w(V2 {
            x: wx,
            y: wy + 1.55,
        });
        draw_circle(arch.x, arch.y, 0.32 * s, glow);
    };
    arched(cx - 3.2, y0 + height * 0.35, 1.0);
    arched(cx + 3.2, y0 + height * 0.35, 2.0);
    let flick = 0.7 + 0.3 * (t * 5.0).sin();
    let c = w(V2 {
        x: cx,
        y: y0 + height * 0.78,
    });
    draw_circle(c.x, c.y, 0.45 * s, frame);
    draw_circle(
        c.x,
        c.y,
        0.28 * s,
        Color::new(1.0, 0.7, 0.32, 0.3 + 0.4 * flick),
    );
}

/// Slim waving pennant on an iron pole above a tower's merlon line.
fn draw_pennant(cx: f32, top: f32, t: f32, seed: u32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let base = w(V2 {
        x: cx,
        y: top + 0.9,
    });
    let tip = w(V2 {
        x: cx,
        y: top + 3.6,
    });
    draw_line(base.x, base.y, tip.x, tip.y, (0.07 * s).max(1.5), IRON);
    let flutter = (t * 4.2 + seed as f32).sin();
    let f0 = w(V2 {
        x: cx,
        y: top + 3.5,
    });
    let f1 = w(V2 {
        x: cx,
        y: top + 3.5 - 0.85,
    });
    let f2 = w(V2 {
        x: cx + 1.7 + flutter * 0.25,
        y: top + 3.5 - 0.42 + flutter * 0.18,
    });
    draw_triangle(f0, f1, f2, BANNER);
}

/// Braziers flanking the gate: pole, basket, layered flame, warm halo on
/// the wall behind.
fn draw_gate_torches(
    x0: f32,
    y0: f32,
    width: f32,
    height: f32,
    t: f32,
    seed: u32,
    w: &dyn Fn(V2) -> Vec2,
) {
    let s = scale();
    for k in 0..2u32 {
        let tx = if k == 0 { x0 + 0.5 } else { x0 + width - 0.5 };
        let ty = y0 + height * 0.72;
        let pole = w(V2 { x: tx, y: ty });
        let ground = w(V2 { x: tx, y: ty - 2.2 });
        draw_line(
            ground.x,
            ground.y,
            pole.x,
            pole.y,
            (0.08 * s).max(1.5),
            darken(WOOD, 0.2),
        );
        let flick = 0.65 + 0.35 * (t * (9.0 + 3.0 * hash2(seed, k))).sin();
        draw_circle(
            pole.x,
            pole.y - 0.1 * s,
            (1.3 + 0.3 * flick) * s,
            Color::new(1.0, 0.6, 0.25, 0.1),
        );
        draw_ellipse(
            pole.x,
            pole.y - 0.25 * s,
            0.16 * s,
            (0.3 + 0.12 * flick) * s,
            0.0,
            Color::new(0.92, 0.38, 0.1, 0.9),
        );
        draw_ellipse(
            pole.x,
            pole.y - 0.2 * s,
            0.09 * s,
            (0.18 + 0.08 * flick) * s,
            0.0,
            Color::new(1.0, 0.82, 0.38, 0.95),
        );
        draw_rectangle(pole.x - 0.18 * s, pole.y, 0.36 * s, 0.22 * s, IRON);
    }
}

fn draw_keep_roof(state: &GameState, x0: f32, width: f32, top: f32, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    // A narrow rear watch-turret leaves the foreground gun deck open.
    let cx = x0 + width * 0.8;
    let apex = w(V2 {
        x: cx,
        y: top + 6.5,
    });
    let left = w(V2 {
        x: cx - width * 0.23,
        y: top,
    });
    let right = w(V2 {
        x: cx + width * 0.23,
        y: top,
    });
    draw_triangle(left, apex, right, ROOF_C);
    draw_triangle(left, apex, w(V2 { x: cx, y: top }), darken(ROOF_C, 0.18));
    let deck = w(V2 {
        x: x0 - 0.4,
        y: top + 0.08,
    });
    draw_rectangle(
        deck.x,
        deck.y,
        (width + 0.8) * s,
        0.38 * s,
        darken(STONE, 0.2),
    );
    draw_line(
        deck.x,
        deck.y,
        deck.x + (width + 0.8) * s,
        deck.y,
        (0.12 * s).max(1.0),
        Color::from_hex(0xD8_C5_A2),
    );
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
