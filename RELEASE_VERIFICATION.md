# Release verification

A repeatable procedure for verifying a release **before** it is published.
The `Build Release Packages` workflow (`.github/workflows/release.yml`) produces
a **draft** GitHub release with per-platform installers; this checklist gates
promoting that draft to a published release.

Artifacts under test:

| Platform | Artifact | Installs |
|----------|----------|----------|
| Windows  | `*.exe` (Inno Setup) | `arduboy-emu.exe` |
| Linux    | `*.deb`, `*.rpm` | `arduboy-emu` |
| macOS    | `*.pkg`, `*.dmg` (universal) | `arduboy-emu` |
| Web      | Cloudflare Pages deploy | browser client |

The desktop binary is `arduboy-emu` (crate `arduboy-frontend`). It supports a
headless mode used throughout this document:

```
arduboy-emu <rom.hex|.arduboy|.elf> --headless --frames N --snapshot F [--press P] [--cpu 328p] [--fx data.bin]
```

`--snapshot F` prints the display as ASCII at frame `F`, which is what makes the
checks below scriptable and diffable across releases.

---

## 1. Automated pre-flight (on the tagged source)

Run these against the exact commit the release was tagged from. All must pass.

```bash
# 1a. Full test suite, including the golden regression tests.
cargo test --workspace

# 1b. Multi-ROM smoke over both corpora (see "Corpora" below). Expect
#     0 panics / 0 unknown opcodes / 0 load failures, and every ROM rendering.
cargo run --release --example rom_smoke -p arduboy-core -- <ArduboyCollection> 600
cargo run --release --example rom_smoke -p arduboy-core -- <Gamebuino-Classic-Games-Compilation> 300
```

Expected summaries (current baseline):

- ArduboyCollection: `317/317 clean … Detected CPU: 317 × ATmega32u4`
- Gamebuino Classic: `50/50 clean … Detected CPU: 50 × ATmega328P`

Any `panic`, `unknown`, `load_fail`, or `blank` regression is a **blocker** —
investigate with `examples/rom_diag` before releasing.

### Corpora

These ROM sets live **outside** the repo (licensing). Point the harness at local
checkouts:

- **Arduboy** — [eried/ArduboyCollection](https://github.com/eried/ArduboyCollection) (317 `.hex`)
- **Gamebuino Classic** — a Gamebuino Classic games compilation (50 `.hex`)

---

## 2. Verification ROM set (headless spot-check)

A small, fixed set exercising display + input + sound + save + FX + 328P. Use it
for headless snapshots and (when hardware is available) real-device comparison.
Substitute equivalents if a title is unavailable, but keep the set stable so
snapshots are comparable release-to-release.

| # | ROM | Exercises |
|---|-----|-----------|
| 1 | Arduboy boot / any title screen | display + boot logo |
| 2 | A tone-heavy game (e.g. an ArduboyTones title) | Timer audio |
| 3 | A game using `analogRead` seeding | ADC (regression: analogRead hang) |
| 4 | A digitized-sound game (e.g. Ardletics) | Timer4 audio interrupt (regression: Timer4 vector) |
| 5 | An Arduboy **FX** game + its `.bin` | FX flash SPI (`--fx`) |
| 6 | A Gamebuino Classic title (`--cpu 328p`) | PCD8544 + 328P path |

Headless snapshot recipe (repeat per ROM; compare ASCII output to the previous
release's, or eyeball that it is a plausible title screen):

```bash
for rom in set/*.hex; do
  echo "== $rom =="
  arduboy-emu "$rom" --headless --frames 300 --snapshot 120 --snapshot 300 --mute
done
```

For the FX title add `--fx game.bin`; for Gamebuino add `--cpu 328p`.

---

## 3. Per-platform artifact install / launch

Do this on a clean machine or VM for each OS (at minimum the maintainer's own
platform every release; the other two whenever the installer scripts change).

For every artifact:

1. **Integrity** — record the SHA-256 of the downloaded file; confirm it matches
   the artifact you built (`sha256sum` / `shasum -a 256` / `Get-FileHash`).
2. **Install** — run the installer; confirm it completes without errors and the
   app appears where expected (Start menu / applications list).
3. **Launch** — open the app; it shows a window and accepts a ROM
   (Open ROM… / drag-and-drop / CLI arg).
4. **Run** — load verification ROM #1; confirm it renders, responds to the D-pad
   and A/B, and produces sound (unmuted).
5. **Persistence** — trigger an EEPROM save (play a game that saves) and a quick
   save state; relaunch and confirm they restore.
6. **Uninstall** — confirm the uninstaller removes the app cleanly.

| Platform | Artifact | Integrity | Install | Launch+run | Persistence | Uninstall |
|----------|----------|:---:|:---:|:---:|:---:|:---:|
| Windows `.exe` | | ☐ | ☐ | ☐ | ☐ | ☐ |
| Linux `.deb`   | | ☐ | ☐ | ☐ | ☐ | ☐ |
| Linux `.rpm`   | | ☐ | ☐ | ☐ | ☐ | ☐ |
| macOS `.pkg`   | | ☐ | ☐ | ☐ | ☐ | ☐ |
| macOS `.dmg`   | | ☐ | ☐ | ☐ | ☐ | ☐ |

Web client (deployed URL): load a ROM from the catalog and via a `?rom=` GitHub-
raw link, confirm render + input + audio, and that a PWA install works. See the
[CORS policy](web/README.md#rom-loading--cors-policy) for URL-load caveats.

---

## 4. Real-hardware comparison (実機)

Best-effort, done when physical hardware is available. Place the real device next
to the emulator running the **same** ROM from the verification set and compare.

**Arduboy (ATmega32u4)** and **Gamebuino Classic (ATmega328P)** checklist:

- ☐ Boot logo / title screen looks the same (allowing for LCD vs OLED contrast).
- ☐ D-pad + A/B move/act identically; no stuck or swapped inputs.
- ☐ Tones/melodies sound at the right pitch and tempo.
- ☐ A digitized-sound title plays audio and does **not** reset/hang
  (guards the ADC and Timer4-vector regressions).
- ☐ EEPROM save survives a power cycle on device and a relaunch in the emulator.
- ☐ (Arduboy FX cart, if available) an FX game loads its assets.

If no hardware is available for a given target, record that real-hardware
verification was **skipped** and rely on: the automated corpus (§1), the
verification-set snapshots (§2), and the committed emulator-vs-datasheet golden
tests (`cargo test`). Do not silently claim hardware verification that did not
happen.

---

## 5. Sign-off

Paste into the release PR / notes and check off before publishing the draft:

```
Release vX.Y.Z verification
- [ ] cargo test --workspace passed at <commit>
- [ ] rom_smoke: Arduboy 317/317, Gamebuino 50/50 (0 panic/unknown/load_fail/blank)
- [ ] Headless snapshots of the verification set look correct
- [ ] Artifact install/launch: Windows [ ] Linux .deb [ ] .rpm [ ] macOS .pkg [ ] .dmg [ ] Web [ ]
- [ ] SHA-256 of each published artifact recorded in the release notes
- [ ] Real-hardware spot-check: Arduboy [ ] Gamebuino [ ]  (or explicitly SKIPPED)
- [ ] CHANGELOG.md updated for vX.Y.Z
```

Only after every applicable box is checked, edit the draft GitHub release
(remove the `draft` flag) to publish.
