# KAMIKAZE HACKER

A self-contained cyberpunk **tower-defense × crowd-runner** hybrid, written in
**Rust + [macroquad](https://macroquad.rs/)** and compiled to **WebAssembly**.
Defend the Core from waves of viruses with hacker towers, grow an orbiting swarm
of allied drones, and watch them dive **kamikaze** into the densest clusters.

**▶ Play it: https://abxda.github.io/kamikaze-hacker/**

> _by Dr. Coronado × Claude_

---

## Gameplay

- **Build / drag towers** (Firewall, Antivirus, ICE, Logic Bomb, Proxy) on the
  empty nodes — tap a node to open the radial menu, or **drag** a placed tower to
  relocate it. They auto-fire at viruses along the data lanes.
- **Collect cyan FORK orbs** drifting down the lanes — tap them to **multiply your
  drone swarm** (`+` adds drones, `x2` doubles them). The drones orbit the Core and
  shoot on their own.
- **Kamikaze drones**: randomly, an orbiting drone breaks formation, dives into the
  thickest swarm and **detonates** for big area damage.
- **Tap a lane** to fire a `kill -9` **purge pulse** (costs ROOT energy) that damages
  and slows every virus on that lane.
- **Combos**: chained kills build a multiplier; big hits trigger screen-flash and
  brief slow-motion.
- The **NEXT WAVE** button has an auto-launch countdown — click to start early, it
  can't be delayed.
- Six sectors, a Ransomware boss, EN/ES toggle, CRT scanlines, adaptive chiptune
  music. Works with **mouse and touch**.

## Tech & legal

- 100% self-contained: **no CDNs, no external fonts, no network calls at runtime**.
- All visuals are drawn procedurally; all **music and SFX are synthesized in code**
  to in-memory WAV — no asset files, no licensing concerns.
- Only dependency is `macroquad` (MIT / Apache-2.0).

## Build from source

Requires the Rust GNU toolchain and the wasm target:

```sh
rustup toolchain install stable-x86_64-pc-windows-gnu   # or your platform's GNU/host toolchain
rustup target add wasm32-unknown-unknown
```

Then (Windows PowerShell):

```powershell
./build.ps1          # compiles to WASM and copies kamikaze.wasm next to index.html
python serve.py      # serves the folder on http://localhost:8080
```

On other platforms, build manually:

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/kamikaze.wasm ./kamikaze.wasm
python3 serve.py
```

A static web server is required (browsers won't stream `.wasm` over `file://`).
The repository ships a prebuilt `kamikaze.wasm` so it runs on GitHub Pages directly.

## License

MIT — see [LICENSE](LICENSE).
