# Siege — graphics & audio pass (2026-08-30)

**State: complete, committed as `e4f81bf`.** All gates green (fmt/clippy 0-warning pedantic, 6/6 test suites, wasm boots, canvas fills viewport). User visually confirmed wall runners and the priming glow. Dev server convention: `trunk serve --port 42387`; user verifies in browser personally (headless screenshot inspection does NOT work — no image input).

## What exists now

- **Audio (`src/audio.rs`)** — every sound synthesized at startup into in-memory mono 22.05 kHz WAVs (`load_sound_from_bytes`), no binary assets. Load failure → silence (`Option<Sound>` no-op `play`).
  - Boom near/far (sine sweep + LP noise thump + 30 ms crack), stone impact (gated gravel), crumble (grain slide + 3 settling thuds), fuse hiss (high-passed white + crackle pops), victory/defeat arpeggios, seamless wind bed (brown noise, loop-periodic swell, 0.4 s crossfaded seam) whose volume rides live wind via `set_wind` (gated ±0.01).
  - Triggers in `game.rs`: `fire_player`/`tick_ai` → `audio.fuse()`; launches → `boom_near/boom_far`; `explode` → `impact(near)` + `crumble` on any alive→dead segment + `victory()/defeat()` on phase change.
- **Primed fire** — `PlayerCannon{fuse,recoil}`, `DefenderCannon{recoil,fuse,pending:Option<ai::Shot>}`; `PRIME_PLAYER=0.32 s`, `PRIME_DEFENDER=0.55 s` telegraph (barrel eases onto pending shot angle). `tick_fuses` puffs the touch hole at 30 Hz with burn-progress-scaled particles and launches on expiry; aim tracks the cursor through the fuse; fuses tick during end slow-mo. Recoil decays 3/s; barrels draw kicked back along their axis.
- **Priming glow** — `fuse_glow()` in `render/mod.rs`: 3-layer swelling fire (halo ≤1.1 m / molten core / white-hot center) ramping with `fuse_progress()`; same on both cannons.
- **Actors (`src/render/actors.rs`)** — wall runners: 1 per 5 m of tower/curtain top, ping-pong (`tri = 1-|2u-1|`), striding legs, pikes, continuous duck factor `prox×elev` for balls within 12 m overhead, vanish when their segment dies. Player crew: rammer bobs during reload, gunner's torch lights (flame + halo) while fuse burns.
- **Castle (`render/castle.rs`)** — flickering lit keep windows + round loft window, waving tower pennants, gate braziers (flame + wall halo).
- **Scene** — birds (4 chevrons, flap + wind-drift via `wind.travel()`), ball shadows (altitude-faded ellipse on terrain).

## Follow-up pass (2026-08-30, later) — all five candidates done

- **Ball whistle + impact split** — `synth_whistle`: descending 1.5 s sweep fired once per ball when its drag-free time-to-ground estimate drops under `WHISTLE_LEAD` (1.5 s) while above 5 m (est. is only a cue; landing stays with `contact`). Ground hits now play a soft earth thud (`synth_thud`) — stone keeps the gravel crack via `impact(near, ground)`.
- **Runner deaths** — new `src/fallers.rs`: when a segment dies its runners are flung from their exact slots (`world::runner_count`/`runner_state`, shared with the renderer via `world::hash2`), tumble under gravity, bounce once, settle flat, fade (cap 32). Drawn as ragdolls in `render/actors.rs`.
- **UI sounds + birdsong** — wooden click (`synth_click`) on menu start / pause / resume / restart; 9 s looped birdsong bed (16 FM-warbled chirps, silent seam margins) whose volume ducks as wind rises (`set_birds`).
- **HUD fuse state** — readout flips to `FIRING` and a pulsing ember sweep crosses the charge gauge while the fuse burns (`draw_charge_gauge`).
- **God rays / vignette / ball rotation** — five slowly swaying sun shafts behind the clouds; nested dark edge bands under the HUD; three surface dimples ride `Ball.spin` (accumulated from horizontal travel at `SPIN_RATE`, ~7× slower than true rolling so it reads at 60 fps).

`Ball` grew presentation fields `spin`/`whistled` (same precedent as `trail`); both construction sites are in `game.rs`.

**State: gates green** (fmt, clippy 0-warning pedantic, 17/17 tests, wasm served on 42387).

### Known debt

- `src/game.rs` is 668 lines — over the 500-line convention (pre-existing; this pass grew it ~50 and put the new sim in `fallers.rs` instead). Next split candidate: fuse/prime ticking or the explode/damage path.
- Whistle cue ignores drag: on extreme lobs it can fire a beat early/late; acceptable for a warning cue.
- Headless screenshot verification does not work (no image input) — user verifies visuals in browser personally.

## Key files

`src/game.rs` (sim + triggers), `src/particles.rs` (`spawn_prime_puff(at, k)`), `src/render/{mod,actors,castle,scenery,hud}.rs`, `src/audio.rs`, `src/lib.rs` (Audio::new().await, passes `&mut Audio` into `update`).
