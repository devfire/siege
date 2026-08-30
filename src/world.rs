//! World layout — terrain, castle segments, collision primitives.
//! Macroquad-free; layout truth for both the sim and the renderer.

use crate::physics::{self, V2};

/// Rolling base terrain (m above world floor y = 0).
fn base_terrain(x: f32) -> f32 {
    2.2 + 1.1 * (0.045 * x + 0.7).sin() + 0.6 * (0.11 * x + 2.0).sin()
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Blend `base` toward `flat` on `[a, b]`, ramping over `ramp` m at each edge.
fn flatten(x: f32, base: f32, a: f32, b: f32, flat: f32, ramp: f32) -> f32 {
    let t = if x < a - ramp || x > b + ramp {
        0.0
    } else if x < a {
        smoothstep((x - (a - ramp)) / ramp)
    } else if x > b {
        smoothstep(((b + ramp) - x) / ramp)
    } else {
        1.0
    };
    base + (flat - base) * t
}

/// Ground height at world `x`. Player platform (flat 2.4 m) on [10, 24];
/// castle plateau (flat 3.2 m) on [148, 196]; both lerped over 4 m edges.
#[must_use]
pub fn ground_height(x: f32) -> f32 {
    let base = base_terrain(x);
    let base = flatten(x, base, 10.0, 24.0, 2.4, 4.0);
    flatten(x, base, 148.0, 196.0, 3.2, 4.0)
}

#[derive(Copy, Clone, PartialEq)]
pub enum SegmentKind {
    Tower,
    Curtain,
    Gate,
    Keep,
}

pub struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub w: f32,
    pub h: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub kind: SegmentKind,
}

impl Segment {
    #[must_use]
    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }
}

/// Top of the keep — the defender's firing deck.
pub const KEEP_TOP: f32 = 30.0;
pub const DEFENDER_PIVOT_X: f32 = 171.0;

#[must_use]
pub fn player_pivot() -> V2 {
    V2 {
        x: 16.0,
        y: ground_height(16.0) + 0.9,
    }
}

#[must_use]
pub fn defender_pivot() -> V2 {
    V2 {
        x: DEFENDER_PIVOT_X,
        y: KEEP_TOP + 0.9,
    }
}

/// Castle segment table. `y0` = 3.2 plateau height; the keep stands behind
/// the walls (drawn behind, always collidable while alive).
#[must_use]
pub fn castle_segments() -> Vec<Segment> {
    fn seg(x0: f32, y0: f32, w: f32, h: f32, hp: f32, kind: SegmentKind) -> Segment {
        Segment {
            x0,
            y0,
            w,
            h,
            hp,
            max_hp: hp,
            kind,
        }
    }
    vec![
        seg(150.0, 3.2, 10.0, 18.8, 130.0, SegmentKind::Tower), // top at 22
        seg(160.0, 3.2, 10.0, 12.0, 100.0, SegmentKind::Curtain),
        seg(170.0, 3.2, 6.0, 9.0, 60.0, SegmentKind::Gate), // wooden portcullis
        seg(176.0, 3.2, 6.0, 12.0, 100.0, SegmentKind::Curtain),
        seg(182.0, 3.2, 10.0, 18.8, 130.0, SegmentKind::Tower), // top at 22
        seg(164.0, 3.2, 14.0, 26.8, 120.0, SegmentKind::Keep),  // top at 30
    ]
}

/// Every rect a ball can strike, in [`game`]'s contact order: live
/// segments at full height, destroyed ones as their rubble mound. The AI
/// planner feeds this to [`physics::simulate_landing`] so the model and
/// the real substep share one notion of "what stops a ball".
#[must_use]
pub fn collidables(segments: &[Segment]) -> Vec<physics::Obstacle> {
    segments
        .iter()
        .map(|seg| {
            let (x0, y0, w, h) = if seg.alive() {
                (seg.x0, seg.y0, seg.w, seg.h)
            } else {
                rubble_rect(seg)
            };
            physics::Obstacle { x0, y0, w, h }
        })
        .collect()
}

/// A destroyed segment leaves a rubble mound: the bottom 25% of its rect,
/// still collidable.
#[must_use]
pub fn rubble_rect(seg: &Segment) -> (f32, f32, f32, f32) {
    (seg.x0, seg.y0, seg.w, seg.h * 0.25)
}

/// Deterministic 0..1 hash of two integers (bricks, tufts, runner slots).
/// Shared by the sim and the renderer so both derive the same runner
/// layout — when a wall falls, the flung bodies leave from the exact
/// positions the drawn runners occupied that frame.
#[must_use]
pub fn hash2(a: u32, b: u32) -> f32 {
    let mut h = a
        .wrapping_mul(374_761_393)
        .wrapping_add(b.wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    let v = (h ^ (h >> 16)) & 0xFFFF;
    #[allow(clippy::cast_possible_truncation)] // masked to 16 bits above
    let wide = v as u16;
    f32::from(wide) / 65_535.0
}

/// Wall-top patrol density: one runner per 5 m of tower or curtain.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // w ≥ 0, small
pub fn runner_count(seg: &Segment) -> u32 {
    match seg.kind {
        SegmentKind::Tower | SegmentKind::Curtain => (seg.w / 5.0) as u32,
        SegmentKind::Gate | SegmentKind::Keep => 0,
    }
}

/// Runner `k` on segment `ix` at time `t` → `(x, facing, phase_seed)`:
/// ping-pong position along the parapet, stride direction (+1/−1), and
/// the hash seeding the stride cycle. Pure function of `t`, so drawing
/// and death-flinging always agree on where a runner stands.
#[must_use]
pub fn runner_state(seg: &Segment, ix: usize, k: u32, t: f32) -> (f32, f32, f32) {
    let seed = u32::try_from(ix).unwrap_or(0) + 1;
    let h1 = hash2(seed * 13 + k, 5);
    let h2 = hash2(seed * 13 + k, 9);
    let period = 7.0 + 6.0 * h2;
    let lap = (t / period + h1).rem_euclid(1.0);
    let tri = 1.0 - (2.0 * lap - 1.0).abs();
    let margin = 1.3;
    let span = (seg.w - 2.0 * margin).max(0.5);
    (
        seg.x0 + margin + span * tri,
        if lap < 0.5 { 1.0 } else { -1.0 },
        h1,
    )
}

/// Ground scar from an impact. Capped at 24, oldest dropped.
pub struct Crater {
    pub x: f32,
    pub r: f32,
}
