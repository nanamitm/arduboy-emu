# arduboy-web — WebAssembly client

The Arduboy emulator running entirely in the browser. `arduboy-core` (Rust) is
compiled to WebAssembly via [`crates/wasm`](../crates/wasm) and driven by a
small vanilla-JS frontend — a `<canvas>` for video, the Web Audio API
(AudioWorklet) for sound, and keyboard + drag-and-drop for input.

```
┌───────────────┐        ┌──────────────────┐  wasm   ┌──────────────┐
│ index.html/JS │ ─────► │ arduboy_bg.wasm  │ ──────► │ arduboy-core │
│ canvas/audio  │  glue  │ (wasm-bindgen)   │         │  (emulator)  │
└───────────────┘        └──────────────────┘         └──────────────┘
```

No server-side code — it deploys as static files (Cloudflare Pages, GitHub
Pages, Netlify, any static host).

## Features

- Load `.hex` / `.arduboy` ROMs via **Open ROM…**, **drag-and-drop**, or a
  `?rom=<url>` deep link; optional **FX flash** `.bin` loading
- Canvas rendering (128×64, crisp `pixelated` upscale), ~60 fps loop, with
  **scale** presets (Fit / 2×–8×) and **fullscreen**
- **Palette themes**: White / Green LCD / Amber
- Input: keyboard (**Arrows** = D-pad, **Z** = A, **X** = B) and **on-screen
  touch controls** (D-pad + A/B) for mobile
- Stereo audio via a single-threaded AudioWorklet (no SharedArrayBuffer, so no
  cross-origin isolation needed) with **volume** slider and **mute**
- **Save states** (quick slot per ROM) and **EEPROM** auto-persistence, both
  stored in **IndexedDB** and keyed by ROM name
- **PNG screenshots** and **animated GIF** recording (downloaded to disk)
- CPU auto-detection (ATmega32u4 / ATmega328P); live FPS + RGB/TX/RX LED readout
- Pause / Reset (reset resumes if paused)

Keyboard shortcuts: **R** reset · **P** pause · **M** mute · **S** screenshot ·
**G** GIF · **F5**/**F9** save/load state.

## Build

Prerequisites (once):

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Then:

```bash
./web/build.sh          # writes web/pkg/{arduboy.js, arduboy_bg.wasm, ...}
```

The prebuilt `web/pkg/` is committed so the site works with no build step.

## Run locally

wasm must be served over HTTP (not `file://`):

```bash
python -m http.server -d web 8080
# open http://localhost:8080/            (or /?rom=game.hex to auto-load)
```

## Deploy to Cloudflare Pages

The whole `web/` folder is static. Two options:

**A. Prebuilt (simplest).** `web/pkg/` is already committed, so no build step:

- Dashboard → Pages → *Connect to Git* → this repo
  - **Build command:** *(leave empty)*
  - **Build output directory:** `web`
- Or via Wrangler:
  ```bash
  npx wrangler pages deploy web --project-name arduboy-web
  ```

**B. Build in CI.** If you'd rather not commit `web/pkg/`, set the build command
to install the Rust toolchain + wasm-pack and run `./web/build.sh`, with output
directory `web`. (Slower cold builds; the prebuilt option is recommended.)

`web/_headers` sets long-cache for the hashed `pkg/` assets and no-cache for the
shell. Cloudflare serves `.wasm` as `application/wasm` automatically.
