//! Living wall patrols and gun crews. Decorative poses use simulation
//! time and projectile proximity only; collapsed segments lose their
//! guards immediately, with their ragdolls still drawn by `Fallers`.
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
use macroquad::shapes::{draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_triangle};

/// Body silhouette (dark enough to read against stone at any scale).
const FIG: Color = Color::new(0.13, 0.11, 0.14, 1.0);
/// Steel helmet highlight.
const HELM: Color = Color::new(0.42, 0.45, 0.5, 1.0);
/// Defender livery — matches the keep roofs and banner.
const LIVERY: Color = Color::from_hex(0x8C_3B_2E);
/// Ochre attacker coats, contrasting with the defender's red livery.
const LEATHER: Color = Color::from_hex(0xAD_79_38);
const SKIN: Color = Color::from_hex(0xE9_B4_7C);

pub(super) fn draw(state: &GameState, shake: Vec2) {
    let off = shake;
    let w = |p: V2| w2s(p) + off;
    draw_wall_runners(state, &w);
    draw_fallers(&state.fallers, &w);
    draw_player_crew(state, &w);
    draw_keep_crew(state, &w);
}

/// Smooth radial awareness: nothing outside six metres of the body.
fn proximity(delta: V2) -> f32 {
    let t = ((6.0 - delta.length()) / 4.5).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A short forward closest approach anticipates incoming rounds. The
/// newest trail segments provide a fading wake without mutable pose state.
fn threat(state: &GameState, foot: V2) -> f32 {
    let center = foot + V2 { x: 0.0, y: 1.0 };
    let mut duck = 0.0_f32;
    for ball in &state.balls {
        let delta = ball.pos - center;
        duck = duck.max(proximity(delta));
        let speed_sq = ball.vel.x * ball.vel.x + ball.vel.y * ball.vel.y;
        if speed_sq > 0.01 {
            let ahead =
                (-(delta.x * ball.vel.x + delta.y * ball.vel.y) / speed_sq).clamp(0.0, 0.35);
            let anticipation = 1.0 - ahead / 0.35;
            duck = duck.max(proximity(delta + ball.vel * ahead) * anticipation);
        }

        // Both work and memory are bounded. Distance along the wake, rather
        // than frame count, keeps recovery independent of rendering rate.
        let mut newer = ball.pos;
        let mut distance = 0.0;
        for &older in ball.trail.iter().rev().take(16) {
            let segment = older - newer;
            let length = segment.length();
            if length > 0.001 {
                let offset = center - newer;
                let along = ((offset.x * segment.x + offset.y * segment.y) / (length * length))
                    .clamp(0.0, 1.0);
                let age = ((distance + along * length) / 12.0).clamp(0.0, 1.0);
                let recovery = 1.0 - age * age * (3.0 - 2.0 * age);
                duck = duck.max(proximity(newer + segment * along - center) * recovery * 0.85);
            }
            distance += length;
            if distance >= 12.0 {
                break;
            }
            newer = older;
        }
    }
    duck
}

struct Figure {
    foot: V2,
    dir: f32,
    duck: f32,
    stride: f32,
    tunic: Color,
    /// Resting hand offsets, in facing-relative, y-up body coordinates.
    hands: [V2; 2],
}

/// Preserve the shared patrol position even while the upper body cowers:
/// collapse-spawned fallers must still agree with `world::runner_state`.
fn draw_wall_runners(state: &GameState, w: &dyn Fn(V2) -> Vec2) {
    let s = scale();
    for (ix, seg) in state.segments.iter().enumerate() {
        if !seg.alive() {
            continue;
        }
        let top = seg.y0 + seg.h;
        for k in 0..world::runner_count(seg) {
            let (rx, dir, seed) = world::runner_state(seg, ix, k, state.t);
            let foot = V2 { x: rx, y: top };
            let duck = threat(state, foot);
            let phase = state.t * 9.0 + seed * std::f32::consts::TAU;
            let stride = phase.sin() * 0.28 * (1.0 - duck);
            let figure = Figure {
                foot,
                dir,
                duck,
                stride,
                tunic: LIVERY,
                hands: [
                    V2 {
                        x: -0.22 - stride,
                        y: 0.97,
                    },
                    V2 { x: 0.4, y: 1.15 },
                ],
            };
            // The spear stays above the parapet and tilts outward to lower.
            let grip = hand_position(&figure, 1);
            let butt = grip
                + V2 {
                    x: -dir * (0.1 + duck * 0.35),
                    y: -0.45,
                };
            let tip = grip
                + V2 {
                    x: dir * (0.15 + duck * 1.1),
                    y: 1.6 - duck * 1.15,
                };
            stroke(w, s, butt, tip, 0.07, LEATHER);
            let p = w(tip);
            let left = w(tip + V2 { x: -0.12, y: -0.25 });
            let right = w(tip + V2 { x: 0.12, y: -0.25 });
            draw_triangle(p, left, right, HELM);
            draw_figure(w, s, &figure);
        }
    }
}

fn draw_player_crew(state: &GameState, w: &dyn Fn(V2) -> Vec2) {
    draw_crew(state, w, false);
}

fn draw_keep_crew(state: &GameState, w: &dyn Fn(V2) -> Vec2) {
    if state
        .segments
        .iter()
        .any(|seg| seg.kind == world::SegmentKind::Keep && seg.alive())
    {
        draw_crew(state, w, true);
    }
}

/// Both crews keep clear of the carriage, work their rammer during reload,
/// raise a fuse torch, and cover their heads during near misses or recoil.
fn draw_crew(state: &GameState, w: &dyn Fn(V2) -> Vec2, defending: bool) {
    let s = scale();
    let (pivot, dir, reload, fuse, recoil, tunic) = if defending {
        (
            world::defender_pivot(),
            -1.0,
            state.defender.reload_anim,
            state.defender.fuse,
            state.defender.recoil,
            LIVERY,
        )
    } else {
        (
            world::player_pivot(),
            1.0,
            state.player.reload,
            state.player.fuse,
            state.player.recoil,
            LEATHER,
        )
    };
    for k in 0..2 {
        let x = pivot.x - dir * (4.1 + k as f32 * 1.8);
        let foot = V2 {
            x,
            y: if defending {
                world::KEEP_TOP
            } else {
                world::ground_height(x)
            },
        };
        let phase = state.t * 3.0 + k as f32 * 2.1;
        let duck = threat(state, foot).max(recoil.clamp(0.0, 1.0) * 0.9);
        let work = (reload * 5.0).clamp(0.0, 1.0) * (state.t * 10.0).sin();
        let hands = if k == 0 {
            [
                V2 {
                    x: 0.15 + work * 0.12,
                    y: 1.02,
                },
                V2 {
                    x: 0.6 + work * 0.17,
                    y: 1.14,
                },
            ]
        } else {
            let raised = (fuse * 8.0).clamp(0.0, 1.0);
            [
                V2 {
                    x: -0.28,
                    y: 1.0 + phase.sin() * 0.04,
                },
                V2 {
                    x: 0.43,
                    y: 1.13 + raised * 0.4,
                },
            ]
        };
        let figure = Figure {
            foot,
            dir,
            duck,
            stride: phase.sin() * 0.045 * (1.0 - duck),
            tunic,
            hands,
        };
        draw_figure(w, s, &figure);
        draw_crew_tools(w, s, &figure, k == 0, fuse, state.t);
    }
}

fn draw_crew_tools(
    w: &dyn Fn(V2) -> Vec2,
    s: f32,
    figure: &Figure,
    rammer: bool,
    fuse: f32,
    time: f32,
) {
    let Figure { dir, duck, .. } = *figure;
    let grip = hand_position(figure, 1);
    if rammer {
        let start = grip
            + V2 {
                x: -dir * 0.65,
                y: -0.08,
            };
        let tip = grip
            + V2 {
                x: dir * (1.05 - duck * 0.45),
                y: 0.12 + duck * 0.3,
            };
        stroke(w, s, start, tip, 0.075, LEATHER);
        stroke(
            w,
            s,
            tip - V2 {
                x: dir * 0.15,
                y: 0.03,
            },
            tip,
            0.17,
            HELM,
        );
    } else {
        // Keep the hot end outside the helmet even while covering ears.
        let tip = grip
            + V2 {
                x: dir * (0.12 + duck * 0.35),
                y: 0.48,
            };
        stroke(w, s, grip, tip, 0.09, LEATHER);
        let flick = 0.6 + 0.4 * (time * 13.0).sin();
        if fuse > 0.0 {
            let glow = w(tip);
            draw_circle(
                glow.x,
                glow.y,
                (0.48 + 0.08 * flick) * s,
                Color::new(1.0, 0.55, 0.2, 0.14),
            );
            flame(w, s, tip, flick);
        } else {
            let ember = w(tip);
            draw_circle(
                ember.x,
                ember.y,
                0.065 * s,
                Color::new(1.0, 0.42, 0.12, 0.85),
            );
        }
    }
}

/// Layered flame blob at a world position (torch fire).
fn flame(w: &dyn Fn(V2) -> Vec2, s: f32, at: V2, flick: f32) {
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

fn stroke(w: &dyn Fn(V2) -> Vec2, s: f32, from: V2, to: V2, width: f32, color: Color) {
    let a = w(from);
    let b = w(to);
    draw_line(a.x, a.y, b.x, b.y, (width * s).max(1.0), color);
}

fn hand_position(figure: &Figure, index: usize) -> V2 {
    let side = if index == 0 { -1.0 } else { 1.0 };
    let cover = V2 {
        x: side * 0.24,
        y: 1.88 - 0.87 * figure.duck,
    };
    let hand = figure.hands[index] * (1.0 - figure.duck) + cover * figure.duck;
    figure.foot
        + V2 {
            x: hand.x * figure.dir,
            y: hand.y,
        }
}

/// Two-metre figures with planted boots, bending knees, colored coats and
/// helmeted faces. Every world-space height is positive above the floor.
fn draw_figure(w: &dyn Fn(V2) -> Vec2, s: f32, figure: &Figure) {
    let Figure {
        foot,
        dir,
        duck,
        tunic,
        ..
    } = *figure;
    let hip_y = 0.83 - 0.39 * duck;
    let shoulder_y = 1.49 - 0.78 * duck;
    let head_y = 1.77 - 0.86 * duck;
    let lean = -dir * duck * 0.12;
    let base = w(foot);
    draw_ellipse(
        base.x,
        base.y,
        0.42 * s,
        0.08 * s,
        0.0,
        Color::new(0.08, 0.07, 0.07, 0.2),
    );
    draw_figure_legs(w, s, figure, hip_y, lean);
    let hip = foot + V2 { x: lean, y: hip_y };
    let shoulder = foot
        + V2 {
            x: lean,
            y: shoulder_y,
        };
    stroke(w, s, hip, shoulder, 0.5, FIG);
    stroke(w, s, hip, shoulder, 0.4, tunic);
    let belt = hip + V2 { x: 0.0, y: 0.09 };
    stroke(
        w,
        s,
        belt - V2 { x: 0.22, y: 0.0 },
        belt + V2 { x: 0.22, y: 0.0 },
        0.09,
        FIG,
    );
    let buckle = w(belt
        + V2 {
            x: dir * 0.09,
            y: 0.0,
        });
    draw_rectangle(
        buckle.x - 0.035 * s,
        buckle.y - 0.04 * s,
        0.07 * s,
        0.08 * s,
        Color::from_hex(0xD6_B4_63),
    );

    let head = w(foot
        + V2 {
            x: lean + dir * 0.035,
            y: head_y,
        });
    draw_figure_head(s, head, dir);

    // Arms are drawn last so the palms visibly cover the helmet on alarm.
    draw_figure_arms(w, s, figure, shoulder, shoulder_y);
    if duck > 0.35 {
        let alpha = ((duck - 0.35) / 0.65).clamp(0.0, 1.0) * 0.75;
        let alarm = Color::new(1.0, 0.83, 0.49, alpha);
        for side in [-1.0_f32, 1.0] {
            let from = foot
                + V2 {
                    x: lean + side * 0.39,
                    y: head_y + 0.32,
                };
            stroke(
                w,
                s,
                from,
                from + V2 {
                    x: side * 0.12,
                    y: 0.18,
                },
                0.05,
                alarm,
            );
        }
    }
}

fn draw_figure_arms(
    w: &dyn Fn(V2) -> Vec2,
    s: f32,
    figure: &Figure,
    shoulder: V2,
    shoulder_y: f32,
) {
    let Figure {
        foot, duck, tunic, ..
    } = *figure;
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let root = shoulder
            + V2 {
                x: side * 0.19,
                y: -0.06,
            };
        let hand = hand_position(figure, index);
        let elbow = foot
            + V2 {
                x: (root.x + hand.x) * 0.5 - foot.x + side * duck * 0.24,
                y: (shoulder_y + figure.hands[index].y) * 0.5 - 0.16 - duck * 0.38,
            };
        stroke(w, s, root, elbow, 0.15, FIG);
        stroke(w, s, root, elbow, 0.11, tunic);
        stroke(w, s, elbow, hand, 0.11, SKIN);
        let p = w(hand);
        draw_circle(p.x, p.y, 0.075 * s, SKIN);
    }
}

fn draw_figure_legs(w: &dyn Fn(V2) -> Vec2, s: f32, figure: &Figure, hip_y: f32, lean: f32) {
    let Figure {
        foot,
        dir,
        duck,
        stride,
        ..
    } = *figure;
    for side in [-1.0_f32, 1.0] {
        let hip = foot
            + V2 {
                x: lean + side * 0.1,
                y: hip_y,
            };
        let boot = foot
            + V2 {
                x: side * (0.17 + stride) * dir,
                y: 0.085,
            };
        let knee = foot
            + V2 {
                x: side * 0.18 + dir * duck * 0.22,
                y: hip_y * 0.52,
            };
        stroke(w, s, hip, knee, 0.14, FIG);
        stroke(w, s, knee, boot, 0.13, FIG);
        stroke(
            w,
            s,
            boot - V2 {
                x: dir * 0.04,
                y: 0.0,
            },
            boot + V2 {
                x: dir * 0.16,
                y: 0.0,
            },
            0.16,
            FIG,
        );
    }
}

fn draw_figure_head(s: f32, head: Vec2, dir: f32) {
    draw_circle(head.x, head.y, 0.255 * s, FIG);
    draw_circle(head.x + dir * 0.045 * s, head.y + 0.01 * s, 0.205 * s, SKIN);
    draw_circle(head.x + dir * 0.23 * s, head.y + 0.02 * s, 0.065 * s, SKIN);
    draw_circle(
        head.x + dir * 0.145 * s,
        head.y - 0.015 * s,
        (0.03 * s).max(0.7),
        FIG,
    );
    draw_ellipse(
        head.x - 0.025 * dir * s,
        head.y - 0.135 * s,
        0.265 * s,
        0.155 * s,
        0.0,
        HELM,
    );
    draw_line(
        head.x - 0.31 * s,
        head.y - 0.065 * s,
        head.x + 0.31 * s,
        head.y - 0.065 * s,
        (0.07 * s).max(1.0),
        FIG,
    );
    draw_line(
        head.x - 0.15 * s,
        head.y - 0.2 * s,
        head.x + 0.07 * s,
        head.y - 0.23 * s,
        (0.035 * s).max(0.7),
        Color::from_hex(0xAF_BB_B8),
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

#[cfg(test)]
mod tests {
    use super::threat;
    use crate::game::GameState;
    use crate::physics::{Ball, Side, V2};
    use crate::rng::Rng;
    use std::collections::VecDeque;

    fn scene_with_ball(pos: V2, vel: V2) -> GameState {
        let mut state = GameState::new(Rng::seed(1));
        state.balls.push(Ball {
            pos,
            vel,
            side: Side::Player,
            trail: VecDeque::new(),
            spin: 0.0,
            whistled: false,
        });
        state
    }

    #[test]
    fn near_misses_trigger_cover_but_high_overhead_shots_do_not() {
        let foot = V2 { x: 20.0, y: 10.0 };
        let mut state = scene_with_ball(V2 { x: 20.0, y: 12.0 }, V2 { x: -40.0, y: 0.0 });
        assert!(
            threat(&state, foot) > 0.9,
            "a head-height shot should make the guard cover"
        );

        state.balls[0].pos.y = 40.0;
        assert!(
            threat(&state, foot) < f32::EPSILON,
            "horizontal proximity alone must not alarm the guard"
        );
    }

    #[test]
    fn incoming_shots_warn_then_wake_releases_cover() {
        let foot = V2 { x: 20.0, y: 10.0 };
        let mut state = scene_with_ball(V2 { x: 29.0, y: 11.0 }, V2 { x: -60.0, y: 0.0 });
        assert!(
            threat(&state, foot) > 0.5,
            "an imminent close approach should warn before entering the radius"
        );
        state.balls[0].vel.x = 60.0;
        assert!(
            threat(&state, foot) < f32::EPSILON,
            "a receding shot without a nearby wake is not an incoming threat"
        );

        state.balls[0].pos.x = 26.0;
        state.balls[0].trail = VecDeque::from([V2 { x: 20.0, y: 11.0 }, V2 { x: 23.0, y: 11.0 }]);
        let just_passed = threat(&state, foot);
        assert!(
            just_passed > 0.4,
            "the close wake should briefly preserve cover after passage"
        );

        let previous = state.balls[0].pos;
        state.balls[0].trail.push_back(previous);
        state.balls[0].pos.x = 30.0;
        let recovering = threat(&state, foot);
        assert!(
            recovering > 0.0 && recovering < just_passed,
            "cover should relax as the wake recedes"
        );

        let previous = state.balls[0].pos;
        state.balls[0].trail.push_back(previous);
        state.balls[0].trail.push_back(V2 { x: 35.0, y: 11.0 });
        state.balls[0].pos.x = 40.0;
        assert!(
            threat(&state, foot) < f32::EPSILON,
            "an old close pass must not leave the guard cowering"
        );
    }
}
