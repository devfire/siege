# Tech Stack

- Rust edition **2024**, `rust-version = 1.85`. Single dependency: **macroquad 0.4.16** (miniquad-based).
- Cargo lints: `clippy::all` + `clippy::pedantic` = warn, `unsafe_code` = warn. `[profile.dev] opt-level = 2` (game must run smoothly even in debug).
- Targets: native + `wasm32-unknown-unknown`. Bundler: **trunk**. No `Trunk.toml` — all trunk config lives in `index.html` via `data-trunk` links.

## WASM boot (deliberate, do not "fix")
macroquad is NOT a wasm-bindgen app. Modern trunk unconditionally runs wasm-bindgen/injects an ES module loader, which breaks macroquad's JS shell. Workaround in `index.html`:
- `<link data-trunk rel="rust" data-wasm-no-import data-wasm-opt="z" />` — keeps trunk's rust target for build/watch but suppresses bindgen import.
- `mq_js_bundle.js` copied via `data-trunk rel="copy-file"`; boot is `<script>load("siege.wasm")</script>` against the pristine cargo artifact (also copy-file'd from `target/wasm32-unknown-unknown/debug/siege.wasm`).
- Canvas must be `#glcanvas` with full-viewport CSS (`width/height: 100%`, `position: absolute`), else macroquad renders a small box top-left.
- Never add wasm-bindgen or switch to trunk's default import path; that reintroduces the loader bug.

## Assets
- `assets/MedievalSharp-Regular.ttf` — display font, embedded via `include_bytes!` (OFL license in `assets/OFL.txt`). Font load failure falls back to macroquad default font.
- `mq_js_bundle.js` — macroquad's JS runtime, checked in at repo root, copied into dist by trunk.