# Conventions

## Code hygiene (hard gates)
- Zero clippy warnings (`clippy::all` + `pedantic` enabled as warns); bypass only with module-level `#[allow]` when truly necessary.
- `cargo fmt` clean. Rust source files ≤ 500 lines — split modules instead of growing files (render passes live in `render.rs`; if it bloats, extract a scenery module).
- Numeric literal separators: `1_000`, `240.0`, etc.

## macroquad API care
- Draw-call parameter order is strict and easy to get wrong: e.g. `draw_ellipse(x, y, rx, ry, rotation, color)` — rotation BEFORE color; `draw_rectangle_ex` has its own required sequence. Check the exact signature before writing/editing any draw call; wrong order compiles nowhere but wrong-order refactors have happened here.

## Design
- Structural enums over booleans: `Phase`, `Side`, `SegmentKind`, `PKind`.
- Determinism: all randomness flows through the PCG `rng::Rng`; game loop seeds with 1. AI convergence tests depend on seeded determinism — don't introduce unseeded entropy.
- Physics tuning constants (`K_DRAG`, `MUZZLE_V_MAX`, ...) are test-pinned via `tests/ballistics.rs`; retuning requires updating/adding ballistics tests, not loosening them.
- AI correctness (zeroing convergence, wind estimation) is test-pinned via `tests/ai_convergence.rs`.

## Process
- Commit regularly; on interruption/abort, commit all work-in-progress before stopping.
- Keep functional edits distinct from formatting churn in diffs.