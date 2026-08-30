//! Living figures — defenders patrolling the wall tops and the player's
//! gun crew on the firing platform. Decorative only: fully parameterized
//! by `state.t`, the live ball list, and the segment table, so a wall
//! loses its runners the frame it falls.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use super::{scale, w2s};
use crate::fallers::Fallers;
use crate::game::GameState;
use crate::physics::V2;
use crate::world;
use macroquad::color::Color;
use macroquad::math::Vec2;
use macroquad::shapes::{draw_circle, draw_line, draw_rectangle};

/// Body silhouette (dark enough to read against stone at any scale).
const FIG: Color = Color::new(0.13, 0.11, 0.14, 1.0);
/// Steel helmet highlight.
const HELM: Color = Color::new(0.42, 0.45, 0.5, 1.0);
/// Defender livery — matches the keep roofs and banner.
const LIVERY: Color = Color::from_hex(0x8C_3B_2E);
/// Attacker crew leather.
const LEATHER: Color = Color::from_hex(0x5E_45_2C);

pub(super) fn draw(state: &GameState, shake: Vec2) {
    let off = shake;
    let w = |p: V2| w2s(p) + off;
    draw_wall_runners(state, &w);
    draw_fallers(&state.fallers, &w);
    draw_player_crew(state, &w);
}

/// One runner per ~5 m of tower/curtain wall top, ping-ponging along the
/// parapet. Any ball overhead inside 12 m makes them duck — the duck
/// factor tracks the ball continuously, so there is no popping.
fn draw_wall_runners(state: &GameState, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    for (ix, seg) in state.segments.iter().enumerate() {
        if !seg.alive() {
            continue;
        }
        let top = seg.y0 + seg.h;
        for k in 0..world::runner_count(seg) {
            // Layout lives in `world` so the fling on collapse spawns the
            // bodies exactly where the drawn runners stood.
            let (rx, dir, h1) = world::runner_state(seg, ix, k, state.t);
            let mut duck = 0.0_f32;
            for b in &state.balls {
                let dx = (b.pos.x - rx).abs();
                if dx < 12.0 {
                    let prox = 1.0 - dx / 12.0;
                    let elev = ((b.pos.y - top - 1.0) / 8.0).clamp(0.0, 1.0);
                    duck = duck.max(prox * elev);
                }
            }
            let moving = 1.0 - duck;
            let ph = state.t * 9.0 + h1 * std::f32::consts::TAU;
            let swing = ph.sin() * 0.16 * dir * moving;
            let bob = (ph * 0.5).sin().abs() * 0.06 * moving;
            let h = 1.3 * (1.0 - 0.5 * duck);
            draw_figure(w, s, rx, top + bob, h, swing, LIVERY);
            // Pike on the running side, lowered when ducking.
            let grip = w(V2 {
                x: rx + dir * 0.3,
                y: top + bob - h * 0.45,
            });
            let tip = w(V2 {
                x: rx + dir * (0.12 + 0.2 * duck),
                y: top + bob - h - 0.5 + duck * 0.75,
            });
            draw_line(grip.x, grip.y, tip.x, tip.y, (0.06 * s).max(1.5), FIG);
            draw_circle(tip.x, tip.y, (0.07 * s).max(1.5), HELM);
        }
    }
}

/// Two-man crew on the platform: the rammer works the bore while the
/// reload runs; the gunner raises a burning torch while the fuse burns
/// (mirroring the touch-hole ember on the cannon itself).
fn draw_player_crew(state: &GameState, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    let pivot = world::player_pivot();
    let gy = world::ground_height(pivot.x - 2.0);

    // Rammer.
    let ramming = state.player.reload > 0.0;
    let bob = if ramming {
        (state.t * 10.0).sin() * 0.1
    } else {
        0.0
    };
    let rx = pivot.x - 2.4;
    draw_figure(w, s, rx, gy, 1.25, 0.0, LEATHER);
    let hand = w(V2 {
        x: rx + 0.25,
        y: gy + 0.55 + bob,
    });
    let bore = w(V2 {
        x: rx + 1.5,
        y: gy + 0.75 - bob,
    });
    draw_line(hand.x, hand.y, bore.x, bore.y, (0.06 * s).max(1.5), LEATHER);

    // Gunner with the torch.
    let lit = state.player.fuse > 0.0;
    let gx = pivot.x - 3.9;
    draw_figure(w, s, gx, gy, 1.2, 0.0, LEATHER);
    let hand = w(V2 {
        x: gx + 0.2,
        y: if lit { gy + 1.05 } else { gy + 0.62 },
    });
    draw_line(
        w(V2 { x: gx, y: gy + 0.7 }).x,
        w(V2 { x: gx, y: gy + 0.7 }).y,
        hand.x,
        hand.y,
        (0.07 * s).max(1.5),
        FIG,
    );
    if lit {
        let flick = 0.6 + 0.4 * (state.t * 13.0).sin();
        draw_circle(
            hand.x,
            hand.y - 0.1 * s,
            (0.5 + 0.15 * flick) * s,
            Color::new(1.0, 0.55, 0.2, 0.16),
        );
        flame(
            w,
            s,
            V2 {
                x: gx + 0.2,
                y: gy + 1.25,
            },
            flick,
        );
    } else {
        draw_circle(
            hand.x,
            hand.y - 0.06 * s,
            (0.06 * s).max(1.5),
            Color::new(1.0, 0.42, 0.12, 0.85),
        );
    }
}

/// Layered flame blob at a world position (torch fire).
fn flame(w: &dyn Fn(V2) -> Vec2, s: f32, at: V2, flick: f32) {
    use macroquad::shapes::draw_ellipse;
    let pt = w(at);
    let (fx, fy) = (pt.x, pt.y - 0.1 * s);
    draw_ellipse(
        fx,
        fy,
        0.13 * s,
        (0.26 + 0.1 * flick) * s,
        0.0,
        Color::new(0.92, 0.38, 0.1, 0.9),
    );
    draw_ellipse(
        fx,
        fy + 0.03 * s,
        0.07 * s,
        (0.15 + 0.07 * flick) * s,
        0.0,
        Color::new(1.0, 0.82, 0.38, 0.95),
    );
}

/// Minimal ~1.3 m figure: two legs striding by `swing` metres, tunic
/// torso, helmeted head. All positions in world metres.
fn draw_figure(w: &dyn Fn(V2) -> Vec2, s: f32, x: f32, fy: f32, h: f32, swing: f32, tunic: Color) {
    let leg_h = h * 0.42;
    let torso_h = h * 0.42;
    let head_r = h * 0.1;
    for k in [-1.0_f32, 1.0] {
        let hip = w(V2 { x, y: fy - leg_h });
        let foot = w(V2 {
            x: x + k * swing,
            y: fy,
        });
        draw_line(hip.x, hip.y, foot.x, foot.y, (0.09 * s).max(1.5), FIG);
    }
    let tl = w(V2 {
        x: x - h * 0.13,
        y: fy - leg_h - torso_h,
    });
    draw_rectangle(tl.x, tl.y, h * 0.26 * s, torso_h * s, tunic);
    let head = w(V2 {
        x,
        y: fy - leg_h - torso_h - head_r * 0.6,
    });
    draw_circle(head.x, head.y, head_r * s, FIG);
    draw_circle(
        head.x - head_r * 0.25 * s,
        head.y - head_r * 0.3 * s,
        head_r * 0.55 * s,
        HELM,
    );
}

/// Flung defenders from `Fallers`: tumbling ragdolls that ease flat and
/// fade where they landed. Drawn after the live runners so the bodies
/// lay over the fresh rubble.
fn draw_fallers(fs: &Fallers, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    for f in &fs.list {
        let fade = (f.life / 0.8).clamp(0.0, 1.0);
        // At rest the body eases toward lying flat; mid-tumble it spins.
        let target = (f.ang / std::f32::consts::PI).round() * std::f32::consts::PI;
        let ang = f.ang + (target - f.ang) * (f.rest_t * 6.0).min(1.0);
        let body = V2 {
            x: ang.cos(),
            y: ang.sin(),
        };
        let fig = Color::new(FIG.r, FIG.g, FIG.b, fade);
        let liv = Color::new(LIVERY.r, LIVERY.g, LIVERY.b, fade);
        // Torso in livery, helmeted head along the body axis.
        let t0 = w(f.pos - body * 0.28);
        let t1 = w(f.pos + body * 0.28);
        draw_line(t0.x, t0.y, t1.x, t1.y, (0.13 * s).max(2.0), liv);
        let head = w(f.pos + body * 0.45);
        draw_circle(head.x, head.y, (0.11 * s).max(1.8), fig);
        draw_circle(
            head.x + body.x * 0.03 * s,
            head.y + body.y * 0.03 * s,
            (0.05 * s).max(1.0),
            Color::new(HELM.r, HELM.g, HELM.b, fade),
        );
        // Flailing limbs: legs from the hip end, arms from the shoulder
        // end, wiggling with the tumble phase.
        let wig = (f.life * 13.0).sin() * 0.25;
        for (along, off) in [
            (-0.22_f32, std::f32::consts::PI + 0.6 + wig),
            (-0.22_f32, std::f32::consts::PI - 0.6 + wig),
            (0.18_f32, 2.4 - wig),
            (0.18_f32, -2.4 - wig),
        ] {
            let limb = V2 {
                x: off.cos(),
                y: off.sin(),
            };
            let hip = w(f.pos + body * along);
            let tip = w(f.pos + body * along + limb * 0.34);
            draw_line(hip.x, hip.y, tip.x, tip.y, (0.08 * s).max(1.5), fig);
        }
    }
}
