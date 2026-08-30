# Suggested Commands

Project commands (Linux, standard unix utilities otherwise behave normally — nothing OS-specific to note):

## Dev server / browser verification
- `trunk serve --port 42387` — dev server; **42387 is the project-convention port**. Kill leftover servers on this port before starting (orphan trunk processes from aborted sessions have held it).
- Browser verification: serve, then screenshot the page with a browser tool; canvas must fill the viewport.
- `trunk build` — release-ish bundle into `dist/`.

## Build
- `cargo build --target wasm32-unknown-unknown` — produces the artifact trunk's copy-file serves (`target/wasm32-unknown-unknown/debug/siege.wasm`). trunk's rust link also triggers this build.

## Test / lint / format
- `cargo test` — all integration tests run natively (ballistics, AI convergence); no wasm needed.
- `cargo clippy --all-targets` — must be zero-warning (project gate).
- `cargo fmt` / `cargo fmt --check`.

Completion gate ordering: `mem:task_completion`.