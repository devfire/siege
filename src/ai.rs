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
//! * The first probe grid-searches angle AND charge (plan: angle fixed at
//!   41°). At `K_DRAG` = 0.004 a fixed 41° cannot reach the player for most
//!   of the wind range (e.g. wind +8 m/s caps a 41° full-charge shot ~7 m
//!   short), so the probe must pick the angle the wind allows.
//! * A |err| > 45 m miss re-probes (fresh grid search with a new wind
//!   estimate) instead of nudging the angle toward 45°, which moves the
//!   wrong way in a headwind.
//! * Charge-saturated angle correction + impact-driven wind learning (not
//!   in the plan; required because a biased wind estimate makes the probe
//!   pick an angle a couple of degrees off the true optimum with the
//!   charge already at max, and the right direction flips with the wind).

use crate::physics::simulate_landing;
use crate::rng::Rng;
use crate::world;

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

    /// Pick the next shot. Wind estimate starts as true wind + N(0, 2.5)
    /// and is refined by every `observe`. First shot (and re-probes):
    /// grid-search angle 25..=65 step 2 and charge 0.20..=1.00 step 0.02
    /// against the battlefield model, then multiply charge by
    /// U(0.94, 1.06) sloppiness. Later shots reuse the aim corrected in
    /// `observe`; while zeroed, only U(−0.02, 0.02) charge scatter is
    /// added.
    pub fn plan(&mut self, wind_true: f32, target_x: f32, rng: &mut Rng) {
        self.wind_est = wind_true + rng.gauss() * WIND_SIGMA;
        if self.prev.is_none() {
            let muzzle = world::defender_pivot();
            let mut best_charge = CHARGE_MIN;
            let mut best_angle = FIRST_ANGLE;
            let mut best_err = f32::MAX;
            for ai in 0..=20u8 {
                let angle = ANGLE_MIN + 2.0 * f32::from(ai);
                for ci in 0..=40u8 {
                    let charge = CHARGE_MIN + 0.02 * f32::from(ci);
                    let x = simulate_landing(
                        muzzle,
                        angle,
                        charge,
                        -1.0,
                        self.wind_est,
                        &world::ground_height,
                    )
                    .x;
                    let err = (x - target_x).abs();
                    if err < best_err {
                        best_err = err;
                        best_angle = angle;
                        best_charge = charge;
                    }
                }
            }
            self.aim_angle = best_angle;
            self.aim_charge = (best_charge * rng.range(0.94, 1.06)).clamp(CHARGE_MIN, CHARGE_MAX);
        } else if self.zeroed {
            self.aim_charge =
                (self.aim_charge + rng.range(-0.02, 0.02)).clamp(CHARGE_MIN, CHARGE_MAX);
        }
    }

    /// Record where a shot landed: `err = impact_x − target_x`.
    ///
    /// First the impact refines `wind_est` by inverting the ballistic model
    /// — the gun reads the wind off the splash. Charge correction is then a
    /// secant on `f(charge) = err` (firing leftward, impact x decreases
    /// with charge, so a short shot — err > 0 — needs more charge); with no
    /// prior pair, step charge by `0.05·sign(err)`. When the charge is
    /// pinned at the boundary that err says to cross (max charge yet still
    /// short), charge cannot help: walk the angle one step toward whichever
    /// neighbor the model predicts closer to the target.
    ///
    /// Zeroed hysteresis: in at |err| ≤ 3.5, out at |err| > 6. A huge miss
    /// (|err| > 45 m) means the wind estimate was useless: forget the pair
    /// and re-probe.
    pub fn observe(&mut self, impact_x: f32, target_x: f32) {
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
        let lo = simulate_landing(
            muzzle,
            (self.aim_angle - ANGLE_STEP).max(ANGLE_MIN),
            self.aim_charge,
            -1.0,
            self.wind_est,
            &world::ground_height,
        )
        .x;
        let hi = simulate_landing(
            muzzle,
            (self.aim_angle + ANGLE_STEP).min(ANGLE_MAX),
            self.aim_charge,
            -1.0,
            self.wind_est,
            &world::ground_height,
        )
        .x;
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
    fn learn_wind(&self, impact_x: f32) -> f32 {
        let muzzle = world::defender_pivot();
        let (mut lo, mut hi) = (-16.0_f32, 16.0);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            let x = simulate_landing(
                muzzle,
                self.aim_angle,
                self.aim_charge,
                -1.0,
                mid,
                &world::ground_height,
            )
            .x;
            if x < impact_x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Per-frame tick: when due, plan and emit the shot; schedule the next
    /// one U(4.5, 7.0) s out. `dt` is unused — barrel easing lives in the
    /// game state, which reads `current_aim`.
    pub fn update(
        &mut self,
        _dt: f32,
        t: f32,
        wind_true: f32,
        target_x: f32,
        rng: &mut Rng,
    ) -> Option<Shot> {
        if t < self.next_fire {
            return None;
        }
        self.plan(wind_true, target_x, rng);
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
