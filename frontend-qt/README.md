# arduboy-qt — Qt6/C++ GUI client

A native desktop GUI for the `arduboy-core` emulator, written in Qt6/C++ and
linked to the Rust core through the `arduboy_ffi` C ABI (see
[`crates/ffi`](../crates/ffi)).

```
┌──────────────┐   C ABI    ┌───────────────┐   Rust    ┌──────────────┐
│  arduboy-qt  │ ─────────► │  arduboy_ffi  │ ────────► │ arduboy-core │
│  (Qt6/C++)   │  (cdylib)  │ (extern "C")  │           │  (emulator)  │
└──────────────┘            └───────────────┘           └──────────────┘
```

The C++ side holds no emulation logic — it renders the framebuffer, plays audio,
and forwards input. All state lives in the Rust core.

## Features

- Load `.hex`, `.arduboy`, and `.elf` ROMs (companion FX `.bin` and `.eep`
  EEPROM files auto-detected), with CPU auto-detection (ATmega32u4 / ATmega328P)
- Scaled display (1×–6×), aspect-correct letterboxing, nearest / smooth scaling,
  fullscreen (F11)
- Keyboard input: **Arrows** = D-pad, **Z** = A, **X** = B
- Stereo audio via `QAudioSink` (sample-accurate output from the core)
- Save/load state (F5/F9), PNG screenshots (S), animated GIF recording (G)
- EEPROM auto-save on exit, reset (Ctrl+R), pause (P), mute (M)
- Register view dock, RGB/TX/RX LED indicators, live FPS, CPU-type readout
- Menus for every action; pass a ROM path as the first CLI argument

## Prerequisites

- **Qt 6** with the **Widgets** and **Multimedia** modules (tested with 6.11.1,
  `msvc2022_64` kit)
- **CMake ≥ 3.21** and a generator (Ninja recommended — bundled with Qt)
- **Rust / cargo** (the `arduboy_ffi` cdylib is built automatically by CMake)
- A C++17 compiler matching the Qt kit's ABI (MSVC for `msvc2022_64`)

> **ABI note:** the Rust cdylib and the Qt app must share an ABI. The default
> Rust target on Windows is `x86_64-pc-windows-msvc`, which matches the
> `msvc2022_64` Qt kit. If you use a MinGW Qt kit, build Rust for
> `x86_64-pc-windows-gnu` instead.

## Building

### Windows (MSVC + Ninja)

From a **Developer Command Prompt** (so `cl.exe` is on `PATH`):

```bat
cd frontend-qt
cmake -S . -B build -G Ninja ^
      -DCMAKE_BUILD_TYPE=Release ^
      -DCMAKE_PREFIX_PATH=C:\Qt\6.11.1\msvc2022_64
cmake --build build
```

Deploy the Qt runtime DLLs beside the executable (once):

```bat
C:\Qt\6.11.1\msvc2022_64\bin\windeployqt.exe --release build\arduboy-qt.exe
```

CMake copies `arduboy_ffi.dll` next to `arduboy-qt.exe` automatically.

### Linux / macOS

```bash
cd frontend-qt
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_PREFIX_PATH="$(qmake6 -query QT_INSTALL_PREFIX)"
cmake --build build
```

## Running

```bash
./build/arduboy-qt path/to/game.hex
```

Or launch without arguments and use **File ▸ Open ROM…**.

## Controls

| Action        | Key        | Action         | Key    |
|---------------|------------|----------------|--------|
| D-pad         | Arrow keys | Reload ROM     | R      |
| A button      | Z          | Screenshot     | S      |
| B button      | X          | GIF record     | G      |
| Pause         | P          | Save / Load    | F5/F9  |
| Mute          | M          | Scale 1×–6×    | 1–6    |
| Reset         | Ctrl+R     | Fullscreen     | F11    |
| Open ROM      | Ctrl+O     | Quit           | Ctrl+Q |
