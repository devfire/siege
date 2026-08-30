//! Real-time duel: input, ball substeps, `AoE` damage, phases.

use crate::ai;
use crate::particles::Particles;
use crate::physics::{self, BALL_R, Ball, DT, Side, V2};
use crate::render;
use crate::rng::Rng;
use crate::world::{self, Crater, Segment, SegmentKind};
use macroquad::input::{
    KeyCode, MouseButton, is_key_down, is_key_pressed, is_mouse_button_down,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel,
};

const AOE_R: f32 = 3.2;
const SEG_DMG: f32 = 55.0;
const CANNON_DMG: f32 = 60.0;
const RELOAD: f32 = 3.5;
const CHARGE_RATE: f32 = (1.0 - 0.18) / 1.1; // 0.18 → 1.0 in 1.1 s
const SHAKE_TAU: f32 = 0.35;
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

pub struct Wind {
    pub base: f32,
    pub phase: f32,
}

impl Wind {
    #[must_use]
    pub fn current(&self, t: f32) -> f32 {
        self.base + 2.0 * (0.07 * t + self.phase).sin()
    }
}

pub struct PlayerCannon {
    pub angle_deg: f32,
    pub power: f32,
    pub charging: Option<f32>,
    pub hp: f32,
    pub reload: f32,
    /// Ramp direction while ping-ponging the charge.
    pub charge_dir: f32,
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
    pub craters: Vec<Crater>,
    pub wind: Wind,
    pub shake: f32,
    pub markers: Vec<(V2, Side)>, // impact flags, cap 6, newest kept
    pub last_ranges: Vec<f32>,    // player's last 3 shot ranges in m
    /// Red vignette flash after the player takes damage (1 → 0).
    pub hurt: f32,
    /// Real seconds since Victory/Defeat began (overlay appears after `END_HOLD`).
    pub end_t: f32,
    sim_acc: f32,
}

impl GameState {
    #[must_use]
    pub fn new(mut rng: Rng) -> Self {
        let wind = Wind {
            base: rng.range(-12.0, 12.0),
            phase: rng.range(0.0, std::f32::consts::TAU),
        };
        Self {
            phase: Phase::Menu,
            rng,
            t: 0.0,
            timescale: 1.0,
            player: PlayerCannon {
                angle_deg: 40.0,
                power: 0.58,
                charging: None,
                hp: 100.0,
                reload: 0.0,
                charge_dir: 1.0,
            },
            defender: DefenderCannon {
                display_angle: 41.0,
                reload_anim: 0.0,
            },
            ai: ai::DefenderAi::new(),
            balls: Vec::new(),
            particles: Particles::new(),
            segments: world::castle_segments(),
            craters: Vec::new(),
            wind,
            shake: 0.0,
            markers: Vec::new(),
            last_ranges: Vec::new(),
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
        self.handle_input(dt_real);
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
            let wind = self.wind.current(self.t);
            self.particles.update(dt, wind);
            self.particles.spawn_leaves(wind, &mut self.rng, dt);
            self.shake = (self.shake * (-dt / SHAKE_TAU).exp()).max(0.0);
            if self.phase != Phase::Playing {
                self.end_t += dt_real;
            }
        }
    }

    fn handle_input(&mut self, dt: f32) {
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
                // Power: mouse wheel notches ± arrows held.
                let wheel = mouse_wheel().1;
                let mut power = self.player.power - wheel * 0.03;
                if is_key_down(KeyCode::Up) {
                    power += 0.6 * dt;
                }
                if is_key_down(KeyCode::Down) {
                    power -= 0.6 * dt;
                }
                self.player.power = power.clamp(0.18, 1.0);
                // Charge: hold LMB, ramp 0.18→1.0 then ping-pong; release fires.
                if is_mouse_button_down(MouseButton::Left) && self.player.reload <= 0.0 {
                    let v = self.player.charging.get_or_insert(0.18);
                    *v += self.player.charge_dir * CHARGE_RATE * dt;
                    if *v >= 1.0 {
                        *v = 1.0;
                        self.player.charge_dir = -1.0;
                    }
                    if *v <= 0.18 {
                        *v = 0.18;
                        self.player.charge_dir = 1.0;
                    }
                }
                if is_mouse_button_released(MouseButton::Left) {
                    if let Some(c) = self.player.charging.take() {
                        self.fire_player(c);
                    }
                }
                if is_key_pressed(KeyCode::Space) && self.player.charging.is_none() {
                    self.fire_player(self.player.power);
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

    fn fire_player(&mut self, charge: f32) {
        if self.phase != Phase::Playing || self.player.reload > 0.0 {
            return;
        }
        let pivot = world::player_pivot();
        let (pos, vel) = physics::launch(pivot, self.player.angle_deg, charge, 1.0);
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
            trail: Vec::new(),
        });
        self.player.reload = RELOAD;
    }

    fn tick_ai(&mut self, dt: f32) {
        let wind = self.wind.current(self.t);
        let target_x = world::player_pivot().x;
        if let Some(shot) = self.ai.update(dt, self.t, wind, target_x, &mut self.rng) {
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
                trail: Vec::new(),
            });
            self.defender.reload_anim = 1.0;
        }
        // Barrel eases toward the current AI aim.
        let (aim, _) = self.ai.current_aim();
        self.defender.display_angle +=
            (aim - self.defender.display_angle) * (dt * 4.0).clamp(0.0, 1.0);
    }

    fn substep(&mut self) {
        let wind = self.wind.current(self.t);
        let keep_alive = self.keep_alive();
        let mut hits: Vec<(V2, Side)> = Vec::new();
        let mut despawn: Vec<usize> = Vec::new();
        for (i, ball) in self.balls.iter_mut().enumerate() {
            let (np, nv) = physics::step(ball.pos, ball.vel, wind, DT);
            ball.pos = np;
            ball.vel = nv;
            if np.x < -5.0 || np.x > 205.0 {
                despawn.push(i);
            } else if let Some(at) = contact(ball, &self.segments, keep_alive) {
                hits.push((at, ball.side));
                despawn.push(i);
            }
        }
        for &i in despawn.iter().rev() {
            self.balls.remove(i);
        }
        for (at, side) in hits {
            self.explode(at, side);
        }
    }

    fn keep_alive(&self) -> bool {
        self.segments
            .iter()
            .any(|s| s.kind == SegmentKind::Keep && s.alive())
    }

    /// Ball detonates at first contact: `AoE` damage, particles, scar, marker.
    fn explode(&mut self, at: V2, side: Side) {
        self.particles.spawn_explosion(at, &mut self.rng);
        let gh = world::ground_height(at.x);
        if at.y <= gh + BALL_R + 0.25 {
            self.craters.push(Crater {
                x: at.x,
                r: self.rng.range(0.8, 1.4),
            });
            if self.craters.len() > 24 {
                self.craters.remove(0);
            }
            self.particles.spawn_dust(at);
        }
        for seg in &mut self.segments {
            if !seg.alive() {
                continue;
            }
            if let Some(c) = world::hit_rect(at, AOE_R, seg.x0, seg.y0, seg.w, seg.h) {
                let dmg = SEG_DMG * (1.0 - (c - at).length() / AOE_R).max(0.0);
                seg.hp = (seg.hp - dmg).max(0.0);
            }
        }
        let pp = world::player_pivot();
        if let Some(c) = world::hit_rect(at, AOE_R, pp.x - 1.6, pp.y - 0.8, 3.2, 1.6) {
            let dmg = CANNON_DMG * (1.0 - (c - at).length() / AOE_R).max(0.0);
            if dmg > 0.0 {
                self.player.hp = (self.player.hp - dmg).max(0.0);
                self.hurt = 1.0;
            }
        }
        self.markers.push((at, side));
        if self.markers.len() > 6 {
            self.markers.remove(0);
        }
        if side == Side::Player {
            let range = (at.x - pp.x).abs();
            self.last_ranges.push(range);
            if self.last_ranges.len() > 3 {
                self.last_ranges.remove(0);
            }
        }
        self.shake += (6.0 - (at - pp).length() / 15.0).clamp(0.0, 6.0);
        if side == Side::Defender {
            self.ai.observe(at.x, pp.x);
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
fn contact(ball: &Ball, segments: &[Segment], keep_alive: bool) -> Option<V2> {
    let p = ball.pos;
    let gh = world::ground_height(p.x);
    if p.y <= gh + BALL_R {
        return Some(V2 { x: p.x, y: gh });
    }
    for seg in segments {
        let hit = if seg.alive() {
            world::hit_rect(p, BALL_R, seg.x0, seg.y0, seg.w, seg.h)
        } else {
            let (rx, ry, rw, rh) = world::rubble_rect(seg);
            world::hit_rect(p, BALL_R, rx, ry, rw, rh)
        };
        if hit.is_some() {
            return hit;
        }
    }
    if keep_alive {
        let dp = world::defender_pivot();
        if let Some(c) = world::hit_rect(p, BALL_R, dp.x - 0.8, dp.y - 0.6, 1.6, 1.2) {
            return Some(c);
        }
    }
    let pp = world::player_pivot();
    world::hit_rect(p, BALL_R, pp.x - 1.6, pp.y - 0.8, 3.2, 1.6)
}

/// Fresh seed from the platform clock (works native + wasm).
fn fresh_seed() -> u64 {
    let mut x = macroquad::miniquad::date::now().to_bits();
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
