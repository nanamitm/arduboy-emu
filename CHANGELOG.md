# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Tones after the first one were cut to a click** — A stopped 8/16-bit timer (`CSn2:0 = 0`) kept its last update timestamp, so the first update after the clock was switched back on replayed the whole stopped interval at once. Restarting a timer after a one-second pause queued ~125 compare matches, which then fired back-to-back and drained the tone's toggle budget in ~16 ms. ArduboyTones stops Timer3 between tones, so in practice every tone after the first was silent — for every game using the library. Timers now resync their timestamp when the clock select goes from stopped to running.
- **Compare-match / overflow flags on the 16-bit timers** are single flags again (`OCFnA/B/C`, `TOVn`), as on the hardware, instead of a queue of pending matches that could be drained one per peripheral update.
- **Speaker 2 was read from the wrong pin** — GPIO audio for the right channel watched PB5 (the blue RGB LED on the Arduboy) instead of PC7. High-volume/bridge mode (`ArduboyTones::volumeMode(VOLUME_ALWAYS_HIGH)`, `TONE_HIGH_VOLUME`) is now audible, LED activity no longer leaks into the audio, and the pin is only sampled on the ATmega32u4.
- **Timer3 `TCNT3H`/`TCNT3L` addresses were swapped** (`TCNT3L` is 0x94, `TCNT3H` is 0x95); Timer1 was already correct.

### Added

- `timer3_restart_does_not_replay_stopped_interval` regression test: mimics a tone library by running Timer3 in CTC with the compare-match interrupt, stopping the counter, idling ~1 M cycles, restarting, and pinning the number of interrupts in a fixed window (6; it was 129 before the fix).

## [0.8.1] - 2025-02-18

### Added

- **Save states** — Quick save (F5) / quick load (F9) with full emulator state persistence to `.state` files. Uses bincode serialization with deflate compression, magic number validation, format versioning, and CPU type checking (ATmega32u4 ↔ ATmega328P mismatch prevention). Loading a save state clears the rewind buffer.

### Changed

- **Windows binary renamed** — `arduboy-frontend.exe` → `arduboy-emu.exe` for consistency with Linux/macOS
- All installer scripts (Windows InnoSetup/WiX, Linux deb/rpm, macOS pkg/dmg) updated for new binary name
- Version bumped to 0.8.1

## [0.7.3] - 2025-02-14

### Fixed

- **Linux segfault on launch** — minifb `AspectRatioStretch` caused out-of-bounds access in X11 `image_resize_linear_stride`. Changed to `UpperLeft` since we handle scaling ourselves.
- **Gamepad buttons ignored on Linux** — Generic controllers without gilrs mapping DB entries reported all buttons as `Unknown`. Added raw evdev code fallback: even codes → A, odd codes → B.
- **RPM build failure** — Disabled debuginfo subpackage (`%define debug_package %{nil}`) for pre-stripped binary. Removed duplicate doc/license install in `%install` section.

### Changed

- Version bumped to 0.7.3
- Gamepad debug logging now shows raw button/axis events with `--debug` flag

## [0.7.2] - 2025-02-14

### Added

- **MSI installer** — WiX v5/v6 based `.msi` package for Windows with per-user install, major upgrade support, file associations, and Start Menu/Desktop shortcuts
- **Console window hidden** — Release builds use `windows_subsystem = "windows"` to suppress console window on double-click launch

### Changed

- Version bumped to 0.7.2
- Removed ROM directory bundling from Inno Setup installer to prevent accidental inclusion of copyrighted game files

## [0.7.1] - 2025-02-14

### Added

- **PWM DAC audio** — Gamebuino Classic games using Timer2 Fast PWM on OC2B (PD3) for analog audio output are now supported. OCR2B sample values are recorded with CPU-tick timestamps and resampled via sample-and-hold interpolation through the full DSP pipeline.
- **Portrait rotation** (V key) — Rotates display 90° CCW (left side becomes bottom). Works with all rendering modes. Title bar shows `[PORT]` indicator.
- **Timer2 prescaler table** — ATmega328P Timer2 has a different prescaler mapping from Timer0 (CS3=/32 vs /64). Added `is_timer2` flag for correct lookup.
- **Installer build scripts** — Cross-platform packaging for distribution:
  - Windows: Inno Setup `.iss` script + `build-windows.bat` → `.exe` installer with Start Menu, desktop icon, file associations
  - Linux: `build-linux.sh` → `.deb` (Debian/Ubuntu) and `.rpm` (Fedora/RHEL) with desktop entry, MIME types, AppStream metadata
  - macOS: `build-macos.sh` → `.app` bundle + `.pkg` installer + `.dmg` disk image, with universal binary support (`--universal`) and code signing (`--sign`)
  - GitHub Actions workflow (`.github/workflows/release.yml`) for automated CI/CD builds on tag push

### Changed

- Version bumped to 0.7.1
- Audio buffer expanded with PWM sample path (`push_pwm_sample`, `sample_pwm`)

## [0.7.0] - 2025-02-13

### Added

- **CPU auto-detection** — Automatically identifies ATmega328P (Gamebuino Classic) vs ATmega32u4 (Arduboy) binaries by analyzing the interrupt vector table size. No more `--cpu 328p` flag needed; just run `arduboy-emu game.hex`. Also works during game switching (N/P keys).
- **ELF/DWARF debug support** (`elf.rs`, ~280 lines) — AVR ELF parser with:
  - Flash loading from PT_LOAD segments
  - Symbol table extraction (function names → byte addresses)
  - DWARF `.debug_line` parser (versions 2–4) for source file:line ↔ PC mapping
  - `describe_pc()` for combined function+source display
- **`.elf` file loading** — `arduboy-emu game.elf` with automatic debug info extraction
- **Rewind** (`snapshot.rs`, ~140 lines) — hold Backspace to rewind gameplay:
  - Ring buffer of 600 snapshots (every 0.5s = ~5 min rewind)
  - Saves CPU state, SRAM, EEPROM, display framebuffer
  - `Arduboy::save_snapshot()` / `restore_snapshot()` API
- **Audio post-processing pipeline** (`audio_buffer.rs`, ~395 lines) — Five-stage DSP chain:
  - Sub-sample edge interpolation: time-weighted integration eliminates aliasing
  - Butterworth LPF (8 kHz): simulates piezo speaker bandwidth rolloff
  - DC-blocking HPF (20 Hz): removes sub-audible drift from LPF
  - Click suppression envelope: 2 ms attack / 5 ms release fade
  - Stereo crossfeed: 20% opposite-channel blend for natural headphone listening
  - **A key** toggles filters on/off at runtime, `[FILT]` indicator in title bar
- **`--lcd`** CLI option to start with LCD display effect enabled
- **`--no-blur`** CLI option to start with blur filter disabled

### Changed

- `load_game_file()` now supports `.elf` file extension alongside `.hex` and `.arduboy`
- Game scanner includes `.elf` files in directory browse (N/P keys)
- Version bumped to 0.7.0 throughout

### Fixed

- **Gamebuino Classic audio silent** — ATmega328P speaker pin PD3 (Arduino D3) had no edge detection in the PORTD write handler. Gamebuino's sound library uses Timer2 COMPA ISR to toggle PD3 via `SBI PIND,3`, but the emulator only tracked PC6 (32u4 Arduboy speaker). Added PD3 edge detection for 328P mode, reusing the speaker1 state fields (PC6 is unused on 328P). Audio now flows through the full sample-accurate pipeline including LPF, envelope, and crossfeed.
- **Gamebuino Classic black screen** — PCD8544 DC/CS pin mapping was wrong. Binary analysis of 3D-DEMO.HEX revealed the actual Gamebuino Classic pin assignment: DC=A2(PC2), CS=A1(PC1), RST=A0(PC0). The emulator had DC=PC0 (confusing DC with RST). The `digitalWrite` function writes port registers via `ST X` (indirect store through pin lookup tables in flash), going through `write_data()`. Fixed by setting correct default pin mapping (DC=PC2, CS=PC1) for ATmega328P, with runtime auto-detection fallback for non-standard pin configurations.
- **Timer8 interrupt priority order** — Timer8 `check_interrupt()` fired TOV before COMPA/COMPB, but ATmega328P datasheet specifies COMPA > COMPB > OVF. Reordered to match hardware. Affects Timer2 COMPA-driven audio on Gamebuino where incorrect priority could delay sound ISR dispatch.
- **FX games black screen** — SPI bus routing was exclusive (FX **or** display), but real hardware has a shared bus where both chips receive all bytes simultaneously. Display commands sent while FX CS was coincidentally LOW (e.g., during boot before explicit deselect) were swallowed by the FX state machine and never reached the SSD1306. Now both targets are routed independently, matching real hardware behavior.
- **ISR-driven audio silent** — Games using `SBI PINC,6` in Timer3 ISR to toggle the speaker (1-bit audio from FX flash data) produced no sound. The PINx toggle handler (`write_data(0x26)`) performed `PORTC ^= value` and returned immediately, bypassing the PORTC speaker edge detection that records transitions into the audio buffer. Fixed all PINx toggle handlers (PINB/C/D/E/F) to re-invoke `write_data` on the corresponding PORTx address so all side effects (speaker detection, LED tracking, SPI routing) fire correctly.
- **Timer16 CTC mode gated on interrupt enable** — Compare match detection and counter reset in CTC mode were incorrectly gated on `ocie_a` (interrupt enable). Real hardware always resets the counter at OCR_A in CTC mode regardless of interrupt settings. Also fixed: CTC wrap-around now handles multiple periods per update interval; removed incorrect FOC gating on interrupt dispatch.
- **ZIP data descriptor support** — `.arduboy` files created by macOS (ditto/Finder) use data descriptors (bit 3 of general purpose flag) where `compressed_size=0` in the local file header. Rewrote ZIP parser to use Central Directory for reliable sizes. Custom inflate replaced with `miniz_oxide` crate for robust decompression.
- **FX flash layout** — FX data was placed at offset 0 instead of right-aligned at end of 16MB flash. Games hardcode `FX_DATA_PAGE` (e.g., `0xF5B3`), reading from the wrong offset returned 0xFF. Implemented `load_fx_layout()` with correct right-aligned placement algorithm.
- **EIJMP/EICALL overflow** — `(eind << 16) | z` caused arithmetic overflow on u16. Removed EIND shift (irrelevant for ATmega32u4's 16-bit PC with 32KB flash).
- **Framebuffer type mismatch** — Snapshot restore used `[u8; 32768]` array where `Vec<u8>` was expected. Added `.to_vec()` conversion.
- **Dead assignment warning** — Removed unused `pos` assignment in ELF DWARF file name table parser.

## [0.6.0] - 2025-02-13

### Added

- **Execution profiler** (`profiler.rs`, 222 lines) — PC histogram, top-N hotspot analysis with disassembly, call graph tracking (CALL/RCALL/ICALL/RET), flat profile (hot regions), CPI metrics
- **Advanced debugger** (`debugger.rs`, 348 lines) — RAM hex+ASCII viewer (`dump_ram`), RAM diff viewer (`dump_ram_diff`), I/O register viewer with named registers for both ATmega32u4 and ATmega328P, data watchpoints (read/write/read-write with value match)
- **GDB Remote Serial Protocol server** (`gdb_server.rs`, 472 lines) — TCP-based RSP for avr-gdb connection: register read/write, memory read/write (flash + SRAM address mapping), software breakpoints, single step, continue, vCont, Ctrl+C interrupt
- **`--gdb <port>`** CLI option to start GDB server mode
- **`--profile`** CLI option to auto-enable profiler with report on exit
- **`--watch <addr>`** CLI option for data watchpoints (repeatable)
- **T key** in GUI mode to toggle profiler (start/stop with report to stderr)
- **Interactive debugger commands** in step mode: `ram <addr>`, `io`/`io all`, `w`/`wl`/`wd` (watchpoints), `prof start/stop/report`, `snap`/`ramdiff`, `f` (run frame), `b`/`bl`/`bd` (breakpoints)
- Title bar `[PROF]` indicator when profiler is active
- Watchpoint hit detection integrated into `run_frame()` loop
- Profiler hooks in `step()` for CALL/RCALL/ICALL/RET tracking
- `Arduboy::dump_ram()`, `dump_io()`, `dump_io_all()`, `profiler_report()`, `gdb_regs()` convenience methods
- **LCD effect** (L key toggle) — display-accurate rendering with 4 layers:
  - Color palette: SSD1306 blue-white OLED / PCD8544 yellow-green LCD
  - Pixel grid lines: subtle darkening at pixel cell boundaries
  - Temporal blend: 20% ghosting for PCD8544 (LCD response delay), 5% for SSD1306
  - Dot corner rounding: darkened corners for organic pixel shape (3× and above)
- **Soft blur** (B key toggle) — 3×3 weighted box filter post-process for pixel smoothing
- **Aspect-ratio-preserving window resize** — maintains 2:1 ratio with integer scaling on drag resize
- Title bar `[LCD]` and `[BLUR]` indicators

### Changed

- Step mode (`--step`) upgraded from simple stepper to full interactive debugger
- Watchpoint checks integrated into `read_data()` and `write_data()` paths (zero-cost when no watchpoints set)
- `Arduboy` struct now contains `profiler::Profiler` and `debugger::Debugger` fields

## [0.5.0] - 2025-02-13

### Added

- **ATmega328P CPU support** — `--cpu 328p` CLI option for Gamebuino Classic / Arduino Uno
- **`CpuType` enum** — `Atmega32u4` (default) / `Atmega328p` selection at construction
- **`Arduboy::new_with_cpu()`** — Constructor with explicit CPU type
- **Timer2 peripheral** — 8-bit asynchronous timer for ATmega328P (reuses Timer8 with dedicated register addresses and interrupt vectors)
- **ATmega328P interrupt vector table** — 26-entry vector table (separate from 32u4's 43-entry table)
- **ATmega328P memory map** — SRAM 2 KB (vs 32u4's 2.5 KB), dynamic `Memory::new_with_size()`
- **Gamebuino Classic button mapping** — 328P pin layout: UP=PB1, DOWN=PD6, LEFT=PB0, RIGHT=PD7, A=PD4, B=PD2
- **328P PCD8544 SPI routing** — CS=PC2, DC=PC3 for Gamebuino Classic display

### Changed

- Timer8 and Timer16 now take interrupt vector addresses via `Addrs` struct (no hardcoded vectors)
- `peripherals::mod.rs` reorganized with separate 32u4 and 328P vector constant blocks
- Timer3, Timer4, USB serial conditionally active based on `cpu_type`
- SPI output tuple expanded to 4-tuple `(byte, portd, portf, portc)` for 328P PORTC routing
- `Memory` doc updated for dual-CPU support

## [0.4.0] - 2025-02-13

### Added

- **`.arduboy` file support**: ZIP archive parser with `info.json` metadata
  extraction, automatic `.hex` and FX `.bin` file detection
  - Minimal ZIP reader (stored + deflate) with RFC 1951 inflate
  - Simple JSON string value extractor for title/author
- **EEPROM persistence**: Automatic save/load to `.eep` file alongside game
  - Auto-save every 10 seconds when dirty, auto-load on startup
  - `--no-save` flag to disable persistence
  - Saved before hot-reload and game switch
- **GIF recording**: G key toggles recording, LZW-compressed animated GIF
  - Custom GIF89a encoder with Netscape infinite loop extension
  - 2-color (monochrome) palette for Arduboy's 1-bit display
  - Saved on stop or window close (`recording_NNNN.gif`)
- **PNG screenshots at any scale**: S key saves at current display scale
  - 1× scale: efficient 8-bit grayscale PNG (monochrome)
  - 2×–6× scale: RGBA PNG with nearest-neighbor upscale
  - Custom PNG encoder (no dependencies, stored deflate blocks)
- **RGB LED state tracking**: Red (PB6), Green (PB7), Blue (PB5) from PORTB
  - TX LED (PD5, active-low) and RX LED (PB0, active-low) detection
  - LED state displayed in window title bar
- **FPS limiter toggle**: F key switches between 60fps and unlimited
- **Game browser**: Scan current directory for `.hex`/`.arduboy` files
  - N key = load next game, P key = load previous game
  - O key = print numbered file list to terminal
  - Alphabetical sorting, circular navigation
  - EEPROM saved/loaded per game automatically
- **Hot reload**: R key reloads current game file from disk

### Changed

- Screenshot format: BMP → PNG (smaller files, widely supported)
- Screenshot naming: `screenshot_NNNN_Sx.png` includes scale factor
- Window title shows LED state, recording indicator, FPS mode

## [0.3.0] - 2025-02-13

### Added

- **Timer4 (10-bit high-speed)**: ATmega32u4 Timer/Counter4 emulation
  - Normal, CTC, and PWM modes with OCR4C as TOP
  - Extended prescaler (/1 through /16384)
  - 10-bit counter with TC4H high-byte buffer register
  - OCR4A/B/C/D compare registers, dead time (DT4)
  - Overflow and compare-match interrupts (TIMSK4/TIFR4)
  - Tone detection in CTC mode for audio output
- **Sample-accurate audio waveform buffer**: `AudioBuffer` module
  - Records pin-level transitions with CPU tick timestamps per frame
  - Converts edge buffers to stereo interleaved PCM at target sample rate
  - Hybrid audio source in frontend: sample-accurate PCM when GPIO edges
    exist, automatic fallback to timer frequency synthesis otherwise
  - Ring buffer between main thread and rodio audio thread

### Changed

- Audio source replaced: `StereoSquareWave` → `HybridAudioSource`
  with `Arc<Mutex<VecDeque<f32>>>` ring buffer for sample-accurate output
- Timer tone priority: Timer3 > Timer4 > GPIO PC6 (left),
  Timer1 > GPIO PB5 (right)

## [0.2.0] - 2025-02-13

### Added

- **Disassembler**: `disasm` module with `disassemble()`, `format_sreg()`,
  `disassemble_range()` for instruction-level debugging
- **Breakpoints**: `--break <addr>` CLI option (repeatable), hex byte-address,
  stops execution when PC matches
- **Step mode**: `--step` interactive debugger with commands:
  Enter=step, N=step N, r=run to break, d=dump, q=quit
- **Register dump**: `dump_regs()` showing R0-R31, PC, SP, SREG flags, X/Y/Z
  pairs; D key in GUI mode, integrated into step and breakpoint output
- **SSD1306 display invert**: 0xA6 (normal) / 0xA7 (inverse) commands,
  XOR applied during framebuffer rendering
- **SSD1306 contrast control**: 0x81 command, brightness scaled by contrast
  byte (0x00=black, 0xFF=full)
- **Window scale toggle**: 1–6 number keys change scale in GUI mode
- **Fullscreen**: F11 toggles borderless fullscreen (12× scale)
- **Screenshot**: S key saves BMP file (screenshot_NNNN.bmp)
- **Scale CLI option**: `--scale N` sets initial scale (1–6)
- **2-channel stereo audio output**: Timer3 → left, Timer1 → right,
  GPIO PC6 → left fallback, GPIO PB5 → right fallback
- **GPIO speaker 2 (PB5)**: Bit-bang edge detection for right channel
- **USB Serial emulation**: UENUM, UEDATX, UEINTX register handling,
  CDC endpoint capture (EP3+), --serial flag outputs to stderr
- **USB register stubs**: USBCON, USBSTA, UDADDR, UESTA0X, UEBCLX for
  programs that check USB state

## [0.1.0] - 2025-02-13

Initial release.

### Added

- **AVR CPU core**: 80+ ATmega32u4 instructions with accurate flag computation
  - Arithmetic: ADD, ADC, SUB, SUBI, SBC, SBCI, AND, ANDI, OR, ORI, EOR, COM,
    NEG, INC, DEC, MUL, MULS, MULSU, FMUL, FMULS, ADIW, SBIW
  - Compare: CP, CPC, CPI, CPSE
  - Branch: RJMP, RCALL, RET, RETI, JMP, CALL, IJMP, ICALL, BRBS, BRBC,
    SBRC, SBRS, SBIC, SBIS
  - Data transfer: MOV, MOVW, LDI, LDS, STS, LD/ST with X/Y/Z (plain,
    post-increment, pre-decrement, displacement)
  - I/O: IN, OUT, SBI, CBI, PUSH, POP
  - Shift/Bit: LSR, ASR, ROR, SWAP, BST, BLD
  - Program memory: LPM (3 modes), ELPM (3 modes, RAMPZ:Z 24-bit addressing)
  - Status register: SEI, CLI, SEC, CLC, SEN, CLN, SEZ, CLZ, SEV, CLV,
    SES, CLS, SEH, CLH, SET, CLT
  - Misc: NOP, SLEEP, WDR
- **SSD1306 OLED display**: 128×64 monochrome, horizontal/vertical addressing,
  column/page windowing, command processing
- **PCD8544 Nokia LCD**: 84×48 monochrome (Gamebuino Classic), auto-detected,
  centered in 128×64 framebuffer
- **Audio output** (3 detection methods):
  - Timer3 CTC mode (standard Arduboy `tone()`)
  - Timer1 CTC mode
  - GPIO bit-bang (PC6 pin toggle via `digitalWrite`)
- **Peripherals**:
  - Timer0 (8-bit): Normal, CTC, Fast PWM modes with prescaler, overflow and
    compare-match interrupts (millis/delay support)
  - Timer1/Timer3 (16-bit): CTC mode, compare-match toggle, tone generation
  - SPI master controller (instant transfer, SPIF flag)
  - ADC with pseudo-random output (xorshift PRNG)
  - PLL frequency synthesizer (instant lock)
  - EEPROM controller (1 KB, read/write via EECR)
- **Arduboy FX**: W25Q128 16 MB SPI flash emulation
  - Commands: Read Data (0x03), Fast Read (0x0B), JEDEC ID (0x9F),
    Release Power Down (0xAB), Read Status (0x05), Power Down (0xB9),
    Write Enable/Disable (0x06/0x04), Page Program (0x02), Sector Erase (0x20)
  - Lazy allocation (16 MB allocated only on first use)
  - Auto-detection of `.bin` / `-fx.bin` companion files
- **Input**: Keyboard (arrows, Z/X) + gamepad (gilrs, cross-platform, hot-plug)
  with OR-combined merging and analog stick deadzone
- **Frontend**: minifb window with 6× nearest-neighbor scaling,
  rodio square wave audio, FPS counter, mute toggle (M key)
- **Headless mode**: `--headless` with `--frames`, `--press`, `--snapshot`,
  `--debug` options, Unicode half-block display rendering
- **Intel HEX parser**: Record types 00 (data), 01 (EOF), 02 (extended segment)
- **Debug output**: `--debug` flag gates all diagnostic `eprintln!` output
