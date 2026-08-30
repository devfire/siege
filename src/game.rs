//! Real-time duel: input, ball substeps, `AoE` damage, phases.

use crate::ai;
use crate::particles::Particles;
use crate::physics::{self, BALL_R, Ball, DT, Obstacle, Side, V2};
use crate::render;
use crate::rng::Rng;
use crate::world::{self, Crater, Segment, SegmentKind};
use macroquad::input::{
    KeyCode, MouseButton, is_key_pressed, is_mouse_button_pressed, mouse_position, mouse_wheel,
};
use std::collections::VecDeque;

const AOE_R: f32 = 3.2;
const SEG_DMG: f32 = 55.0;
const CANNON_DMG: f32 = 60.0;
const RELOAD: f32 = 3.5;
const CHARGE_STEP: f32 = 0.05; // per wheel notch
const CRATER_CAP: usize = 24;
const MARKER_CAP: usize = 6;
const RANGE_CAP: usize = 3;
const SHAKE_TAU: f32 = 0.35;
/// Starting regime is uniform on ±this value (m/s).
const WIND_BASE_SPAN: f32 = 12.0;
/// Regime mean reversion (1/s): ~20 s memory, so the base wanders across
/// zero within a round but aim knowledge survives between shots.
const WIND_REGIME_THETA: f32 = 0.05;
/// Regime noise (m/s per √s); with [`WIND_REGIME_THETA`] the stationary
/// spread is ~6 m/s.
const WIND_REGIME_KICK: f32 = 2.0;
/// Gust mean reversion (1/s): ~3 s memory, so a ball in flight meets a
/// different gust than the one its shot was aimed under.
const WIND_GUST_THETA: f32 = 0.30;
/// Gust noise (m/s per √s); with [`WIND_GUST_THETA`] the gust spread
/// around the base is ~3 m/s.
const WIND_GUST_KICK: f32 = 2.2;

/// Low-pass time constant (s) easing the live speed toward regime + gust.
/// Bounds how fast the wind any consumer sees can change; scenery and the
/// gauge read a glide, not a twitch.
const WIND_SMOOTH_TAU: f32 = 0.6;
/// Hard envelope (m/s) clamping the live speed; the AI wind bracket
/// ([−16, 16]) and the reach probe tune against ±14.
const WIND_MAX: f32 = 14.0;
pub(crate) const END_SLOWMO: f32 = 0.35;
pub(crate) const END_HOLD: f32 = 2.0;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Phase {
    Menu,
    Playing,
    Paused,
    Victory,
    Defeat,
}

/// Wind speed (m/s, +x), stochastic instead of a sine. Each round draws a
/// fresh base uniform on ±12 m/s — either sign equally likely, so no side
/// opens with a permanent headwind — then the base random-walks as a slow
/// Ornstein–Uhlenbeck regime and a fast band-limited gust OU wobbles ~±3
/// m/s around it. The live speed low-passes toward `base + gust`, so the
/// wind evolves gradually and smoothly instead of twitching with
/// per-substep noise. The wind is never steady: it drifts second to
/// second, and a ball in flight is pushed by a different gust than the
/// shot was aimed under. Peak magnitude stays ±14 m/s.
pub struct Wind {
    /// Slow regime the gusts ride on.
    base: f32,
    /// Fast OU wobble around the regime; band-limited, so it glides.
    gust: f32,
    /// Live speed applied to balls, particles, clouds, and the HUD gauge.
    speed: f32,
    /// Time integral of [`Wind::speed`] (m): how far the air has drifted.
    /// Scenery rides this so wind changes cannot teleport it.
    travel: f32,
}

impl Wind {
    /// Round start: uniform base across the full span; the live speed
    /// begins coherent with it.
    #[must_use]
    pub fn new(rng: &mut Rng) -> Self {
        let base = rng.range(-WIND_BASE_SPAN, WIND_BASE_SPAN);
        Self {
            base,
            gust: 0.0,
            speed: base,
            travel: 0.0,
        }
    }

    /// Advance one physics substep. The regime and the gust layer are both
    /// Ornstein–Uhlenbeck walks, but the live speed only eases toward
    /// `base + gust` with a [`WIND_SMOOTH_TAU`] time constant: gust noise
    /// reaches balls, particles, clouds, and the gauge as a gradual glide,
    /// never a twitch. `travel` integrates the speed so scenery can drift
    /// with the wind instead of multiplying it by elapsed time.
    pub fn step(&mut self, dt: f32, rng: &mut Rng) {
        let noise = dt.sqrt();
        self.base -= WIND_REGIME_THETA * self.base * dt;
        self.base += WIND_REGIME_KICK * noise * rng.gauss();
        self.gust -= WIND_GUST_THETA * self.gust * dt;
        self.gust += WIND_GUST_KICK * noise * rng.gauss();
        let target = (self.base + self.gust).clamp(-WIND_MAX, WIND_MAX);
        self.speed += (target - self.speed) * (dt / WIND_SMOOTH_TAU).min(1.0);
        self.speed = self.speed.clamp(-WIND_MAX, WIND_MAX);
        self.travel += self.speed * dt;
    }

    /// Live wind applied to balls, particles, clouds, and the HUD gauge.
    #[must_use]
    pub fn current(&self) -> f32 {
        self.speed
    }

    /// Distance the air has drifted (m): the time integral of the live
    /// speed. Scenery (clouds) rides this instead of `speed × t`, which
    /// jumps on every wind change; integrating makes wind response
    /// gradual by construction.
    #[must_use]
    pub fn travel(&self) -> f32 {
        self.travel
    }
}

pub struct PlayerCannon {
    pub angle_deg: f32,
    pub charge: f32,
    pub hp: f32,
    pub reload: f32,
}

pub struct DefenderCannon {
    pub display_angle: f32,
    pub reload_anim: f32,
}

pub struct GameState {
    pub phase: Phase,
    pub rng: Rng,
    pub t: f32,
    pub timescale: f32,
    pub player: PlayerCannon,
    pub defender: DefenderCannon,
    pub ai: ai::DefenderAi,
    pub balls: Vec<Ball>,
    pub particles: Particles,
    pub segments: Vec<Segment>,
    pub craters: VecDeque<Crater>,
    pub wind: Wind,
    pub shake: f32,
    pub markers: VecDeque<(V2, Side)>, // impact flags, cap 6, newest kept
    pub last_ranges: VecDeque<f32>,    // player's last 3 shot ranges in m
    /// Red vignette flash after the player takes damage (1 → 0).
    pub hurt: f32,
    /// Real seconds since Victory/Defeat began (overlay appears after `END_HOLD`).
    pub end_t: f32,
    sim_acc: f32,
}

impl GameState {
    #[must_use]
    pub fn new(mut rng: Rng) -> Self {
        // Round start: fresh uniform base; the live speed begins coherent.
        let wind = Wind::new(&mut rng);
        Self {
            phase: Phase::Menu,
            rng,
            t: 0.0,
            timescale: 1.0,
            player: PlayerCannon {
                angle_deg: 40.0,
                charge: 0.58,
                hp: 100.0,
                reload: 0.0,
            },
            defender: DefenderCannon {
                display_angle: 41.0,
                reload_anim: 0.0,
            },
            ai: ai::DefenderAi::new(),
            balls: Vec::new(),
            particles: Particles::new(),
            segments: world::castle_segments(),
            craters: VecDeque::new(),
            wind,
            shake: 0.0,
            markers: VecDeque::new(),
            last_ranges: VecDeque::new(),
            hurt: 0.0,
            end_t: 0.0,
            sim_acc: 0.0,
        }
    }

    fn restart(&mut self) {
        *self = GameState::new(Rng::seed(fresh_seed()));
    }

    /// Advance the duel by `dt_real` (unscaled wall-clock seconds).
    pub fn update(&mut self, dt_real: f32) {
        self.handle_input();
        if matches!(self.phase, Phase::Playing | Phase::Victory | Phase::Defeat) {
            let dt = dt_real * self.timescale;
            self.t += dt;
            self.player.reload = (self.player.reload - dt).max(0.0);
            self.defender.reload_anim = (self.defender.reload_anim - dt).max(0.0);
            self.hurt = (self.hurt - 2.5 * dt).max(0.0);
            if self.phase == Phase::Playing {
                self.tick_ai(dt);
            }
            self.sim_acc += dt.min(0.05);
            while self.sim_acc >= DT {
                self.substep();
                self.sim_acc -= DT;
            }
            for ball in &mut self.balls {
                ball.push_trail(ball.pos);
            }
            let wind = self.wind.current();
            self.particles.update(dt, wind);
            self.particles.spawn_leaves(wind, &mut self.rng, dt);
            self.shake = (self.shake * (-dt / SHAKE_TAU).exp()).max(0.0);
            if self.phase != Phase::Playing {
                self.end_t += dt_real;
            }
        }
    }

    fn handle_input(&mut self) {
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        match self.phase {
            Phase::Menu => {
                if clicked {
                    self.phase = Phase::Playing;
                }
            }
            Phase::Paused => {
                if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Escape) {
                    self.phase = Phase::Playing;
                }
                if is_key_pressed(KeyCode::R) {
                    self.restart();
                }
            }
            Phase::Playing => {
                if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Escape) {
                    self.phase = Phase::Paused;
                    return;
                }
                if is_key_pressed(KeyCode::R) {
                    self.restart();
                    return;
                }
                // Aim: barrel tracks the cursor.
                let (mx, my) = mouse_position();
                let m = render::screen_to_world(mx, my);
                let pivot = world::player_pivot();
                let ang = (m.y - pivot.y).atan2(m.x - pivot.x).to_degrees();
                self.player.angle_deg = ang.clamp(5.0, 80.0);
                // Charge: wheel notches set it; persists between shots.
                let (_, wheel_y) = mouse_wheel();
                if wheel_y != 0.0 {
                    self.player.charge =
                        (self.player.charge + wheel_y.signum() * CHARGE_STEP).clamp(0.18, 1.0);
                }
                if clicked {
                    self.fire_player();
                }
                if is_key_pressed(KeyCode::Space) {
                    self.fire_player();
                }
            }
            Phase::Victory | Phase::Defeat => {
                if is_key_pressed(KeyCode::R) {
                    self.restart();
                    return;
                }
                if clicked && self.end_t >= END_HOLD {
                    self.restart();
                }
            }
        }
    }

    fn fire_player(&mut self) {
        if self.phase != Phase::Playing || self.player.reload > 0.0 {
            return;
        }
        let pivot = world::player_pivot();
        let (pos, vel) = physics::launch(pivot, self.player.angle_deg, self.player.charge, 1.0);
        let a = self.player.angle_deg.to_radians();
        self.particles.spawn_muzzle(
            pos,
            V2 {
                x: a.cos(),
                y: a.sin(),
            },
        );
        self.balls.push(Ball {
            pos,
            vel,
            side: Side::Player,
            trail: VecDeque::new(),
        });
        self.player.reload = RELOAD;
    }

    fn tick_ai(&mut self, dt: f32) {
        let wind = self.wind.current();
        let target_x = world::player_pivot().x;
        if let Some(shot) = self
            .ai
            .update(self.t, wind, target_x, &mut self.rng, &self.segments)
        {
            let (pos, vel) =
                physics::launch(world::defender_pivot(), shot.angle_deg, shot.charge, -1.0);
            let a = shot.angle_deg.to_radians();
            self.particles.spawn_muzzle(
                pos,
                V2 {
                    x: -a.cos(),
                    y: a.sin(),
                },
            );
            self.balls.push(Ball {
                pos,
                vel,
                side: Side::Defender,
                trail: VecDeque::new(),
            });
            self.defender.reload_anim = 1.0;
        }
        // Barrel eases toward the current AI aim.
        let (aim, _) = self.ai.current_aim();
        self.defender.display_angle +=
            (aim - self.defender.display_angle) * (dt * 4.0).clamp(0.0, 1.0);
    }

    fn substep(&mut self) {
        // Wind evolves every substep, so a ball in flight meets changing gusts.
        self.wind.step(DT, &mut self.rng);
        let wind = self.wind.current();
        let keep_alive = self.keep_alive();
        let mut hits: Vec<(V2, Side, bool)> = Vec::new(); // (impact, side, ground contact)
        let mut despawn: Vec<usize> = Vec::new();
        for (i, ball) in self.balls.iter_mut().enumerate() {
            let prev = ball.pos;
            let (np, nv) = physics::step(ball.pos, ball.vel, wind, DT);
            ball.pos = np;
            ball.vel = nv;
            if np.x < -5.0 || np.x > 205.0 {
                despawn.push(i);
            } else if let Some((at, ground)) = contact(prev, ball, &self.segments, keep_alive) {
                hits.push((at, ball.side, ground));
                despawn.push(i);
            }
        }
        for &i in despawn.iter().rev() {
            self.balls.remove(i);
        }
        for (at, side, ground) in hits {
            self.explode(at, side, ground);
        }
    }

    fn keep_alive(&self) -> bool {
        self.segments
            .iter()
            .any(|s| s.kind == SegmentKind::Keep && s.alive())
    }

    fn explode(&mut self, at: V2, side: Side, ground_contact: bool) {
        self.particles.spawn_explosion(at, &mut self.rng);
        let gh = world::ground_height(at.x);
        if at.y <= gh + BALL_R + 0.25 {
            if self.craters.len() >= CRATER_CAP {
                self.craters.pop_front();
            }
            self.craters.push_back(Crater {
                x: at.x,
                r: self.rng.range(0.8, 1.4),
            });
            self.particles.spawn_dust(at);
        }
        for seg in &mut self.segments {
            if !seg.alive() {
                continue;
            }
            if let Some(c) = physics::hit(
                at,
                AOE_R,
                &Obstacle {
                    x0: seg.x0,
                    y0: seg.y0,
                    w: seg.w,
                    h: seg.h,
                },
            ) {
                let dmg = SEG_DMG * (1.0 - (c - at).length() / AOE_R).max(0.0);
                seg.hp = (seg.hp - dmg).max(0.0);
            }
        }
        let pp = world::player_pivot();
        if let Some(c) = physics::hit(
            at,
            AOE_R,
            &Obstacle {
                x0: pp.x - 1.6,
                y0: pp.y - 0.8,
                w: 3.2,
                h: 1.6,
            },
        ) {
            let dmg = CANNON_DMG * (1.0 - (c - at).length() / AOE_R).max(0.0);
            if dmg > 0.0 {
                self.player.hp = (self.player.hp - dmg).max(0.0);
                self.hurt = 1.0;
            }
        }
        if self.markers.len() >= MARKER_CAP {
            self.markers.pop_front();
        }
        self.markers.push_back((at, side));
        if side == Side::Player {
            let range = (at.x - pp.x).abs();
            if self.last_ranges.len() >= RANGE_CAP {
                self.last_ranges.pop_front();
            }
            self.last_ranges.push_back(range);
        }
        self.shake += (6.0 - (at - pp).length() / 15.0).clamp(0.0, 6.0);
        if side == Side::Defender {
            self.ai.observe(at.x, pp.x, ground_contact);
        }
        if self.phase == Phase::Playing {
            if !self.keep_alive() {
                self.phase = Phase::Victory;
                self.timescale = END_SLOWMO;
                self.end_t = 0.0;
            } else if self.player.hp <= 0.0 {
                self.phase = Phase::Defeat;
                self.timescale = END_SLOWMO;
                self.end_t = 0.0;
            }
        }
    }
}

/// First contact for a ball: ground, alive segments, rubble, cannon boxes.
/// Returns `(impact, ground_contact)`. The ground crossing is linearly
/// interpolated between `prev` and the current position, so the reported
/// impact sits on the terrain surface instead of up to one substep past it.
fn contact(prev: V2, ball: &Ball, segments: &[Segment], keep_alive: bool) -> Option<(V2, bool)> {
    let p = ball.pos;
    let gh = world::ground_height(p.x);
    if p.y <= gh + BALL_R {
        // Interpolate where the ball's surface crossed the ground.
        let f0 = prev.y - world::ground_height(prev.x) - BALL_R;
        let f1 = p.y - gh - BALL_R;
        let t = if f0 > f1 {
            (f0 / (f0 - f1)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let x = prev.x + (p.x - prev.x) * t;
        return Some((
            V2 {
                x,
                y: world::ground_height(x),
            },
            true,
        ));
    }
    for seg in segments {
        let rect = if seg.alive() {
            (seg.x0, seg.y0, seg.w, seg.h)
        } else {
            world::rubble_rect(seg)
        };
        let o = Obstacle {
            x0: rect.0,
            y0: rect.1,
            w: rect.2,
            h: rect.3,
        };
        if let Some(c) = physics::hit(p, BALL_R, &o) {
            return Some((c, false));
        }
    }
    if keep_alive {
        let dp = world::defender_pivot();
        let o = Obstacle {
            x0: dp.x - 0.8,
            y0: dp.y - 0.6,
            w: 1.6,
            h: 1.2,
        };
        if let Some(c) = physics::hit(p, BALL_R, &o) {
            return Some((c, false));
        }
    }
    let pp = world::player_pivot();
    let o = Obstacle {
        x0: pp.x - 1.6,
        y0: pp.y - 0.8,
        w: 3.2,
        h: 1.6,
    };
    physics::hit(p, BALL_R, &o).map(|c| (c, false))
}

/// Fresh seed from the platform clock (works native + wasm).
fn fresh_seed() -> u64 {
    let mut x = macroquad::miniquad::date::now().to_bits();
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
