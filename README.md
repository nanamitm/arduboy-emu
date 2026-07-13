# arduboy-emu

**v0.8.1** — A cycle-accurate Arduboy emulator written in Rust.

Emulates the ATmega32u4 (Arduboy) and ATmega328P (Gamebuino Classic) microcontrollers at 16 MHz with display, audio, gamepad, and Arduboy FX flash support. Includes an interactive debugger, execution profiler, and GDB server for avr-gdb integration.

## Features

- **Dual CPU support** — ATmega32u4 (Arduboy) and ATmega328P (Gamebuino Classic), auto-detected from binary
- **AVR CPU core** — 80+ instructions with accurate flag computation (ADD, SUB, SBC/SBCI carry chains, MUL, etc.)
- **SSD1306 OLED display** — 128×64 monochrome with horizontal/vertical addressing, contrast control, and invert
- **PCD8544 LCD** — 84×48 Nokia display for Gamebuino Classic compatibility (auto-detected, default on 328P)
- **LCD effect** — Display-accurate color palettes, pixel grid, response ghosting, dot rounding (L key)
- **Stereo audio** — Two independent channels with sample-accurate waveform rendering
- **Gamepad support** — Cross-platform via gilrs (Windows/Linux/macOS), with hot-plug
- **Arduboy FX** — W25Q128 16 MB SPI flash emulation (Read, Fast Read, JEDEC ID, erase, program)
- **Peripherals** — Timer0/1/2/3/4, SPI, ADC, PLL, EEPROM, USB Serial output
- **Interactive debugger** — RAM hex viewer, I/O register viewer with names, breakpoints, watchpoints, profiler
- **Execution profiler** — PC histogram, top-N hotspot analysis, call graph, CPI metrics (T key / `--profile`)
- **GDB server** — Remote Serial Protocol over TCP for avr-gdb (`--gdb <port>`)
- **ELF/DWARF debug** — Load `.elf` files with symbol table and source-level debugging
- **Rewind** — Hold Backspace to rewind up to 5 minutes of gameplay
- **Save states** — Quick save (F5) / quick load (F9) with full emulator state persistence
- **Dynamic display** — Scale 1×–6× toggle, fullscreen, PNG screenshots, blur filter
- **USB Serial** — Captures `Serial.print()` output via UEDATX register interception (32u4 only)
- **Headless mode** — Automated testing with frame snapshots and diagnostics
- **.arduboy file support** — Load ZIP archives with info.json, hex, and FX bin
- **EEPROM persistence** — Auto-save/load to .eep file alongside game
- **GIF recording** — Capture gameplay as animated GIF (G key toggle, LZW compressed)
- **LED status** — RGB LED, TX LED, RX LED state displayed in title bar
- **FPS control** — Toggle between 60fps locked and unlimited (F key)
- **Hot reload** — Reload current game file without restart (R key)
- **Game browser** — N/P keys to cycle through games in directory, O to list

## Client applications

Choose the client that best fits how you want to run games. Both use the same
`arduboy-core` emulation logic and support `.hex`, `.arduboy`, and `.elf` ROMs.

| Client | Best for | Highlights |
|--------|----------|------------|
| [Web client](web/README.md) | Playing in a browser | WebAssembly, drag-and-drop ROM loading, touch controls, Gamepad API, ArduboyCollection online catalog, local save data, screenshots/GIF recording, and five switchable device skins |
| [Qt6 client](frontend-qt/README.md) | Native desktop use | Qt Widgets UI, native audio, menu-driven controls, save states, screenshots/GIF recording, five persistent device skins, and clickable on-screen controls |
| `frontend-minifb` | Lightweight desktop/debugging | Native Rust frontend with gamepad support, debugger tools, LCD effects, profiler, and command-line options |

### Web client

The web client is available at [arduboy-web.pages.dev](https://arduboy-web.pages.dev/).
To run the checked-out version locally, serve the `web/` directory (ES modules
cannot be loaded reliably from `file://` URLs):

```bash
python -m http.server 8080 -d web
```

Then open `http://localhost:8080`. Use **Open ROM** or drag a ROM into the page.
Keyboard controls are arrows for the D-pad, **Z** for A, and **X** for B;
touch controls and a connected standard gamepad are also supported.

### Qt6 client

The native Qt6 client lives in [`frontend-qt`](frontend-qt/). It provides the
same emulator display inside selectable Arduboy, Microcard, Tama, Pip-Boy 3000,
and Pip-Boy Mk IV skins. Select one from **View ▸ Skin**; the choice is restored
at the next launch. Click and hold the displayed D-pad or A/B buttons to play,
or use arrows, **Z**, and **X**. On Windows, XInput gamepads are also supported
(D-pad/left stick; A/X → A and B/Y → B). Build prerequisites and platform-specific
instructions are in the [Qt client README](frontend-qt/README.md).

## Building

```bash
# Linux: install dependencies
sudo apt install libudev-dev libasound2-dev

# Build and run
cargo build --release
cargo run --release -- game.hex
```

### Creating Installers

Pre-built installer scripts for all platforms. See [BUILDING.md](BUILDING.md) for full details.

```bash
./build-installers.sh                            # Auto-detect OS
installers\windows\build-windows.bat             # Windows (.exe via Inno Setup)
./installers/linux/build-linux.sh --deb          # Debian/Ubuntu (.deb)
./installers/linux/build-linux.sh --rpm          # Fedora/RHEL (.rpm)
./installers/macos/build-macos.sh --universal    # macOS (.pkg + .dmg, universal)
```

## Usage

```
arduboy-emu <file.hex|file.arduboy|file.elf> [options]

Options:
  --fx <file.bin>    Load FX flash data
  --cpu <type>       CPU type: 32u4 or 328p (auto-detected if omitted)
  --mute             Disable audio
  --debug            Show per-frame diagnostics
  --headless         Run without GUI
  --frames N         Run N frames (headless, default 60)
  --press N          Press A button on frame N (headless)
  --snapshot F       Print display at frame F (repeatable)
  --break <addr>     Set breakpoint at hex byte-address (repeatable)
  --watch <addr>     Set data watchpoint at hex address (repeatable)
  --step             Interactive debugger (RAM viewer, profiler, watchpoints)
  --gdb <port>       Start GDB remote debug server on TCP port
  --profile          Enable execution profiler (report on exit)
  --scale N          Initial display scale 1-6 (default 6)
  --serial           Show USB serial output on stderr
  --no-save          Disable EEPROM auto-save
  --lcd              Start with LCD display effect enabled
  --no-blur          Start with blur filter disabled
```

### File Formats

| Format | Description |
|--------|------------|
| `.hex` | Intel HEX binary (auto-detects companion `.bin` / `-fx.bin` for FX data) |
| `.arduboy` | ZIP archive containing `info.json`, `.hex`, and optional FX `.bin` |

### FX Flash Auto-Detection

FX data is loaded automatically if a matching `.bin` file exists alongside the `.hex`:

```
game.hex + game.bin       → auto-loaded
game.hex + game-fx.bin    → auto-loaded
game.hex --fx custom.bin  → explicit path
game.arduboy              → hex + fx extracted from ZIP
```

### EEPROM Persistence

EEPROM is automatically saved to a `.eep` file alongside the game:

```
game.hex → game.eep (auto-saved every 10s + on exit)
```

Use `--no-save` to disable. EEPROM data survives hot reload (R key).

### Save States

Press **F5** to quick-save and **F9** to quick-load the full emulator state:

```
game.hex → game.state (CPU, RAM, EEPROM, timers, display, FX flash, ...)
```

Save files use deflate compression, CPU type validation, and a versioned binary format. Loading a save state clears the rewind buffer.

### Game Browser

Press **O** to list all `.hex` and `.arduboy` files in the game's directory, then use **N** (next) and **P** (previous) to switch between them. EEPROM state is saved and loaded per game automatically.

```
--- Games in ./roms (5 found) ---
    1. arcodia.hex
    2. breakout.hex <<
    3. circuit-dude.arduboy
    4. nineteen44.hex
    5. starduino.hex
---
```

## Controls

| Arduboy     | Keyboard   | Xbox Controller             | PlayStation                   |
|-------------|------------|-----------------------------|-------------------------------|
| D-pad       | Arrow keys | D-pad / Left stick          | D-pad / Left stick            |
| A           | Z          | X, Y, LB, RB, LT, RT, Select | □, △, L1, R1, L2, R2, Select |
| B           | X          | A, B, Start                 | ×, ○, Start                   |
| Scale 1×–6× | 1–6 keys   | —                           | —                             |
| Fullscreen  | F11        | —                           | —                             |
| Screenshot  | S          | —                           | — (PNG at current scale)      |
| GIF record  | G          | —                           | —                             |
| Next game   | N          | —                           | —                             |
| Prev game   | P          | —                           | —                             |
| List games  | O          | —                           | —                             |
| Reload      | R          | —                           | —                             |
| FPS toggle  | F          | —                           | — (60fps ↔ unlimited)         |
| Reg dump    | D          | —                           | —                             |
| Mute       | M          | —                           | —                             |
| Audio filter| A          | —                           | — (LPF/envelope/crossfeed)    |
| Blur       | B          | —                           | — (soft pixel smoothing)      |
| LCD effect | L          | —                           | — (display-accurate colors)   |
| Portrait   | V          | —                           | — (rotate 90° left→bottom)    |
| Profiler   | T          | —                           | — (toggle execution profiler) |
| Rewind     | Backspace  | —                           | — (hold to rewind ~5 min)     |
| Save state | F5         | —                           | — (quick save to .state file) |
| Load state | F9         | —                           | — (quick load from .state)    |
| Quit       | Escape     | —                           | —                             |

Keyboard and gamepad inputs are OR-combined, so both can be used simultaneously.

## Architecture

```
arduboy-emu/
├── crates/
│   ├── core/                    # Platform-independent emulation core
│   │   └── src/
│   │       ├── lib.rs           # Arduboy struct: top-level emulator
│   │       ├── cpu.rs           # AVR CPU state and instruction execution
│   │       ├── opcodes.rs       # Instruction decoder (16/32-bit → enum)
│   │       ├── memory.rs        # Data space, flash, EEPROM
│   │       ├── display.rs       # SSD1306 OLED controller (contrast/invert)
│   │       ├── pcd8544.rs       # PCD8544 Nokia LCD controller
│   │       ├── hex.rs           # Intel HEX parser
│   │       ├── disasm.rs        # Instruction disassembler (debugger)
│   │       ├── audio_buffer.rs  # Sample-accurate waveform buffer
│   │       ├── arduboy_file.rs  # .arduboy ZIP file parser
│   │       ├── png.rs           # PNG encoder (no dependencies)
│   │       ├── gif.rs           # Animated GIF encoder (LZW compressed)
│   │       └── peripherals/
│   │           ├── timer8.rs    # Timer/Counter0 (millis/delay)
│   │           ├── timer16.rs   # Timer/Counter1 & 3 (audio tone)
│   │           ├── timer4.rs    # Timer/Counter4 (10-bit high-speed PWM)
│   │           ├── spi.rs       # SPI master controller
│   │           ├── adc.rs       # ADC (random seed)
│   │           ├── pll.rs       # PLL frequency synthesizer
│   │           ├── eeprom.rs    # EEPROM controller
│   │           └── fx_flash.rs  # W25Q128 external flash (16 MB)
│   └── frontend-minifb/         # Desktop frontend
│       └── src/main.rs          # Window, stereo audio, gamepad, debugger
└── roms/                        # Test ROM directory
```

### Emulation Loop

Each frame (~13.5 ms at 60 FPS):

1. Poll keyboard and gamepad → set GPIO pin states
2. Execute CPU instructions until 216,000 cycles elapsed (with breakpoint checks)
3. Flush SPI buffer → route bytes to display or FX flash
4. Update timers and fire pending interrupts
5. Read tone frequency (Timer3 / Timer1 / GPIO) → update stereo audio
6. Capture USB serial output bytes
7. Blit RGBA framebuffer to window at configurable scale

### Audio (Stereo, Sample-Accurate)

GPIO bit-bang audio is rendered sample-accurately using a per-frame edge buffer.
Timer-driven audio falls back to frequency-based square wave synthesis.

| Channel | Priority | Method | Mechanism | Example |
|---------|----------|--------|-----------|---------|
| Left  | 1 | Timer3 CTC | OC3A (PC6) toggle on compare match | Arduboy2 `tone()` |
| Left  | 2 | Timer4 CTC | OC4A toggle on compare match | PWM audio games |
| Left  | 3 | Timer2 CTC | ISR toggles PD3 (328P only) | Gamebuino Classic |
| Left  | 4 | GPIO bit-bang | Direct PORTC bit 6 / PORTD bit 3 toggling | Arcodia |
| Right | 1 | Timer1 CTC | OC1A (PB5) toggle on compare match | Dual-tone games |
| Right | 2 | GPIO bit-bang | Direct PORTB bit 5 toggling | Custom engines |

A toggleable post-processing pipeline (A key) improves audio quality:
sub-sample edge interpolation → Butterworth LPF (8 kHz speaker sim) → DC-blocking HPF (20 Hz) → click suppression envelope (2 ms attack / 5 ms release) → stereo crossfeed (20%).

## Tested Games

- **Nineteen44** — Scrolling shooter (Timer3 audio, complex SPI)
- **Arcodia** — Space Invaders clone (GPIO bit-bang audio)
- **101 Starships** — Fleet management game
- Various Arduboy2 library games

## Roadmap

See [ROADMAP.md](ROADMAP.md) for a detailed feature comparison with ProjectABE
and the planned development phases toward v1.0.0.

See [CHANGELOG.md](CHANGELOG.md) for the release history.

## Notice

This software was generated by AI (Claude by Anthropic) through interactive
development sessions with a human operator. No code from existing emulator
projects (such as ProjectABE) was used. The implementation is based solely
on publicly available hardware datasheets (ATmega32u4, SSD1306, PCD8544,
W25Q128) and the Intel HEX format specification.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
