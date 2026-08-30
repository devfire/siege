//! HUD & overlays — health bars, wind gauge, charge readout, last-shot
//! panel, pause hint, and the victory/defeat overlays.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use crate::game::{GameState, Phase};
use crate::world::SegmentKind;
use macroquad::color::Color;
use macroquad::shapes::{draw_line, draw_rectangle};
use macroquad::text::{TextDimensions, TextParams, draw_text_ex, measure_text};
use macroquad::window::{screen_height, screen_width};

const PARCHMENT: Color = Color::from_hex(0xF0_E6_D2);
const INK: Color = Color::from_hex(0x2E_26_20);
const RED: Color = Color::from_hex(0xC2_3B_2E);
const GOOD_HP: Color = Color::from_hex(0x6F_A3_5C);
const MID_HP: Color = Color::from_hex(0xD9_A4_41);
const LOW_HP: Color = Color::from_hex(0xC2_3B_2E);
const DEAD_HP: Color = Color::from_hex(0x3A_36_34);

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

fn hp_color(f: f32) -> Color {
    if f > 0.66 {
        GOOD_HP
    } else if f > 0.33 {
        MID_HP
    } else {
        LOW_HP
    }
}

pub(super) fn draw_ui(state: &GameState, font: Option<&macroquad::text::Font>) {
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
    let wind = state.wind.current();
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
    // Aim/charge readout, bottom-left.
    let readout = format!(
        "AIM {:.0}\u{00B0}   CHARGE {:.0}%",
        state.player.angle_deg,
        state.player.charge * 100.0
    );
    txt(&readout, 24.0, h - 28.0, 24, PARCHMENT, font);
    draw_rectangle(24.0, h - 20.0, 200.0, 8.0, PARCHMENT);
    draw_rectangle(26.0, h - 18.0, 196.0 * state.player.charge, 4.0, MID_HP);
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
                "aim: mouse · charge: wheel · fire: click LMB or space",
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
