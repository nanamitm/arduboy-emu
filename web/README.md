# arduboy-web — WebAssembly client

The Arduboy emulator running entirely in the browser. `arduboy-core` (Rust) is
compiled to WebAssembly via [`crates/wasm`](../crates/wasm) and driven by a
small vanilla-JS frontend — a `<canvas>` for video, the Web Audio API
(AudioWorklet) for sound, and keyboard + drag-and-drop for input.

```
┌──────────────────────┐ postMessage ┌──────────────────┐  wasm   ┌──────────────┐
│ index.html / main.js │ ◀─────────► │ emulator-worker  │ ──────► │ arduboy-core │
│ Canvas / AudioWorklet │  frame/audio │ (wasm-bindgen)  │         │  (emulator)  │
└──────────────────────┘              └──────────────────┘         └──────────────┘
```

No server-side code — it deploys as static files (Cloudflare Pages, GitHub
Pages, Netlify, any static host).

## Features

- Load `.hex` / `.arduboy` ROMs via **Open ROM…**, **drag-and-drop**, or a
  `?rom=<url>` deep link; optional **FX flash** `.bin` loading
- Canvas rendering (128×64, crisp `pixelated` upscale), ~60 fps loop, with
  **scale** presets (Fit / 2×–8×) and **fullscreen**
- **Web Worker emulation**: the Wasm core runs outside the UI thread. Video
  frames and PCM audio are transferred as `ArrayBuffer`s, keeping controls and
  menus responsive without requiring cross-origin isolation.
- **Palette themes**: White / Green LCD / Amber
- **Device skins**: Arduboy, Microcard, Tama, Pipboy 3000, and Pipboy Mk IV
  layouts. Select one in the toolbar, add `?skin=pipboy` (or `pipboymkiv`) to
  the URL, and the last selected skin is restored on the next visit.
- Input: keyboard (**Arrows** = D-pad, **Z** = A, **X** = B) and **on-screen
  touch controls** (D-pad + A/B) for mobile; standard **Gamepad API** support
  (D-pad or left stick, A/X = A, B/Y = B)
- **Online game catalog** from [eried/ArduboyCollection](https://github.com/eried/ArduboyCollection):
  search by title/author, filter by category, and load supported ROMs directly.
  The catalog JSON is generated weekly; ROMs and screenshots continue to be
  fetched from the upstream project, with its game-specific license metadata.
- **Installable PWA**: install from a supporting browser to launch the emulator
  as a standalone app. The emulator shell and catalog are cached for offline
  launch; individual upstream ROM downloads still require a connection.
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
# open http://localhost:8080/
# or /?rom=game.hex&skin=pipboy to auto-load a ROM with a skin
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
