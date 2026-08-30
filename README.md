# Siege!

A real-time 2D physics artillery duel built with Rust 2024 and [macroquad](https://macroquad.rs).

In **Siege!**, the player commands a siege cannon on the left flank tasked with reducing an enemy castle's fortifications (towers, curtain walls, gate, and keep). Meanwhile, the AI defender perched atop the castle keep (171 m downrange, 30.9 m elevation) returns fire, dynamically adapting its aim to shifting wind conditions and splash observations.

---

## Quick Start

### Prerequisites
- **Rust toolchain:** Rust 2024 / 1.85+ (`rustup default stable`)
- Optional for WebAssembly builds: `trunk` 0.21.14+

### Running Locally
```bash
cargo run --release
```

### Running Tests
```bash
cargo test
```

### Controls
- **Aim:** Mouse cursor position (barrel tracks cursor angle: 5°–80° elevation).
- **Fire (Charge Mode):** Hold **Left Mouse Button (LMB)** to charge power (oscillates between 18% and 100%), release to fire.
- **Fire (Direct Power):** **Spacebar** fires using the current static power setting.
- **Adjust Base Power:** **Up / Down Arrow Keys** to adjust baseline power.
- **Pause / Unpause:** **P** or **Escape**.
- **Restart Match:** **R**.

---

## Game Architecture

The simulation runs on a deterministic, macroquad-independent physics foundation:

- **Coordinate System & Layout:** 
  - Player pivot: `(16.0, 3.3)` m (on ground plateau `y = 2.4`).
  - Castle footprint: spans `x = 148.0` to `196.0` m on a `y = 3.2` m plateau.
  - Defender pivot: `(171.0, 30.9)` m atop the keep.
- **Ballistics Simulation:**
  - Integrates quadratic aerodynamic drag with gravity:
    $$\vec{a} = -g \hat{j} - K_{\text{drag}} \|\vec{v}_{\text{rel}}\| \vec{v}_{\text{rel}}$$
    where $\vec{v}_{\text{rel}} = \vec{v} - (v_{\text{wind}}, 0)$, $g = 9.81\text{ m/s}^2$, and $K_{\text{drag}} = 0.0040\text{ m}^{-1}$.
  - Main simulation substeps at 240 Hz (`DT = 1/240 s`) using semi-implicit Euler integration.
  - Forward trajectory prediction uses `SIM_DT = 1/120 s`.
- **Wind Dynamics:** Two stochastic Ornstein–Uhlenbeck layers, integrated at the
  240 Hz substep so the live value drifts second to second (a ball in flight meets
  changing gusts):
  - **Regime base:** drawn uniform on $[-12, +12]\text{ m/s}$ at round start (either
    sign equally likely), then mean-reverting toward zero with
    $\theta = 0.05\text{ s}^{-1}$ and noise $2.0\text{ m/s}\sqrt{\text{s}}$ — it wanders across zero within a round.
  - **Gust layer:** mean-reverting toward the base with $\theta = 0.30\text{ s}^{-1}$ and
    noise $2.2\text{ m/s}\sqrt{\text{s}}$ (~3 s memory, ~±3 m/s spread).
  - Live speed clamped to $[-14, +14]\text{ m/s}$.

---

## Castle Defense AI Deep Dive

The castle defender AI (`src/ai.rs`) operates on a **Probe -> Observe -> Invert -> Secant Correct** closed-loop feedback pipeline.

```
       +---------------------------------------------+
       |               Initial Probe                 |
       |  Grid search across (angle, charge) with    |
       |  noisy wind estimate + U(0.94, 1.06) noise  |
       +----------------------+----------------------+
                              |
                              v
       +---------------------------------------------+
+----->|                  Fire Shot                  |
|      +----------------------+----------------------+
|                             |
|                             v
|      +---------------------------------------------+
|      |             Observe Impact (x)              |
|      |               err = x - x_target            |
|      +----------------------+----------------------+
|                             |
|                             v
|      +---------------------------------------------+
|      |           Ballistic Inversion               |
|      |  24-step bisection on [-16, 16] m/s to find |
|      |  effective wind that matches observed splash|
|      +----------------------+----------------------+
|                             |
|         +-------------------+-------------------+
|         | |err| <= 3.5m                         | |err| > 45m
|         v                                       v
|  +--------------+                       +---------------+
|  |  Zeroed In   |                       |   Big Miss    |
|  | Lock aim;    |                       | Discard state;|
|  | +/-2% jitter |                       | Full re-probe |
|  +--------------+                       +---------------+
|         |                                       |
|         | |err| > 6.0m (gust drift)             |
|         +-------------------+-------------------+
|                             |
|                             v
|      +---------------------------------------------+
|      |               Error Correction              |
|      |  Charge pinned at boundary (MIN / MAX)?     |
|      |   - No  --> Secant charge correction        |
|      |   - Yes --> Angle walking (+/-3 deg probe)  |
|      +----------------------+----------------------+
|                             |
+-----------------------------+
```

### 1. Noisy Prior & Initial Grid-Search Probe
The defender does not have immediate ground-truth knowledge of the wind. Instead:
- Initial wind is sampled with Gaussian noise: $\hat{v}_{\text{wind}} = v_{\text{wind\_true}} + \mathcal{N}(0, \sigma = 2.5\text{ m/s})$.
- The AI runs an offline brute-force simulation grid across candidate angles and charges:
  - **Angle Range:** $25^\circ \le \theta \le 65^\circ$ in steps of $2^\circ$ (21 samples).
  - **Charge Range:** $0.20 \le c \le 1.00$ in steps of $0.02$ (41 samples).
- It selects $(\theta^*, c^*)$ minimizing $|\hat{x}_{\text{landing}} - x_{\text{target}}|$ in its internal model.
- An intentional launch imperfection $c_{\text{aim}} = \text{clamp}(c^* \cdot \mathcal{U}(0.94, 1.06), 0.2, 1.0)$ is applied to simulate human/mechanical error on the opening salvo.

### 2. Ballistic Inversion (Reading Wind from Splashes)
When a cannonball impacts at $x_{\text{impact}}$, the AI uses the splash location to infer actual atmospheric wind conditions via **bisection inversion**:
- Given the fired parameters $(\theta, c)$, landing distance $x$ is strictly monotonically increasing with wind speed (tailwind pushes shots right/positive, headwind pulls left/negative).
- The AI executes 24 iterations of bisection over the interval $[-16.0, 16.0]\text{ m/s}$:
  $$\text{mid} = \frac{v_{\text{lo}} + v_{\text{hi}}}{2}$$
  $$x_{\text{sim}} = \text{simulate\_landing}(\theta, c, \text{dir} = -1, \text{wind} = \text{mid})$$
  $$\text{if } x_{\text{sim}} < x_{\text{impact}} \implies v_{\text{lo}} = \text{mid} \quad \text{else } v_{\text{hi}} = \text{mid}$$
- This refines the AI's internal wind estimate to sub-millimeter trajectory accuracy under steady wind.

### 3. Secant Charge Correction
When firing leftward from $x = 171\text{ m}$ toward target $x = 16\text{ m}$:
- Error is defined as $\text{err} = x_{\text{impact}} - x_{\text{target}}$.
- A positive error ($\text{err} > 0$) indicates an overshoot (landing to the right of the target, i.e., short of the player), requiring higher charge.
- When two consecutive shots $(c_{k-1}, \text{err}_{k-1})$ and $(c_k, \text{err}_k)$ are available, the next charge is calculated via the secant root-finding step:
  $$c_{k+1} = c_k - \text{err}_k \frac{c_k - c_{k-1}}{\text{err}_k - \text{err}_{k-1}}$$
- If prior history is unavailable or singular ($|\text{err}_k - \text{err}_{k-1}| \le 10^{-4}$)$, it applies a fixed directional nudge: $c_{k+1} = c_k + 0.05 \cdot \text{sgn}(\text{err}_k)$.

### 4. Charge-Saturated Angle Walking
If the charge saturates at limits ($c = 1.0$ and $\text{err} > 0$, or $c = 0.2$ and $\text{err} < 0$), charge adjustment alone cannot eliminate the error (common when firing into severe headwinds):
- The AI probes its internal model at $\theta \pm 3^\circ$ using the current pinned charge and refined wind estimate.
- It steps the barrel elevation by $3^\circ$ toward whichever neighbor yields an impact closer to $x_{\text{target}}$.

### 5. Hysteresis & Recovery
- **Zero-In:** When $|\text{err}| \le 3.5\text{ m}$, the gun enters the `zeroed` state. Subsequent shots maintain the locked elevation and charge, applying only subtle scatter $\mathcal{U}(-0.02, 0.02)$.
- **Gust Drift (Un-zeroing):** If time-varying wind causes $|\text{err}| > 6.0\text{ m}$, the AI exits the `zeroed` state and resumes secant corrections.
- **Big Miss / Re-probe:** If $|\text{err}| > 45.0\text{ m}$ (caused by massive wind regime shifts), historical data is purged, and a fresh grid-search probe is executed with the latest learned wind.

### 6. Firing Cadence
- The defender fires its opening probe at $t = 6.0\text{ s}$.
- Subsequent shots are scheduled at intervals of $\mathcal{U}(4.5, 7.0)\text{ s}$.
- Barrel visual elevation smoothly eases toward the target angle at $4.0\text{ rad/s}$.
