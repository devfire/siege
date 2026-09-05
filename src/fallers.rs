//! Flung wall defenders — when a tower or curtain collapses, its runners
//! are thrown from the parapet slots they occupied that frame
//! (`world::runner_state`), tumble under gravity, bounce once, settle
//! flat, and fade. A blast landing close to a defender instead
//! obliterates them on the spot (`gib_defender`): helmet, head, torso,
//! and limbs fly as independent parts. Sim twin of the runner drawing
//! in `render::actors`.

use crate::physics::{G, V2};
use crate::rng::Rng;
use crate::world::{self, Segment};

const CAP: usize = 64;

/// Which piece of a defender a faller is: a whole ragdoll flung off a
/// collapsing wall, or one anatomical chunk of a gibbed body.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Part {
    /// Whole ragdoll — thrown clear of a wall that fell under them.
    Body,
    Head,
    Torso,
    Arm,
    Leg,
    Helmet,
}

/// One tumbling body. `spin == 0.0` marks the rested state.
pub struct Faller {
    pub pos: V2,
    pub vel: V2,
    /// Body axis angle (rad).
    pub ang: f32,
    /// Tumble rate (rad/s).
    pub spin: f32,
    /// Bounce spent?
    pub bounced: bool,
    /// Seconds since coming to rest (drives the settle ease).
    pub rest_t: f32,
    /// Which piece of a defender this faller is.
    pub part: Part,
    /// Seconds left before fading out.
    pub life: f32,
}

pub struct Fallers {
    pub list: Vec<Faller>,
}

impl Default for Fallers {
    fn default() -> Self {
        Self::new()
    }
}

impl Fallers {
    #[must_use]
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    fn push(&mut self, f: Faller) {
        if self.list.len() >= CAP {
            self.list.remove(0);
        }
        self.list.push(f);
    }

    /// Throw every runner standing on `seg` (index `ix`) as it dies.
    /// `at` is the impact point; `t` the sim time — spawn positions match
    /// the drawn runners exactly.
    pub fn fling_wall(&mut self, seg: &Segment, ix: usize, at: V2, t: f32, rng: &mut Rng) {
        let top = seg.y0 + seg.h;
        for k in 0..world::runner_count(seg) {
            // Slots vacated by a closer blast were gibbed then.
            if (seg.gone & (1_u8 << k)) != 0 {
                continue;
            }
            let (rx, dir, _) = world::runner_state(seg, ix, k, t);
            // Fling away from the impact; a runner right under it just
            // follows their facing.
            let away = if (rx - at.x).abs() < 0.3 {
                dir
            } else {
                (rx - at.x).signum()
            };
            self.push(Faller {
                pos: V2 { x: rx, y: top },
                vel: V2 {
                    x: away * rng.range(2.5, 6.5),
                    y: rng.range(4.5, 9.0),
                },
                ang: rng.range(0.0, std::f32::consts::TAU),
                spin: away * rng.range(5.0, 11.0),
                bounced: false,
                rest_t: 0.0,
                life: 2.8,
                part: Part::Body,
            });
        }
    }

    /// Blow the defender standing at `foot` (facing `dir`) to smithereens:
    /// helmet, head, torso, and four limbs burst apart, each an independent
    /// faller biased away from the blast at `at`. Heavier chunks (torso,
    /// legs) fly slower; the helmet pops hardest.
    pub fn gib_defender(&mut self, foot: V2, dir: f32, at: V2, rng: &mut Rng) {
        let away = if (foot.x - at.x).abs() < 0.3 {
            dir
        } else {
            (foot.x - at.x).signum()
        };
        // (part, spawn height above the boots, speed scale, spin ceiling)
        for (part, h, speed, spin_max) in [
            (Part::Helmet, 1.95_f32, 1.25_f32, 17.0_f32),
            (Part::Head, 1.77, 1.0, 13.0),
            (Part::Torso, 1.15, 0.75, 8.0),
            (Part::Arm, 1.42, 0.9, 14.0),
            (Part::Arm, 1.36, 0.9, 14.0),
            (Part::Leg, 0.5, 0.85, 11.0),
            (Part::Leg, 0.44, 0.85, 11.0),
        ] {
            self.push(Faller {
                pos: foot
                    + V2 {
                        x: rng.range(-0.15, 0.15),
                        y: h,
                    },
                vel: V2 {
                    x: away * rng.range(2.5, 6.0) * speed,
                    y: rng.range(3.5, 7.5) * speed,
                },
                ang: rng.range(0.0, std::f32::consts::TAU),
                spin: away * rng.range(4.0, spin_max),
                bounced: false,
                rest_t: 0.0,
                life: rng.range(1.7, 2.4),
                part,
            });
        }
    }

    /// Tumble under gravity, bounce once on the terrain, rest, fade.
    pub fn update(&mut self, dt: f32) {
        for f in &mut self.list {
            f.life -= dt;
            if f.spin == 0.0 {
                f.rest_t += dt;
                continue; // resting: settle + fade in place
            }
            f.vel.y -= G * dt;
            f.ang += f.spin * dt;
            let gh = world::ground_height(f.pos.x) + 0.12;
            if f.pos.y <= gh && f.vel.y < 0.0 {
                if f.bounced {
                    // Second contact: rest (settle + fade from here on).
                    f.vel = V2::default();
                    f.spin = 0.0;
                    f.pos.y = gh - 0.02;
                } else {
                    // First contact: the single bounce.
                    f.bounced = true;
                    f.vel.y = -f.vel.y * 0.25;
                    f.vel.x *= 0.5;
                    f.spin *= 0.4;
                    f.pos.y = gh;
                }
            }
            f.pos += f.vel * dt;
        }
        self.list.retain(|f| f.life > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::SegmentKind;

    fn wall() -> Segment {
        Segment {
            x0: 150.0,
            y0: 3.2,
            w: 10.0,
            h: 18.8,
            hp: 1.0,
            max_hp: 1.0,
            kind: SegmentKind::Tower,
            gone: 0,
        }
    }

    /// A gibbed defender bursts into exactly one helmet, head, and torso
    /// plus a pair each of arms and legs — every chunk thrown away from
    /// the blast.
    #[test]
    fn gib_bursts_into_all_parts_away_from_blast() {
        let mut fs = Fallers::new();
        let mut rng = Rng::seed(7);
        let foot = V2 { x: 155.0, y: 22.0 };
        fs.gib_defender(foot, 1.0, V2 { x: 153.0, y: 22.0 }, &mut rng);
        assert_eq!(fs.list.len(), 7);
        let (mut heads, mut torsos, mut arms, mut legs, mut helmets) = (0, 0, 0, 0, 0);
        for f in &fs.list {
            assert!(
                f.vel.x > 0.0,
                "{:?} not thrown away from the blast: {:?}",
                f.part,
                f.vel
            );
            match f.part {
                Part::Head => heads += 1,
                Part::Torso => torsos += 1,
                Part::Arm => arms += 1,
                Part::Leg => legs += 1,
                Part::Helmet => helmets += 1,
                Part::Body => panic!("whole body in a gib burst"),
            }
        }
        assert_eq!((heads, torsos, arms, legs, helmets), (1, 1, 2, 2, 1));
    }

    /// Runners already gibbed by an earlier blast must not be flung
    /// again when their wall finally collapses.
    #[test]
    fn fling_skips_vacated_slots() {
        let mut fs = Fallers::new();
        let mut rng = Rng::seed(9);
        let at = V2 { x: 155.0, y: 22.0 };
        let gone = Segment {
            gone: 0b0000_0011,
            ..wall()
        };
        fs.fling_wall(&gone, 0, at, 0.0, &mut rng);
        assert!(fs.list.is_empty(), "both slots vacated, nothing to fling");

        fs.fling_wall(&wall(), 0, at, 0.0, &mut rng);
        assert_eq!(fs.list.len(), 2); // w = 10 → runner_count = 2
        assert!(fs.list.iter().all(|f| f.part == Part::Body));
    }
}
