//! Audio — every sound synthesized at startup into in-memory WAVs; no
//! binary assets. Cannons, impacts, rubble, fuses, stings, and a wind bed
//! whose volume tracks the live wind speed. Load failures degrade to
//! silence (`Option<Sound>` + no-op `play`), so the game runs where the
//! audio backend is missing (headless CI, blocked wasm autoplay).
//! Starts muted (meetings!); the HUD button or the M key toggles via
//! [`Audio::toggle_mute`].
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::rng::Rng;
use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, play_sound_once, set_sound_volume,
};

const RATE: u32 = 22_050;

/// 16-bit mono PCM WAV header + samples.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// One-pole low-pass state for shaping noise bursts.
struct Lp {
    y: f32,
    k: f32, // 0..1: higher passes more
}

impl Lp {
    fn new(k: f32) -> Self {
        Self { y: 0.0, k }
    }
    fn next(&mut self, x: f32) -> f32 {
        self.y += self.k * (x - self.y);
        self.y
    }
}

/// Muzzle blast: sine sweep `f0`→`f1` under a muffled noise thump, with a
/// sharp crack in the first 30 ms.
fn synth_boom(len: f32, f0: f32, f1: f32, noise: f32, lp_k: f32, gain: f32) -> Vec<i16> {
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0xB00);
    let mut lp = Lp::new(lp_k);
    let mut phase = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (-t * 4.5_f32).exp();
        let f = f0 * (f1 / f0).powf(t / len);
        phase += std::f32::consts::TAU * f / RATE as f32;
        let body = phase.sin() * env * 0.9;
        let thump = lp.next(rng.f01() * 2.0 - 1.0) * env * env * noise;
        let crack = (rng.f01() * 2.0 - 1.0) * (-t * 90.0).exp() * 0.6;
        out.push((((body + thump + crack) * gain).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
    }
    out
}

/// Stone impact: deeper sweep, heavier gravel, slower decay.
fn synth_impact() -> Vec<i16> {
    let len = 1.3_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0x1712);
    let mut lp = Lp::new(0.18);
    let mut phase = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (-t * 3.0_f32).exp();
        let f = 95.0_f32 * (30.0_f32 / 95.0).powf(t / len);
        phase += std::f32::consts::TAU * f / RATE as f32;
        // Gravel: gated noise chunks, ~40 Hz gate.
        let gate = if (t * 40.0).sin().abs() > 0.45 {
            1.0
        } else {
            0.25
        };
        let gravel = lp.next(rng.f01() * 2.0 - 1.0) * env * gate * 0.9;
        out.push(
            (((phase.sin() * env * 0.75 + gravel) * 0.9).clamp(-1.0, 1.0) * f32::from(i16::MAX))
                as i16,
        );
    }
    out
}

/// Segment collapse: hissing gravel slide with three settling thuds.
fn synth_crumble() -> Vec<i16> {
    let len = 1.6_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0xC0F);
    let mut lp = Lp::new(0.3);
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (-t * 2.2_f32).exp();
        // Sparse heavy grains instead of uniform hiss.
        let grain = (rng.f01() * 2.0 - 1.0).powi(3);
        let slide = lp.next(grain) * env * 1.5;
        let mut thud = 0.0_f32;
        for (at, amp) in [(0.08_f32, 0.4_f32), (0.34, 0.3), (0.72, 0.22)] {
            if t >= at {
                let dt = t - at;
                thud += (-70.0 * dt).sin() * (-dt * 18.0).exp() * amp;
            }
        }
        out.push((((slide + thud) * 0.8).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
    }
    out
}

/// Touch-hole fuse: bright hiss with sparse crackle pops.
fn synth_fuse() -> Vec<i16> {
    let len = 0.55_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0xF55);
    let mut prev = 0.0_f32;
    let mut pop = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = 1.0 - t / len;
        let w = rng.f01() * 2.0 - 1.0;
        let hiss = (w - prev * 0.8) * env * 0.35; // crudely high-passed
        prev = w;
        if rng.f01() > 0.996 {
            pop = 0.7;
        }
        pop *= 0.9;
        out.push(
            (((hiss + pop * (rng.f01() * 2.0 - 1.0)) * 0.8).clamp(-1.0, 1.0) * f32::from(i16::MAX))
                as i16,
        );
    }
    out
}

/// Falling-ball whistle: descending exponential sweep with vibrato and a
/// quadratic swell — the incoming-shot warning, fired on the final
/// stretch of a ball's descent.
fn synth_whistle() -> Vec<i16> {
    let len = 1.5_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let f = 1_500.0 * (380.0 / 1_500.0_f32).powf(t / len)
            + 35.0 * (std::f32::consts::TAU * 19.0 * t).sin();
        phase += std::f32::consts::TAU * f / RATE as f32;
        let swell = (t / len).powi(2);
        out.push(((phase.sin() * swell * 0.85).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
    }
    out
}

/// Earth thud for ground impacts: a muffled body sweep under soft dirt
/// grains — clearly softer and duller than the stone crack.
fn synth_thud() -> Vec<i16> {
    let len = 0.55_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0x70D);
    let mut lp = Lp::new(0.07);
    let mut phase = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (-t * 7.0_f32).exp();
        let f = 82.0 * (26.0_f32 / 82.0).powf(t / len);
        phase += std::f32::consts::TAU * f / RATE as f32;
        let body = phase.sin() * env * 0.9;
        let dirt = lp.next((rng.f01() * 2.0 - 1.0).powi(3)) * env * 3.0;
        out.push((((body + dirt) * 0.85).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
    }
    out
}

/// UI tick for menu / restart / pause: a dry wooden knock.
fn synth_click() -> Vec<i16> {
    let len = 0.07_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Rng::seed(0xC11);
    let mut phase = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let f = 1_050.0 * (480.0 / 1_050.0_f32).powf(t / len);
        phase += std::f32::consts::TAU * f / RATE as f32;
        let body = phase.sin() * (-t * 55.0).exp() * 0.85;
        let tap = (rng.f01() * 2.0 - 1.0) * (-t * 400.0).exp() * 0.3;
        out.push((((body + tap) * 0.9).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
    }
    out
}

/// Ambient birdsong bed: 16 sparse FM-warbled chirps scattered over 9 s.
/// Both seam margins stay silent, so the loop point is inaudible.
fn synth_birdsong() -> Vec<i16> {
    let len = 9.0_f32;
    let n = (len * RATE as f32) as usize;
    let mut out = vec![0.0_f32; n];
    let mut rng = Rng::seed(0xB1D);
    for _ in 0..16 {
        let start = (rng.range(0.4, 8.2) * RATE as f32) as usize;
        let dur = rng.range(0.05, 0.14);
        let m = (dur * RATE as f32) as usize;
        let fc = rng.range(2_300.0, 4_300.0);
        let depth = rng.range(200.0, 650.0);
        let fm = rng.range(14.0, 38.0);
        let amp = rng.range(0.35, 1.0);
        let mut phase = 0.0_f32;
        for i in 0..m.min(n - start) {
            let t = i as f32 / RATE as f32;
            phase += std::f32::consts::TAU * (fc + depth * (std::f32::consts::TAU * fm * t).sin())
                / RATE as f32;
            let env = (std::f32::consts::PI * t / dur).sin();
            out[start + i] += phase.sin() * env * amp;
        }
    }
    out.iter()
        .map(|&s| ((s * 0.45).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

/// Note arpeggio; each note is a sine + soft octave with a percussive
/// envelope. `step` is the note spacing, notes are `(freq, dur)`.
fn synth_sting(notes: &[(f32, f32)], step: f32) -> Vec<i16> {
    let total = step * notes.len() as f32 + 0.6;
    let n = (total * RATE as f32) as usize;
    let mut out = vec![0.0_f32; n];
    for (k, &(f, dur)) in notes.iter().enumerate() {
        let start = (step * k as f32 * RATE as f32) as usize;
        let len = (dur * RATE as f32) as usize;
        let mut phase = 0.0_f32;
        for i in 0..len.min(n - start) {
            let t = i as f32 / RATE as f32;
            phase += std::f32::consts::TAU * f / RATE as f32;
            let env = (-t * 5.0_f32).exp() * (t * 200.0).min(1.0);
            out[start + i] += (phase.sin() * 0.8 + (phase * 2.0).sin() * 0.2) * env * 0.5;
        }
    }
    out.iter()
        .map(|&s| ((s * 0.9).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

/// Seamless wind bed: brown noise, amplitude-modulated at multiples of
/// the loop frequency, with the tail crossfaded into the head.
fn synth_wind() -> Vec<i16> {
    let len = 5.0_f32;
    let n = (len * RATE as f32) as usize;
    let mut rng = Rng::seed(0x11D);
    let mut brown = 0.0_f32;
    let mut out = vec![0.0_f32; n];
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        brown += (rng.f01() * 2.0 - 1.0) * 0.03;
        brown *= 0.997;
        // Modulation at k/len Hz keeps the loop periodic.
        let swell = 0.65 + 0.35 * (std::f32::consts::TAU * 2.0 * t / len).sin();
        *slot = brown * 6.0 * swell;
    }
    let xf = (0.4_f32 * RATE as f32) as usize; // 0.4 s crossfade
    for i in 0..xf {
        let a = i as f32 / xf as f32;
        out[n - xf + i] = out[n - xf + i] * (1.0 - a) + out[i] * a;
    }
    let peak = out.iter().fold(0.0_f32, |m, &s| m.max(s.abs())).max(1e-6);
    out.iter()
        .map(|&s| ((s / peak * 0.8).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

pub struct Audio {
    boom: Option<Sound>,
    boom_far: Option<Sound>,
    impact: Option<Sound>,
    crumble: Option<Sound>,
    fuse: Option<Sound>,
    victory: Option<Sound>,
    defeat: Option<Sound>,
    wind: Option<Sound>,
    whistle: Option<Sound>,
    thud: Option<Sound>,
    click: Option<Sound>,
    birds: Option<Sound>,
    /// Master mute. Starts ON so the game opens silent; gates one-shots
    /// and drives the ambient beds to 0.
    muted: bool,
    /// Last wind volume pushed to the mixer; gates per-frame FFI calls.
    wind_vol: f32,
    /// Same gating for the birdsong bed.
    birds_vol: f32,
}

impl Audio {
    /// Synthesize one buffer into a loaded sound (None on decode failure).
    async fn load(samples: Vec<i16>) -> Option<Sound> {
        load_sound_from_bytes(&wav(&samples)).await.ok()
    }

    /// Synthesize + load. Individual failures leave that sound silent.
    #[must_use]
    pub async fn new() -> Self {
        let boom = Self::load(synth_boom(0.9, 115.0, 36.0, 0.55, 0.25, 0.9)).await;
        let boom_far = Self::load(synth_boom(1.1, 90.0, 30.0, 0.4, 0.12, 0.6)).await;
        let impact = Self::load(synth_impact()).await;
        let crumble = Self::load(synth_crumble()).await;
        let fuse = Self::load(synth_fuse()).await;
        let victory = Self::load(synth_sting(
            &[(392.0, 0.5), (523.25, 0.5), (659.25, 0.5), (783.99, 0.9)],
            0.16,
        ))
        .await;
        let defeat = Self::load(synth_sting(
            &[(329.63, 0.6), (293.66, 0.6), (246.94, 0.6), (196.0, 1.2)],
            0.24,
        ))
        .await;
        let wind = Self::load(synth_wind()).await;
        let whistle = Self::load(synth_whistle()).await;
        let thud = Self::load(synth_thud()).await;
        let click = Self::load(synth_click()).await;
        let birds = Self::load(synth_birdsong()).await;
        if let Some(w) = &wind {
            play_sound(
                w,
                PlaySoundParams {
                    looped: true,
                    volume: 0.0,
                },
            );
        }
        if let Some(b) = &birds {
            play_sound(
                b,
                PlaySoundParams {
                    looped: true,
                    volume: 0.0,
                },
            );
        }
        Self {
            boom,
            boom_far,
            impact,
            crumble,
            fuse,
            victory,
            defeat,
            wind,
            whistle,
            thud,
            click,
            birds,
            muted: true,
            wind_vol: -1.0,
            birds_vol: -1.0,
        }
    }

    fn play(&self, s: Option<&Sound>, volume: f32) {
        if self.muted {
            return;
        }
        if let Some(s) = s {
            play_sound_once(s);
            // play_sound_once restarts at the stored volume; re-apply ours.
            set_sound_volume(s, volume);
        }
    }

    /// Flip the master mute. Muting also zeroes anything still ringing
    /// so the cut is instant; unmuting re-arms the ambient beds — their
    /// volumes re-push on the next `set_wind`/`set_birds`.
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.wind_vol = -1.0;
        self.birds_vol = -1.0;
        if self.muted {
            for s in [
                &self.boom,
                &self.boom_far,
                &self.impact,
                &self.crumble,
                &self.fuse,
                &self.victory,
                &self.defeat,
                &self.wind,
                &self.whistle,
                &self.thud,
                &self.click,
                &self.birds,
            ]
            .into_iter()
            .flatten()
            {
                set_sound_volume(s, 0.0);
            }
        }
    }

    /// Master-mute state — the HUD button label reads it.
    #[must_use]
    pub fn muted(&self) -> bool {
        self.muted
    }

    /// Player cannon blast (full presence).
    pub fn boom_near(&self) {
        self.play(self.boom.as_ref(), 0.9);
    }

    /// Defender cannon blast, thinned by distance across the field.
    pub fn boom_far(&self) {
        self.play(self.boom_far.as_ref(), 0.45);
    }

    /// Ball impact. `near` 0..1 scales presence by distance to the
    /// player; `ground` picks the earth thud over the stone crack.
    pub fn impact(&self, near: f32, ground: bool) {
        if ground {
            self.play(self.thud.as_ref(), 0.45 + 0.4 * near);
        } else {
            self.play(self.impact.as_ref(), 0.45 + 0.5 * near);
        }
    }

    /// Falling-ball warning, fired once per ball on its final descent.
    pub fn whistle(&self, volume: f32) {
        self.play(self.whistle.as_ref(), volume);
    }

    /// UI tick for menu / restart / pause.
    pub fn click(&self) {
        self.play(self.click.as_ref(), 0.5);
    }

    pub fn crumble(&self, near: f32) {
        self.play(self.crumble.as_ref(), 0.35 + 0.45 * near);
    }

    pub fn fuse(&self, near: bool) {
        self.play(self.fuse.as_ref(), if near { 0.5 } else { 0.22 });
    }

    pub fn victory(&self) {
        self.play(self.victory.as_ref(), 0.8);
    }

    pub fn defeat(&self) {
        self.play(self.defeat.as_ref(), 0.8);
    }

    /// Ride the ambient bed on the live wind (m/s, ±14). Only touches the
    /// mixer when the target moved by > 0.01.
    pub fn set_wind(&mut self, wind_ms: f32) {
        let target = if self.muted {
            0.0
        } else {
            0.04 + (wind_ms.abs() / 14.0) * 0.16
        };
        if (target - self.wind_vol).abs() < 0.01 {
            return;
        }
        self.wind_vol = target;
        if let Some(w) = &self.wind {
            set_sound_volume(w, target);
        }
    }

    /// Ride the birdsong bed inversely to the wind — birds go quiet in a
    /// blow. Same ±0.01 mixer gating as the wind bed.
    pub fn set_birds(&mut self, wind_ms: f32) {
        let target = if self.muted {
            0.0
        } else {
            (0.13 - wind_ms.abs() / 14.0 * 0.11).max(0.02)
        };
        if (target - self.birds_vol).abs() < 0.01 {
            return;
        }
        self.birds_vol = target;
        if let Some(b) = &self.birds {
            set_sound_volume(b, target);
        }
    }
}
