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

## ROM loading & CORS policy

There are three ways to get a ROM into the emulator:

| Method | How it reaches the app | Cross-origin? |
|--------|------------------------|:---:|
| **Open ROM…** / **drag-and-drop** | Read locally in the browser (`FileReader`) | No fetch — always works |
| **`?rom=<url>` deep link** | Browser `fetch(url)` → bytes | Subject to CORS |
| **Online catalog** | Browser `fetch(game.hexUrl)` | Subject to CORS |

### Why remote URLs depend on CORS

The site is **fully static** — there is no server-side code that could download a
ROM on the user's behalf. A `?rom=` or catalog load is a plain browser `fetch()`
issued from the page's own origin, so it is governed by the browser's
[same-origin policy](https://developer.mozilla.org/docs/Web/Security/Same-origin_policy).
A cross-origin fetch only succeeds when the **remote host returns a permissive
`Access-Control-Allow-Origin`** header. This is a browser security boundary that a
static site cannot and should not bypass.

Two consequences to be aware of:

- **Mixed content.** The site is served over HTTPS, so `http://` ROM URLs are
  blocked by the browser as mixed content. Use `https://`.
- **CORS headers.** If the host does not send `Access-Control-Allow-Origin`, the
  fetch fails and the app shows `Could not fetch <url>: …`. This is expected, not
  a bug in the emulator.

### What works

- **Same-origin** ROMs (hosted alongside the site, e.g. `?rom=game.hex`).
- CORS-permissive static hosts, including **`raw.githubusercontent.com`**
  (`access-control-allow-origin: *`) and **jsDelivr** (`cdn.jsdelivr.net`).
  The bundled catalog works precisely because its `hexUrl`s are GitHub-raw links.
- Object storage with CORS enabled (S3/R2/GCS bucket configured to allow the
  site's origin).

### What does not work

- Arbitrary web servers, blog links, or game pages that serve the `.hex` without
  CORS headers. Their download link may work in a normal browser tab but **not**
  from `fetch()` in another origin.

### Policy: no proxy

We deliberately **do not** run a server-side CORS proxy to “fix” such URLs:

- it would turn the deployment into an **open relay** (an abuse and security
  risk), and it contradicts the static, serverless design;
- it would route users' ROM URLs — and the ROM bytes — **through our
  infrastructure**, which we don't want for privacy reasons.

So cross-origin ROM loading is intentionally left to the source host's CORS
configuration.

### Workarounds for users

1. **Download and open locally** — save the `.hex`/`.arduboy` and use **Open ROM…**
   or drag-and-drop. No fetch, no CORS.
2. **Re-host on a CORS-friendly service** — push it to a GitHub repo and link the
   `raw.githubusercontent.com` URL, use jsDelivr, or a bucket with CORS enabled.
3. **Self-host the site** next to the ROM so the request is same-origin.

### Privacy

The `?rom=` URL is fetched **directly by the user's browser**; no ROM URL or ROM
data passes through the project's servers (there are none). Save states and
EEPROM are stored only in the browser's IndexedDB.
