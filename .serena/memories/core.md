# Core — Siege!

2D artillery game (Rust + macroquad): player cannon vs AI defender on a castle wall. Ships as WASM in browser; native window 1600x900.

## Source map (all under src/)
- `lib.rs` — `run()` game loop: dt capped 0.05, `GameState::update` → `render::draw`, MedievalSharp font via `include_bytes!` (silent fallback to default font on load failure).
- `main.rs` — `#[macroquad::main]` entry, window config only.
- `physics.rs` — `V2`, `Ball`, `Side`; `accel`/`step`/`launch`/`simulate_landing`. Tuned constants: `G`, `K_DRAG` (0.0040), `DT` (1/240), `SIM_DT` (1/120), `MUZZLE_V_MAX` (52), `BARREL_LEN`, `BALL_R`. Drag is quadratic; constants are test-pinned (see tests).
- `game.rs` — `GameState`, `Phase` enum, `Wind` (zero-crossing swing: 12·sin(0.03t+slow_phase) + 2·sin(0.07t+fast_phase), envelope ±14 m/s; the old frozen per-round base left most rounds one-signed forever), `PlayerCannon`, `DefenderCannon`; damage/reload tuning (`AOE_R`, `SEG_DMG`, `CANNON_DMG`, `RELOAD`, `CHARGE_RATE`), `END_SLOWMO`/`END_HOLD`, `contact()`, `fresh_seed()`.
- `ai.rs` — `DefenderAi` + `Shot`: secant zeroing on angle (`ZERO_IN`/`ZERO_OUT`), wind estimation (`WIND_SIGMA`), angle bounds `ANGLE_MIN/MAX/STEP`, charge bounds, `FIRST_ANGLE`.
- `world.rs` — terrain (`base_terrain`, `smoothstep`, `flatten`, `ground_height`), cannon pivots (`player_pivot`, `defender_pivot`, `DEFENDER_PIVOT_X`), `Segment`/`SegmentKind` castle geometry with `hit_rect`, `Crater`, `rubble_rect`.
- `render.rs` — world→screen mapping (`WORLD_W/H`, `w2s`, `screen_to_world`), palette consts, draw passes: sky/sun/clouds/mountains/ground/castle/cannons/balls/aim/markers/hurt-vignette/UI/overlays.
- `particles.rs` — capped `Particles` pool (`CAP`, `LEAF_CAP`), `PKind`.
- `rng.rs` — PCG `Rng` (`PCG_MULT`/`PCG_INC`); `run()` seeds with 1.

## Tests (integration, run natively)
- `tests/ballistics.rs` — 5 tests: full power reaches castle, high arc falls short, half power midfield, wind matters, range monotonic in charge.
- `tests/ai_convergence.rs` — secant zeroing converges on the player.
- `tests/wind.rs` — wind sweeps strongly positive and negative within one slow period (~209 s) for any phase pair (guards against a frozen one-signed wind).

## Invariants
- Win/lose both via `hp <= 0` transition.
- Physics/AI tuning changes MUST keep `cargo test` green; ballistics tests pin K_DRAG + muzzle behavior.
- WASM boot quirks are deliberate — read `mem:tech_stack` before touching index.html/trunk.
- Commands: `mem:suggested_commands`. Style gates: `mem:conventions`. Done-checklist: `mem:task_completion`.