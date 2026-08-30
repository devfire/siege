# Task Completion

A coding task is done only when ALL of the following pass, run from the project root:

1. `cargo fmt --check` — no formatting diffs.
2. `cargo clippy --all-targets` — zero warnings (pedantic enabled).
3. `cargo test` — all integration tests pass (ballistics + AI convergence).
4. If gameplay/visual behavior changed: `trunk serve --port 42387` and browser-verify (screenshot; canvas fills viewport; changed behavior observable in-game). Terminal-only verification is insufficient for render/gameplay changes.
5. If the WASM artifact or index.html changed: confirm the served page boots macroquad (`load("siege.wasm")`) — a bindgen/loader regression shows as a blank canvas.
6. Commit with a plain descriptive subject.