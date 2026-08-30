//! Defender AI — probe, observe, secant-correct.
//!
//! The defender gauges where each shot lands ([`DefenderAi::observe`]) and
//! iteratively corrects its aim until impacts zero in on the player cannon.
//! Every impact refines the wind estimate by inverting the ballistic model
//! (the gun literally reads the wind off its splashes); the charge is then
//! secant-corrected, and when the charge saturates at a boundary the angle
//! is walked toward the model-predicted closer impact. Hysteresis keeps a
//! zeroed gun firing with small scatter; gust drift beyond 6 m re-enters
//! the correction loop.
//!
//! * The first probe (and any re-probe after a huge miss) picks the angle
//!   from a ladder of candidates and bisects the charge against the target
//!   for each — impact x is strictly monotone in charge, so ~18 bisection
//!   steps converge exactly where the old 861-simulation grid left up to
//!   3 m of residual. The planning model collides against the same castle
//!   obstacles the real ball does, so it can no longer plan through its
//!   own walls.
//! * The learned wind estimate is kept across normal shots; only the
//!   initial probe and re-probes take a fresh noisy sample of the true
//!   wind.
//! * Charge-saturated angle correction + impact-driven wind learning: a
//!   biased wind estimate makes the probe pick an angle a couple of
//!   degrees off the true optimum with the charge already at max, and the
//!   right direction flips with the wind.

use crate::physics::{Landing, Obstacle, V2, simulate_landing};
use crate::rng::Rng;
use crate::world::{self, Segment};

const ANGLE_MIN: f32 = 25.0;
const ANGLE_MAX: f32 = 65.0;
const ANGLE_STEP: f32 = 3.0;
const CHARGE_MIN: f32 = 0.2;
const CHARGE_MAX: f32 = 1.0;
/// |err| at or below this zeroes the gun in.
const ZERO_IN: f32 = 3.5;
/// |err| above this un-zeroes (gust drift handling).
const ZERO_OUT: f32 = 6.0;
/// |err| above this discards the learned pair and re-probes from scratch.
const BIG_MISS: f32 = 45.0;
/// Wind estimator noise σ (m/s); the estimator is `true + gauss * σ`.
const WIND_SIGMA: f32 = 2.5;
/// Probe search center; also the initial display angle.
const FIRST_ANGLE: f32 = 41.0;
/// Ladder of candidate probe angles; one charge bisection per rung.
const LADDER: [f32; 8] = [27.0, 31.0, 35.0, 39.0, 43.0, 47.0, 51.0, 55.0];
/// Bisection iterations per ladder rung; 2^-12 of the charge span gives
/// ~0.03 m of impact resolution — far below one metre.
const BISECT_STEPS: u8 = 12;

pub struct Shot {
    pub angle_deg: f32,
    pub charge: f32,
}

pub struct DefenderAi {
    aim_angle: f32,           // deg, clamp [25, 65]
    aim_charge: f32,          // clamp [0.2, 1.0]
    prev: Option<(f32, f32)>, // (charge, err) of last fired shot
    zeroed: bool,
    next_fire: f32, // t of next shot; first at 6.0 s
    wind_est: f32,  // latest estimate; refined by each impact
}

impl Default for DefenderAi {
    fn default() -> Self {
        Self::new()
    }
}

impl DefenderAi {
    #[must_use]
    pub fn new() -> Self {
        Self {
            aim_angle: FIRST_ANGLE,
            aim_charge: 0.7,
            prev: None,
            zeroed: false,
            next_fire: 6.0,
            wind_est: 0.0,
        }
    }

    /// Pick the next shot. The wind estimate starts as true wind + N(0, 2.5)
    /// and is refined by every `observe` — it is only re-sampled on the
    /// initial probe and on re-probes, so what the splashes teach is never
    /// wiped before the next shot. First shots (and re-probes) pick the
    /// angle from [`LADDER`], bisecting the charge against `target_x` under
    /// the battlefield model (terrain + castle obstacles), then multiply the
    /// charge by U(0.94, 1.06) sloppiness. Later shots reuse the aim
    /// corrected in `observe`; while zeroed, only U(−0.02, 0.02) charge
    /// scatter is added.
    pub fn plan(&mut self, wind_true: f32, target_x: f32, rng: &mut Rng, segments: &[Segment]) {
        if self.prev.is_none() {
            self.wind_est = wind_true + rng.gauss() * WIND_SIGMA;
            let obstacles = world::collidables(segments);
            let muzzle = world::defender_pivot();
            let mut best_angle = FIRST_ANGLE;
            let mut best_charge = CHARGE_MIN;
            let mut best_err = f32::MAX;
            for &angle in &LADDER {
                let charge = self.bisect_charge(muzzle, angle, target_x, &obstacles);
                let err = self
                    .modeled_err(muzzle, angle, charge, target_x, &obstacles)
                    .abs();
                if err < best_err {
                    best_err = err;
                    best_angle = angle;
                    best_charge = charge;
                }
            }
            self.aim_angle = best_angle;
            self.aim_charge = (best_charge * rng.range(0.94, 1.06)).clamp(CHARGE_MIN, CHARGE_MAX);
        } else if self.zeroed {
            self.aim_charge =
                (self.aim_charge + rng.range(-0.02, 0.02)).clamp(CHARGE_MIN, CHARGE_MAX);
        }
    }

    /// Ground-impact x the model predicts for a candidate, with saturating
    /// fallbacks that keep the sign of `x - target_x` monotone in charge
    /// for [`Self::bisect_charge`]: blocked candidates are short (own wall,
    /// x ≈ 150+), off-field ones overshoot past the −5 m edge.
    fn modeled_err(
        &self,
        muzzle: V2,
        angle: f32,
        charge: f32,
        target_x: f32,
        obstacles: &[Obstacle],
    ) -> f32 {
        match simulate_landing(
            muzzle,
            angle,
            charge,
            -1.0,
            self.wind_est,
            world::ground_height,
            obstacles,
        ) {
            Landing::Ground(p) => p.x - target_x,
            Landing::Obstacle(c) => c.x - target_x,
            Landing::OffField => -5.0 - target_x,
        }
    }

    /// Bisection on charge: the defender fires leftward, so impact x
    /// decreases monotonically in charge; drive `x - target_x` to zero.
    fn bisect_charge(&self, muzzle: V2, angle: f32, target_x: f32, obstacles: &[Obstacle]) -> f32 {
        let g = |charge: f32| self.modeled_err(muzzle, angle, charge, target_x, obstacles);
        // Saturation short-circuits: if even full charge falls short the
        // bisection would walk to `CHARGE_MAX`; if minimum charge already
        // overshoots, to `CHARGE_MIN`. One eval each instead of twelve.
        if g(CHARGE_MAX) > 0.0 {
            return CHARGE_MAX;
        }
        if g(CHARGE_MIN) < 0.0 {
            return CHARGE_MIN;
        }
        let (mut lo, mut hi) = (CHARGE_MIN, CHARGE_MAX);
        for _ in 0..BISECT_STEPS {
            let mid = 0.5 * (lo + hi);
            if g(mid) > 0.0 {
                lo = mid; // short — needs more charge
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }
    /// or a cannon box: the splash says nothing about wind or range, so the
    /// learned pair is discarded and a re-probe happens — it is never fed
    /// to `learn_wind`. Ground impacts first refine `wind_est` by inverting
    /// the ballistic model, then charge correction runs as a secant on
    /// `f(charge) = err` (firing leftward, impact x decreases with charge,
    /// so a short shot — err > 0 — needs more charge); with no prior pair,
    /// step charge by `0.05·sign(err)`. When the charge is pinned at the
    /// boundary that err says to cross, charge cannot help: walk the angle
    /// one step toward whichever neighbor the model predicts closer to the
    /// target.
    ///
    /// Zeroed hysteresis: in at |err| ≤ 3.5, out at |err| > 6. A huge miss
    /// (|err| > 45 m) means the wind estimate was useless: forget the pair
    /// and re-probe.
    pub fn observe(&mut self, impact_x: f32, target_x: f32, ground_contact: bool) {
        if !ground_contact {
            // Struck a segment, rubble, or a cannon box: no ballistic signal.
            self.prev = None;
            self.zeroed = false;
            return;
        }
        let err = impact_x - target_x;
        let fired = self.aim_charge;
        self.wind_est = self.learn_wind(impact_x);
        if err.abs() <= ZERO_IN {
            self.zeroed = true;
        } else if err.abs() > ZERO_OUT {
            self.zeroed = false; // gust drift (or first bad probe) — back in the loop
        }
        if err.abs() > BIG_MISS {
            // Hopelessly off — probe anew with what this splash taught us.
            self.prev = None;
            self.zeroed = false;
            return;
        }
        if !self.zeroed {
            let pinned_high = self.aim_charge >= CHARGE_MAX && err > 0.0;
            let pinned_low = self.aim_charge <= CHARGE_MIN && err < 0.0;
            if pinned_high || pinned_low {
                self.correct_angle(target_x);
            } else {
                self.correct_charge(err);
            }
        }
        self.prev = Some((fired, err));
    }

    /// Secant (or nudge) on charge toward err = 0.
    fn correct_charge(&mut self, err: f32) {
        self.aim_charge = match self.prev {
            Some((prev_charge, prev_err)) if (err - prev_err).abs() > 1e-4 => {
                // Secant step: charge' = charge − err·(charge − prev)/(err − prev_err).
                let next =
                    self.aim_charge - err * (self.aim_charge - prev_charge) / (err - prev_err);
                next.clamp(CHARGE_MIN, CHARGE_MAX)
            }
            _ => (self.aim_charge + 0.05 * err.signum()).clamp(CHARGE_MIN, CHARGE_MAX),
        };
    }

    /// Charge-saturated case: probe the model at `angle ± ANGLE_STEP` with
    /// the pinned charge under the latest wind estimate, and step toward
    /// whichever neighbor lands closer to the target.
    fn correct_angle(&mut self, target_x: f32) {
        let muzzle = world::defender_pivot();
        let obstacles = world::collidables(&world::castle_segments());
        let x_at = |angle: f32| -> f32 {
            match simulate_landing(
                muzzle,
                angle,
                self.aim_charge,
                -1.0,
                self.wind_est,
                world::ground_height,
                &obstacles,
            ) {
                Landing::Ground(p) => p.x,
                Landing::Obstacle(c) => c.x,
                Landing::OffField => -5.0,
            }
        };
        let lo = x_at((self.aim_angle - ANGLE_STEP).max(ANGLE_MIN));
        let hi = x_at((self.aim_angle + ANGLE_STEP).min(ANGLE_MAX));
        let dir = if (hi - target_x).abs() < (lo - target_x).abs() {
            1.0
        } else {
            -1.0
        };
        self.aim_angle = (self.aim_angle + dir * ANGLE_STEP).clamp(ANGLE_MIN, ANGLE_MAX);
    }

    /// Invert the ballistic model: the wind at which the shot just fired
    /// (current aim) lands at `impact_x`. Impact x is monotone increasing
    /// in wind (a rightward wind pushes the ball right), so bisection on
    /// [−16, 16] m/s converges to the effective wind this splash measured.
    /// If either bracket endpoint is not a ground landing, or the bracket
    /// does not straddle the observation, the model is saturated: keep the
    /// previous estimate rather than invent one.
    fn learn_wind(&self, impact_x: f32) -> f32 {
        let muzzle = world::defender_pivot();
        let obstacles = world::collidables(&world::castle_segments());
        let x_at = |wind: f32| -> Option<f32> {
            match simulate_landing(
                muzzle,
                self.aim_angle,
                self.aim_charge,
                -1.0,
                wind,
                world::ground_height,
                &obstacles,
            ) {
                Landing::Ground(p) => Some(p.x),
                Landing::Obstacle(_) | Landing::OffField => None,
            }
        };
        let (mut lo, mut hi) = (-16.0_f32, 16.0);
        let (Some(x_lo), Some(x_hi)) = (x_at(lo), x_at(hi)) else {
            return self.wind_est;
        };
        if !(x_lo < impact_x && impact_x < x_hi) {
            return self.wind_est; // not straddled — bisection would invent wind
        }
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            match x_at(mid) {
                Some(x) if x < impact_x => lo = mid,
                // A saturated half-bracket is unusable, same as a too-high x.
                Some(_) | None => hi = mid,
            }
        }
        0.5 * (lo + hi)
    }

    /// Per-frame tick: when due, plan and emit the shot; schedule the next
    /// one U(4.5, 7.0) s out.
    pub fn update(
        &mut self,
        t: f32,
        wind_true: f32,
        target_x: f32,
        rng: &mut Rng,
        segments: &[Segment],
    ) -> Option<Shot> {
        if t < self.next_fire {
            return None;
        }
        self.plan(wind_true, target_x, rng, segments);
        let shot = Shot {
            angle_deg: self.aim_angle,
            charge: self.aim_charge,
        };
        self.next_fire = t + rng.range(4.5, 7.0);
        Some(shot)
    }

    #[must_use]
    pub fn current_aim(&self) -> (f32, f32) {
        (self.aim_angle, self.aim_charge)
    }
}
