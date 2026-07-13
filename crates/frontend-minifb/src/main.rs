//! Arduboy emulator frontend v0.8.1.
//!
// Hide the console window on Windows in release builds.
// Debug builds still show it for eprintln!() diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//!
//! Provides four execution modes:
//!
//! - **GUI mode** (default): Scaled window with stereo audio, keyboard/gamepad input,
//!   dynamic scale toggle, PNG screenshot, GIF recording, EEPROM persistence,
//!   runtime game browser, LCD effect, profiler toggle.
//! - **Headless mode** (`--headless`): Automated testing with ASCII snapshots.
//! - **Step mode** (`--step`): Interactive debugger with RAM viewer, I/O register
//!   viewer, watchpoints, breakpoints, and execution profiler.
//! - **GDB mode** (`--gdb <port>`): GDB Remote Serial Protocol server for
//!   connection from avr-gdb or compatible clients.
//!
//! ## v0.8.1 features
//! - Quick save (F5) / quick load (F9) with full emulator state persistence
//! - Windows binary renamed from `arduboy-frontend.exe` to `arduboy-emu.exe`
//! - Interactive debugger: `ram`, `io`, `w` (watchpoint), `prof`, `snap`/`ramdiff`
//! - Execution profiler: PC histogram, hotspot analysis, call graph tracking
//! - GDB Remote Serial Protocol server (`--gdb <port>`)
//! - Data watchpoints (`--watch <addr>` CLI, `w` in step mode)
//! - LCD effect (L key): display-accurate colors, pixel grid, ghosting, dot rounding
//! - Profiler toggle (T key) in GUI mode

use arduboy_core::{
    detect_cpu_type, Arduboy, Button, CpuType, DisplayType, SCREEN_HEIGHT, SCREEN_WIDTH,
};
use gilrs::{Axis, Button as GilrsButton, Event as GilrsEvent, EventType, Gilrs};
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use std::env;
use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Audio output sample rate in Hz
const AUDIO_SAMPLE_RATE: u32 = 44100;
/// Square wave amplitude (0.0–1.0)
const AUDIO_VOLUME: f32 = 0.15;
/// Analog stick deadzone
const STICK_DEADZONE: f32 = 0.3;
/// Analog trigger deadzone
const TRIGGER_DEADZONE: f32 = 0.2;

// ─── Audio Sources ──────────────────────────────────────────────────────────

/// Hybrid audio source: uses sample-accurate PCM from ring buffer when
/// available (GPIO bit-bang), falls back to square wave synthesis for
/// timer-driven tones.
struct HybridAudioSource {
    ring: Arc<std::sync::Mutex<std::collections::VecDeque<f32>>>,
    freq_l: Arc<AtomicU32>,
    freq_r: Arc<AtomicU32>,
    sample_rate: u32,
    phase_l: f32,
    phase_r: f32,
    left_next: bool,
}

impl HybridAudioSource {
    fn new(
        ring: Arc<std::sync::Mutex<std::collections::VecDeque<f32>>>,
        freq_l: Arc<AtomicU32>,
        freq_r: Arc<AtomicU32>,
        sample_rate: u32,
    ) -> Self {
        HybridAudioSource {
            ring,
            freq_l,
            freq_r,
            sample_rate,
            phase_l: 0.0,
            phase_r: 0.0,
            left_next: true,
        }
    }
}

impl Iterator for HybridAudioSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Try to drain from sample-accurate ring buffer first
        if let Ok(mut ring) = self.ring.try_lock() {
            if !ring.is_empty() {
                return ring.pop_front();
            }
        }
        // Fallback: synthesize square wave from timer frequencies
        if self.left_next {
            self.left_next = false;
            let freq = f32::from_bits(self.freq_l.load(Ordering::Relaxed));
            if freq <= 0.0 {
                self.phase_l = 0.0;
                return Some(0.0);
            }
            let s = if self.phase_l < 0.5 {
                AUDIO_VOLUME
            } else {
                -AUDIO_VOLUME
            };
            self.phase_l += freq / self.sample_rate as f32;
            self.phase_l %= 1.0;
            Some(s)
        } else {
            self.left_next = true;
            let freq = f32::from_bits(self.freq_r.load(Ordering::Relaxed));
            if freq <= 0.0 {
                self.phase_r = 0.0;
                return Some(0.0);
            }
            let s = if self.phase_r < 0.5 {
                AUDIO_VOLUME
            } else {
                -AUDIO_VOLUME
            };
            self.phase_r += freq / self.sample_rate as f32;
            self.phase_r %= 1.0;
            Some(s)
        }
    }
}

impl rodio::Source for HybridAudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn setup_audio(
    ring: Arc<std::sync::Mutex<std::collections::VecDeque<f32>>>,
    freq_l: Arc<AtomicU32>,
    freq_r: Arc<AtomicU32>,
) -> Option<(rodio::OutputStream, rodio::OutputStreamHandle, rodio::Sink)> {
    match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => match rodio::Sink::try_new(&handle) {
            Ok(sink) => {
                let source = HybridAudioSource::new(ring, freq_l, freq_r, AUDIO_SAMPLE_RATE);
                sink.append(source);
                Some((stream, handle, sink))
            }
            Err(e) => {
                eprintln!("Warning: audio sink: {}", e);
                None
            }
        },
        Err(e) => {
            eprintln!("Warning: audio device: {}", e);
            None
        }
    }
}

// ─── Gamepad ────────────────────────────────────────────────────────────────

struct GamepadState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    a: bool,
    b: bool,
    left_stick_x: f32,
    left_stick_y: f32,
}

impl GamepadState {
    fn new() -> Self {
        GamepadState {
            up: false,
            down: false,
            left: false,
            right: false,
            a: false,
            b: false,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
        }
    }
    fn eff_up(&self) -> bool {
        self.up || self.left_stick_y < -STICK_DEADZONE
    }
    fn eff_down(&self) -> bool {
        self.down || self.left_stick_y > STICK_DEADZONE
    }
    fn eff_left(&self) -> bool {
        self.left || self.left_stick_x < -STICK_DEADZONE
    }
    fn eff_right(&self) -> bool {
        self.right || self.left_stick_x > STICK_DEADZONE
    }
}

fn init_gamepad(debug: bool) -> Option<Gilrs> {
    match Gilrs::new() {
        Ok(gilrs) => {
            if debug {
                let mut found = false;
                for (id, gp) in gilrs.gamepads() {
                    println!("Gamepad: [{}] \"{}\" ({})", id, gp.name(), gp.os_name());
                    found = true;
                }
                if !found {
                    println!("No gamepad (hot-plug supported).");
                }
            }
            Some(gilrs)
        }
        Err(e) => {
            eprintln!("Warning: gamepad: {}", e);
            None
        }
    }
}

fn poll_gamepad(gilrs: &mut Gilrs, state: &mut GamepadState, debug: bool) {
    while let Some(GilrsEvent { event, id, .. }) = gilrs.next_event() {
        if debug {
            match &event {
                EventType::ButtonPressed(b, code) => {
                    eprintln!("  GP[{}] ButtonPressed: {:?} code={:?}", id, b, code)
                }
                EventType::ButtonReleased(b, code) => {
                    eprintln!("  GP[{}] ButtonReleased: {:?} code={:?}", id, b, code)
                }
                EventType::AxisChanged(a, v, code) => eprintln!(
                    "  GP[{}] AxisChanged: {:?} val={:.3} code={:?}",
                    id, a, v, code
                ),
                EventType::Connected => eprintln!("  GP[{}] Connected", id),
                EventType::Disconnected => eprintln!("  GP[{}] Disconnected", id),
                _ => {}
            }
        }
        match event {
            EventType::ButtonPressed(b, code) => apply_button_ext(state, b, code, true),
            EventType::ButtonReleased(b, code) => apply_button_ext(state, b, code, false),
            EventType::AxisChanged(a, v, _) => apply_axis(state, a, v),
            EventType::Connected => {
                if debug {
                    for (_, gp) in gilrs.gamepads() {
                        if gp.is_connected() {
                            println!("Gamepad connected: \"{}\"", gp.name());
                        }
                    }
                }
            }
            EventType::Disconnected => {
                if debug {
                    println!("Gamepad disconnected");
                }
                *state = GamepadState::new();
            }
            _ => {}
        }
    }
}

/// Map known gilrs buttons to Arduboy controls.
/// For Unknown buttons (generic controllers without gilrs mapping DB entry),
/// use the raw evdev code: even codes → A, odd codes → B.
fn apply_button_ext(
    state: &mut GamepadState,
    btn: GilrsButton,
    code: gilrs::ev::Code,
    pressed: bool,
) {
    match btn {
        GilrsButton::DPadUp => state.up = pressed,
        GilrsButton::DPadDown => state.down = pressed,
        GilrsButton::DPadLeft => state.left = pressed,
        GilrsButton::DPadRight => state.right = pressed,
        GilrsButton::South | GilrsButton::East | GilrsButton::Start => state.b = pressed,
        GilrsButton::West
        | GilrsButton::North
        | GilrsButton::LeftTrigger
        | GilrsButton::RightTrigger
        | GilrsButton::LeftTrigger2
        | GilrsButton::RightTrigger2
        | GilrsButton::Select => state.a = pressed,
        GilrsButton::Unknown => {
            // Generic controller fallback: split buttons by raw code parity
            // evdev codes 288,290,292... (even) → A;  289,291,293... (odd) → B
            let raw = code.into_u32() & 0xFFFF;
            if raw % 2 == 0 {
                state.a = pressed;
            } else {
                state.b = pressed;
            }
        }
        _ => {}
    }
}

fn apply_axis(state: &mut GamepadState, axis: Axis, value: f32) {
    match axis {
        Axis::LeftStickX => state.left_stick_x = value,
        Axis::LeftStickY => state.left_stick_y = value,
        Axis::RightStickX => {
            if state.left_stick_x.abs() < 0.01 {
                state.left_stick_x = value;
            }
        }
        Axis::RightStickY => {
            if state.left_stick_y.abs() < 0.01 {
                state.left_stick_y = value;
            }
        }
        Axis::DPadX => {
            state.left = value < -STICK_DEADZONE;
            state.right = value > STICK_DEADZONE;
        }
        Axis::DPadY => {
            state.up = value < -STICK_DEADZONE;
            state.down = value > STICK_DEADZONE;
        }
        Axis::LeftZ | Axis::RightZ => {
            if value > TRIGGER_DEADZONE {
                state.a = true;
            } else if value < 0.05 {
                state.a = false;
            }
        }
        _ => {}
    }
}

// ─── Screenshot (PNG) ───────────────────────────────────────────────────────

/// Save a screenshot at the current display scale (nearest-neighbor upscale).
fn save_screenshot_png(arduboy: &Arduboy, path: &str, scale: usize) -> Result<(), String> {
    if scale <= 1 {
        // 1x: save efficient monochrome PNG
        let fb = arduboy.framebuffer_rgba();
        let pixels: Vec<bool> = (0..SCREEN_WIDTH * SCREEN_HEIGHT)
            .map(|i| fb[i * 4] > 128)
            .collect();
        let png =
            arduboy_core::png::encode_png_mono(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, &pixels);
        fs::write(path, &png).map_err(|e| format!("{}: {}", path, e))
    } else {
        // Scaled: nearest-neighbor upscale to RGBA PNG
        let fb = arduboy.framebuffer_rgba();
        let sw = SCREEN_WIDTH * scale;
        let sh = SCREEN_HEIGHT * scale;
        let mut scaled = vec![0u8; sw * sh * 4];
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let si = (y * SCREEN_WIDTH + x) * 4;
                let r = fb[si];
                let g = fb[si + 1];
                let b = fb[si + 2];
                let a = fb[si + 3];
                for sy in 0..scale {
                    for sx in 0..scale {
                        let di = ((y * scale + sy) * sw + x * scale + sx) * 4;
                        scaled[di] = r;
                        scaled[di + 1] = g;
                        scaled[di + 2] = b;
                        scaled[di + 3] = a;
                    }
                }
            }
        }
        let png = arduboy_core::png::encode_png(sw as u32, sh as u32, &scaled);
        fs::write(path, &png).map_err(|e| format!("{}: {}", path, e))
    }
}

// ─── EEPROM Persistence ─────────────────────────────────────────────────────

fn eeprom_path(hex_path: &str) -> String {
    let p = std::path::Path::new(hex_path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    let dir = p.parent().unwrap_or(std::path::Path::new("."));
    dir.join(format!("{}.eep", stem))
        .to_string_lossy()
        .into_owned()
}

fn load_eeprom(arduboy: &mut Arduboy, path: &str, debug: bool) {
    if let Ok(data) = fs::read(path) {
        arduboy.load_eeprom(&data);
        if debug {
            eprintln!("EEPROM loaded: {} ({} bytes)", path, data.len());
        }
    }
}

fn save_eeprom(arduboy: &Arduboy, path: &str, debug: bool) {
    let data = arduboy.save_eeprom();
    // Only save if not all 0xFF (default/empty)
    if data.iter().any(|&b| b != 0xFF) {
        if let Err(e) = fs::write(path, &data) {
            eprintln!("EEPROM save error: {}: {}", path, e);
        } else if debug {
            eprintln!("EEPROM saved: {}", path);
        }
    }
}

// ─── File Loading ───────────────────────────────────────────────────────────

struct LoadedGame {
    hex_str: String,
    fx_data: Option<Vec<u8>>,
    fx_save: Option<Vec<u8>>,
    title: String,
    hex_path: String,
    /// Raw ELF bytes (when loading .elf files)
    elf_data: Option<Vec<u8>>,
}

fn load_game_file(
    path: &str,
    fx_override: Option<&str>,
    debug: bool,
) -> Result<LoadedGame, String> {
    let lower = path.to_lowercase();

    if lower.ends_with(".arduboy") {
        // Parse .arduboy ZIP
        let data = fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
        let ab = arduboy_core::arduboy_file::parse_arduboy(&data)?;
        if debug {
            eprintln!("Arduboy file: \"{}\" by {}", ab.title, ab.author);
            if let Some(ref fx) = ab.fx_data {
                eprintln!("  FX data: {} bytes", fx.len());
            }
        }
        Ok(LoadedGame {
            hex_str: ab.hex.ok_or("No HEX in .arduboy file")?,
            fx_data: ab.fx_data,
            fx_save: ab.fx_save,
            title: if ab.title.is_empty() {
                String::new()
            } else {
                ab.title
            },
            hex_path: path.to_string(),
            elf_data: None,
        })
    } else if lower.ends_with(".elf") {
        // ELF binary with debug info
        let data = fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
        // We store the raw ELF bytes; main() will call load_elf()
        Ok(LoadedGame {
            hex_str: String::new(), // not used for ELF
            fx_data: if let Some(fx_path) = fx_override {
                Some(fs::read(fx_path).map_err(|e| format!("{}: {}", fx_path, e))?)
            } else {
                auto_find_fx(path)
            },
            fx_save: None,
            title: String::new(),
            hex_path: path.to_string(),
            elf_data: Some(data),
        })
    } else {
        // Plain .hex file
        let hex_str = fs::read_to_string(path).map_err(|e| format!("{}: {}", path, e))?;
        let fx_data = if let Some(fx_path) = fx_override {
            Some(fs::read(fx_path).map_err(|e| format!("{}: {}", fx_path, e))?)
        } else {
            auto_find_fx(path)
        };
        if debug {
            if let Some(ref fx) = fx_data {
                eprintln!("FX data: {} bytes", fx.len());
            }
        }
        Ok(LoadedGame {
            hex_str,
            fx_data,
            fx_save: None,
            title: String::new(),
            hex_path: path.to_string(),
            elf_data: None,
        })
    }
}

fn auto_find_fx(hex_path: &str) -> Option<Vec<u8>> {
    let bin = hex_path.replace(".hex", ".bin").replace(".HEX", ".bin");
    if bin != hex_path && std::path::Path::new(&bin).exists() {
        return fs::read(&bin).ok();
    }
    let dir = std::path::Path::new(hex_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let stem = std::path::Path::new(hex_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let fx = dir.join(format!("{}-fx.bin", stem));
    if fx.exists() {
        fs::read(&fx).ok()
    } else {
        None
    }
}

/// Load FX data+save into the emulator at the correct flash layout offsets.
fn load_game_fx(arduboy: &mut Arduboy, game: &LoadedGame, debug: bool) {
    if let Some(ref fx) = game.fx_data {
        let save = game.fx_save.as_deref();
        let (dp, sp) = arduboy.load_fx_layout(fx, save);
        eprintln!(
            "FX layout: data={} bytes at page 0x{:04X} (byte 0x{:06X}), save at page 0x{:04X}",
            fx.len(),
            dp,
            dp as u32 * 256,
            sp
        );
        if debug {
            // Verify: print first 16 bytes at data offset
            let data_off = dp as usize * 256;
            let end = (data_off + 16).min(arduboy.fx_flash.data.len());
            if data_off < arduboy.fx_flash.data.len() {
                let flash_bytes: Vec<String> = arduboy.fx_flash.data[data_off..end]
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect();
                eprintln!(
                    "FX verify: flash[0x{:06X}..] = {}",
                    data_off,
                    flash_bytes.join(" ")
                );
                let orig: Vec<String> = fx[..16.min(fx.len())]
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect();
                eprintln!("FX verify: data.bin[0..16]   = {}", orig.join(" "));
            }
        }
    }
}

// ─── File Browser ──────────────────────────────────────────────────────────

/// Scan a directory for loadable game files (.hex, .arduboy).
fn scan_game_dir(dir: &str) -> Vec<String> {
    let dir_path = std::path::Path::new(dir);
    let mut games: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                let lower = name.to_lowercase();
                if lower.ends_with(".hex") || lower.ends_with(".arduboy") || lower.ends_with(".elf")
                {
                    games.push(entry.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    games.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    games
}

/// Find the index of a file path in a sorted game list.
fn find_game_index(games: &[String], current: &str) -> Option<usize> {
    let current_canon = std::path::Path::new(current)
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| current.to_string());
    games.iter().position(|g| {
        let g_canon = std::path::Path::new(g)
            .canonicalize()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| g.clone());
        g_canon == current_canon || g == current
    })
}

/// Load a game into the emulator, returning the new hex_path and title.
fn switch_game(
    arduboy: &mut Arduboy,
    path: &str,
    eep_path_old: &str,
    no_save: bool,
    debug: bool,
) -> Result<(String, String, String), String> {
    // Save current EEPROM before switching
    if !no_save && arduboy.eeprom_dirty {
        save_eeprom(arduboy, eep_path_old, debug);
    }
    let game = load_game_file(path, None, debug)?;

    // Auto-detect CPU type for the new game
    let mut tmp = vec![0u8; 32768];
    let detected = if arduboy_core::hex::parse_hex(&game.hex_str, &mut tmp).is_ok() {
        detect_cpu_type(&tmp)
    } else {
        arduboy.cpu_type
    };

    // If CPU type changed, reinitialize Arduboy entirely
    if detected != arduboy.cpu_type {
        let was_debug = arduboy.debug;
        *arduboy = Arduboy::new_with_cpu(detected);
        arduboy.debug = was_debug;
        if detected == CpuType::Atmega328p {
            eprintln!("CPU: ATmega328P (Gamebuino Classic mode)");
        } else {
            eprintln!("CPU: ATmega32u4 (Arduboy mode)");
        }
    } else {
        arduboy.reset();
    }

    arduboy
        .load_hex(&game.hex_str)
        .map_err(|e| format!("HEX parse: {}", e))?;
    load_game_fx(arduboy, &game, debug);
    let new_eep = eeprom_path(&game.hex_path);
    if !no_save {
        load_eeprom(arduboy, &new_eep, debug);
    }
    let title = if game.title.is_empty() {
        std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        game.title
    };
    Ok((game.hex_path, title, new_eep))
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    // Force X11 backend on Linux — minifb can segfault on Wayland (server-side
    // decoration failures). Only override if WAYLAND_DISPLAY is set and the user
    // hasn't explicitly chosen a backend.
    #[cfg(target_os = "linux")]
    {
        if env::var("WAYLAND_DISPLAY").is_ok() && env::var("MINIFB_BACKEND").is_err() {
            // Safety: called at start of main() before any threads are spawned.
            unsafe {
                env::remove_var("WAYLAND_DISPLAY");
            }
            eprintln!("Note: forcing X11 backend (minifb Wayland crash workaround)");
        }
    }

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Arduboy Emulator v0.8.1 - Rust");
        eprintln!("Usage: {} <file.hex|.arduboy|.elf> [options]", args[0]);
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  .hex             Intel HEX binary");
        eprintln!("  .arduboy         ZIP archive (info.json + hex + fx bin)");
        eprintln!("  .elf             ELF binary with debug symbols (avr-gcc output)");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --headless           Run without GUI");
        eprintln!("  --frames N           Run N frames (headless/step, default 60)");
        eprintln!("  --debug              Show per-frame diagnostics");
        eprintln!("  --press N            Press A on frame N (headless)");
        eprintln!("  --snapshot F         Print display at frame F (repeatable)");
        eprintln!("  --mute               Disable audio");
        eprintln!("  --fx <file.bin>      Load FX flash data");
        eprintln!("  --break <addr>       Breakpoint at hex byte-address (repeatable)");
        eprintln!("  --watch <addr>       Data watchpoint at hex address (repeatable)");
        eprintln!("  --step               Interactive step debugger");
        eprintln!("  --gdb <port>         Start GDB remote debug server on TCP port");
        eprintln!("  --profile            Enable profiler (report on exit)");
        eprintln!("  --scale N            Initial scale 1-6 (default 6)");
        eprintln!("  --serial             Show USB serial output on stderr");
        eprintln!("  --no-save            Disable EEPROM auto-save");
        eprintln!("  --cpu <type>         CPU type: 32u4 or 328p (auto-detected if omitted)");
        eprintln!("  --lcd                Start with LCD effect enabled");
        eprintln!("  --no-blur            Start with blur disabled");
        eprintln!();
        eprintln!("GUI keys: Arrows=D-pad Z=A X=B  1-6=Scale F11=Fullscreen");
        eprintln!("          S=Screenshot(PNG) G=GIF record D=RegDump T=Profiler");
        eprintln!("          M=Mute F=FPS unlimited B=Blur L=LCD effect A=Audio filter");
        eprintln!("          V=Portrait rotation  R=Reload N=Next P=Previous O=List games");
        eprintln!("          Backspace=Rewind  Esc=Quit");
        std::process::exit(1);
    }

    let game_path = &args[1];
    let headless = args.iter().any(|a| a == "--headless");
    let mute = args.iter().any(|a| a == "--mute");
    let debug = args.iter().any(|a| a == "--debug");
    let step_mode = args.iter().any(|a| a == "--step");
    let serial_enabled = args.iter().any(|a| a == "--serial");
    let no_save = args.iter().any(|a| a == "--no-save");
    let profile_enabled = args.iter().any(|a| a == "--profile");
    let lcd_start = args.iter().any(|a| a == "--lcd");
    let no_blur = args.iter().any(|a| a == "--no-blur");

    let gdb_port: Option<u16> = args
        .iter()
        .position(|a| a == "--gdb")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let initial_scale: usize = args
        .iter()
        .position(|a| a == "--scale")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(6)
        .max(1)
        .min(6);

    let fx_override: Option<&str> = args
        .iter()
        .position(|a| a == "--fx")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let cpu_override: Option<CpuType> = args
        .iter()
        .position(|a| a == "--cpu")
        .and_then(|i| args.get(i + 1))
        .map(|s| match s.as_str() {
            "328p" | "328P" | "atmega328p" => CpuType::Atmega328p,
            _ => CpuType::Atmega32u4,
        });

    // Load game (hex or .arduboy)
    let game = load_game_file(game_path, fx_override, debug).expect("Failed to load game file");

    // Determine CPU type: explicit --cpu flag, or auto-detect from flash contents
    let cpu_type = if let Some(ct) = cpu_override {
        ct
    } else {
        let mut tmp = vec![0u8; 32768];
        if arduboy_core::hex::parse_hex(&game.hex_str, &mut tmp).is_ok() {
            let detected = detect_cpu_type(&tmp);
            if debug {
                eprintln!("CPU auto-detected: {:?}", detected);
            }
            detected
        } else {
            CpuType::Atmega32u4
        }
    };

    let mut arduboy = Arduboy::new_with_cpu(cpu_type);
    arduboy.debug = debug;
    if cpu_type == CpuType::Atmega328p {
        eprintln!("CPU: ATmega328P (Gamebuino Classic mode)");
    }

    // Load game — ELF or HEX
    let mut _elf_info: Option<arduboy_core::elf::ElfFile> = None;
    if let Some(ref elf_data) = game.elf_data {
        match arduboy.load_elf(elf_data) {
            Ok(elf) => {
                eprintln!(
                    "ELF loaded: {} symbols, {} line entries",
                    elf.symbols.len(),
                    elf.line_map.len()
                );
                _elf_info = Some(elf);
            }
            Err(e) => {
                eprintln!("ELF parse error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        let size = arduboy
            .load_hex(&game.hex_str)
            .expect("Failed to parse HEX");
        if debug {
            eprintln!("Loaded {} bytes into flash", size);
        }
    }

    load_game_fx(&mut arduboy, &game, debug);

    // Parse breakpoints
    {
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--break" {
                if let Some(s) = args.get(i + 1) {
                    let s = s.trim_start_matches("0x").trim_start_matches("0X");
                    if let Ok(addr) = u16::from_str_radix(s, 16) {
                        let word_addr = addr / 2;
                        arduboy.breakpoints.push(word_addr);
                        if debug {
                            eprintln!("Breakpoint: 0x{:04X} (word 0x{:04X})", addr, word_addr);
                        }
                    }
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    // Parse watchpoints
    {
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--watch" {
                if let Some(s) = args.get(i + 1) {
                    let s = s.trim_start_matches("0x").trim_start_matches("0X");
                    if let Ok(addr) = u16::from_str_radix(s, 16) {
                        let idx = arduboy
                            .debugger
                            .add_watchpoint(addr, arduboy_core::debugger::WatchKind::ReadWrite);
                        if debug {
                            eprintln!("Watchpoint [{}]: 0x{:04X} RW", idx, addr);
                        }
                    }
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    // Auto-start profiler if --profile
    if profile_enabled {
        arduboy.profiler.start(arduboy.cpu.tick);
        if debug {
            eprintln!("Profiler: started");
        }
    }

    // EEPROM: auto-load
    let eep_path = eeprom_path(&game.hex_path);
    if !no_save {
        load_eeprom(&mut arduboy, &eep_path, debug);
    }

    if let Some(port) = gdb_port {
        run_gdb_mode(&mut arduboy, port, debug);
    } else if step_mode {
        run_step_mode(&args, &mut arduboy);
    } else if headless {
        run_headless(&args, &mut arduboy, serial_enabled);
    } else {
        run_gui(
            &mut arduboy,
            mute,
            debug,
            initial_scale,
            serial_enabled,
            &game.hex_path,
            &game.title,
            no_save,
            lcd_start,
            no_blur,
        );
    }

    // Profiler report on exit
    if profile_enabled || arduboy.profiler.enabled {
        if arduboy.profiler.enabled {
            arduboy.profiler.stop(arduboy.cpu.tick);
        }
        eprintln!("{}", arduboy.profiler_report());
    }

    // EEPROM: auto-save on exit
    if !no_save && arduboy.eeprom_dirty {
        save_eeprom(&arduboy, &eep_path, debug);
    }
}

// ─── GUI Mode ───────────────────────────────────────────────────────────────

fn run_gui(
    arduboy: &mut Arduboy,
    start_muted: bool,
    debug: bool,
    initial_scale: usize,
    serial_enabled: bool,
    hex_path: &str,
    game_title: &str,
    no_save: bool,
    lcd_start: bool,
    no_blur: bool,
) {
    let mut cur_hex_path = hex_path.to_string();
    let mut scale = initial_scale;
    let mut scaled_w = SCREEN_WIDTH * scale;
    let mut scaled_h = SCREEN_HEIGHT * scale;
    let make_title = |game_t: &str| -> String {
        if game_t.is_empty() {
            "Arduboy v0.8.1".to_string()
        } else {
            format!("Arduboy v0.8.1 - {}", game_t)
        }
    };
    let mut title_base = make_title(game_title);

    let mut window = Window::new(
        &title_base,
        scaled_w,
        scaled_h,
        WindowOptions {
            scale: Scale::X1,
            scale_mode: ScaleMode::UpperLeft,
            resize: true,
            ..Default::default()
        },
    )
    .expect("Failed to create window");
    window.set_target_fps(60);

    let audio_ring: Arc<std::sync::Mutex<std::collections::VecDeque<f32>>> = Arc::new(
        std::sync::Mutex::new(std::collections::VecDeque::with_capacity(16384)),
    );
    let freq_l = Arc::new(AtomicU32::new(0.0f32.to_bits()));
    let freq_r = Arc::new(AtomicU32::new(0.0f32.to_bits()));
    let mut muted = start_muted;
    let mut _audio = if !muted {
        setup_audio(audio_ring.clone(), freq_l.clone(), freq_r.clone())
    } else {
        None
    };
    let mut pcm_buf: Vec<f32> = Vec::with_capacity(16384);

    let mut gilrs = init_gamepad(debug);
    let mut gp = GamepadState::new();
    let mut frame_count: u64 = 0;
    let start_time = Instant::now();
    let mut last_fps_time = Instant::now();
    let mut fps_frames: u64 = 0;
    let mut scaled_buf = vec![0u32; scaled_w * scaled_h];
    let mut prev_m = false;
    let mut prev_s = false;
    let mut prev_d = false;
    let mut prev_f = false;
    let mut prev_g = false;
    let mut prev_r = false;
    let mut prev_f11 = false;
    let mut fullscreen = false;
    let mut fps_unlimited = false;
    let mut screenshot_n = 0u32;
    let mut prev_num = [false; 6];

    // GIF recording state
    let mut gif_encoder: Option<arduboy_core::gif::GifEncoder> = None;
    let mut gif_file_n = 0u32;

    // EEPROM auto-save timer
    let mut eep_path = eeprom_path(&cur_hex_path);
    let mut last_eeprom_save = Instant::now();

    // File browser state
    let game_dir = std::path::Path::new(&cur_hex_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_string_lossy()
        .into_owned();
    let mut game_list = scan_game_dir(&game_dir);
    let mut game_index = find_game_index(&game_list, &cur_hex_path).unwrap_or(0);
    let mut prev_n = false;
    let mut prev_p = false;
    let mut prev_o = false;
    let mut prev_b = false;
    let mut blur_enabled = !no_blur;
    let mut blur_buf = vec![0u32; scaled_w * scaled_h];
    let mut prev_l = false;
    let mut lcd_effect = lcd_start;
    let mut prev_t = false;
    let mut prev_a = false;
    let mut prev_v = false;
    let mut portrait = false;
    let mut rot_buf: Vec<u32> = Vec::new();
    // Temporal blend buffer for PCD8544 ghosting (128×64 float RGB)
    let mut prev_frame: Vec<(f32, f32, f32)> = vec![(0.0, 0.0, 0.0); SCREEN_WIDTH * SCREEN_HEIGHT];

    // Rewind buffer: snapshot every 30 frames (~0.5s), up to 600 slots (~5 min)
    let mut rewind = arduboy_core::snapshot::RewindBuffer::new(600, 30);
    let mut prev_backspace = false;

    // Save state path
    let mut state_path = arduboy_core::savestate::state_path(&cur_hex_path);
    // Notification message (shown in title bar temporarily)
    let mut notify_msg: Option<String> = None;
    let mut notify_until = Instant::now();
    let mut prev_f5 = false;
    let mut prev_f9 = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if let Some(ref mut g) = gilrs {
            poll_gamepad(g, &mut gp, debug);
        }

        // Scale toggle (1-6)
        let num = [
            window.is_key_down(Key::Key1),
            window.is_key_down(Key::Key2),
            window.is_key_down(Key::Key3),
            window.is_key_down(Key::Key4),
            window.is_key_down(Key::Key5),
            window.is_key_down(Key::Key6),
        ];
        for i in 0..6 {
            if num[i] && !prev_num[i] && !fullscreen {
                scale = i + 1;
                scaled_w = SCREEN_WIDTH * scale;
                scaled_h = SCREEN_HEIGHT * scale;
                scaled_buf.resize(scaled_w * scaled_h, 0);
                let (ww, wh) = if portrait {
                    (scaled_h, scaled_w)
                } else {
                    (scaled_w, scaled_h)
                };
                window = Window::new(
                    &title_base,
                    ww,
                    wh,
                    WindowOptions {
                        scale: Scale::X1,
                        scale_mode: ScaleMode::UpperLeft,
                        resize: true,
                        ..Default::default()
                    },
                )
                .expect("window");
                if fps_unlimited {
                    window.set_target_fps(0);
                } else {
                    window.set_target_fps(60);
                }
            }
        }
        prev_num = num;

        // Fullscreen (F11)
        let f11 = window.is_key_down(Key::F11);
        if f11 && !prev_f11 {
            fullscreen = !fullscreen;
            if fullscreen {
                scaled_w = SCREEN_WIDTH * 12;
                scaled_h = SCREEN_HEIGHT * 12;
            } else {
                scaled_w = SCREEN_WIDTH * scale;
                scaled_h = SCREEN_HEIGHT * scale;
            }
            scaled_buf.resize(scaled_w * scaled_h, 0);
            let (ww, wh) = if portrait {
                (scaled_h, scaled_w)
            } else {
                (scaled_w, scaled_h)
            };
            let mut opts = WindowOptions {
                scale: Scale::X1,
                scale_mode: ScaleMode::UpperLeft,
                resize: true,
                ..Default::default()
            };
            if fullscreen {
                opts.borderless = true;
            }
            window = Window::new(&title_base, ww, wh, opts).expect("window");
            if fps_unlimited {
                window.set_target_fps(0);
            } else {
                window.set_target_fps(60);
            }
        }
        prev_f11 = f11;

        // FPS unlimited toggle (F)
        let fk = window.is_key_down(Key::F);
        if fk && !prev_f {
            fps_unlimited = !fps_unlimited;
            if fps_unlimited {
                window.set_target_fps(0);
                eprintln!("FPS: unlimited");
            } else {
                window.set_target_fps(60);
                eprintln!("FPS: 60");
            }
        }
        prev_f = fk;

        // Blur toggle (B)
        let bk = window.is_key_down(Key::B);
        if bk && !prev_b {
            blur_enabled = !blur_enabled;
            eprintln!("Blur: {}", if blur_enabled { "ON" } else { "OFF" });
        }
        prev_b = bk;

        // LCD effect toggle (L)
        let lk = window.is_key_down(Key::L);
        if lk && !prev_l {
            lcd_effect = !lcd_effect;
            eprintln!("LCD effect: {}", if lcd_effect { "ON" } else { "OFF" });
        }
        prev_l = lk;

        // Profiler toggle (T)
        let tk = window.is_key_down(Key::T);
        if tk && !prev_t {
            if arduboy.profiler.enabled {
                arduboy.profiler.stop(arduboy.cpu.tick);
                eprintln!("{}", arduboy.profiler_report());
            } else {
                arduboy.profiler.start(arduboy.cpu.tick);
                eprintln!("Profiler: started (press T again to stop and report)");
            }
        }
        prev_t = tk;

        // Mute (M)
        let m = window.is_key_down(Key::M);
        if m && !prev_m {
            muted = !muted;
            if muted {
                freq_l.store(0.0f32.to_bits(), Ordering::Relaxed);
                freq_r.store(0.0f32.to_bits(), Ordering::Relaxed);
                _audio = None;
            } else {
                _audio = setup_audio(audio_ring.clone(), freq_l.clone(), freq_r.clone());
            }
        }
        prev_m = m;

        // Audio filter toggle (A)
        let ak = window.is_key_down(Key::A);
        if ak && !prev_a {
            arduboy.audio_buf.toggle_filters();
            eprintln!(
                "Audio filter: {}",
                if arduboy.audio_buf.filters_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
        prev_a = ak;

        // Portrait rotation toggle (V)
        let vk = window.is_key_down(Key::V);
        if vk && !prev_v {
            portrait = !portrait;
            eprintln!("Portrait: {}", if portrait { "ON" } else { "OFF" });
            let (ww, wh) = if portrait {
                (scaled_h, scaled_w)
            } else {
                (scaled_w, scaled_h)
            };
            let opts = WindowOptions {
                scale: Scale::X1,
                scale_mode: ScaleMode::UpperLeft,
                resize: true,
                ..Default::default()
            };
            window = Window::new(&title_base, ww, wh, opts).expect("window");
            if fps_unlimited {
                window.set_target_fps(0);
            } else {
                window.set_target_fps(60);
            }
        }
        prev_v = vk;

        // Screenshot (S) — PNG at current scale
        let s = window.is_key_down(Key::S);
        if s && !prev_s {
            let cur_s = scaled_w / SCREEN_WIDTH;
            let f = format!("screenshot_{:04}_{}x.png", screenshot_n, cur_s);
            match save_screenshot_png(arduboy, &f, cur_s) {
                Ok(()) => {
                    eprintln!("Screenshot: {} ({}x)", f, cur_s);
                    screenshot_n += 1;
                }
                Err(e) => eprintln!("Screenshot error: {}", e),
            }
        }
        prev_s = s;

        // GIF recording toggle (G)
        let gk = window.is_key_down(Key::G);
        if gk && !prev_g {
            if let Some(encoder) = gif_encoder.take() {
                // Stop recording
                let frames = encoder.frame_count();
                let gif_data = encoder.finish();
                let fname = format!("recording_{:04}.gif", gif_file_n);
                match fs::write(&fname, &gif_data) {
                    Ok(()) => eprintln!(
                        "GIF saved: {} ({} frames, {} bytes)",
                        fname,
                        frames,
                        gif_data.len()
                    ),
                    Err(e) => eprintln!("GIF save error: {}", e),
                }
                gif_file_n += 1;
            } else {
                // Start recording
                gif_encoder = Some(arduboy_core::gif::GifEncoder::new(
                    SCREEN_WIDTH as u16,
                    SCREEN_HEIGHT as u16,
                    2,
                ));
                eprintln!("GIF recording started (press G to stop)");
            }
        }
        prev_g = gk;

        // Reload (R)
        let rk = window.is_key_down(Key::R);
        if rk && !prev_r {
            // Save EEPROM before reload
            if !no_save && arduboy.eeprom_dirty {
                save_eeprom(arduboy, &eep_path, debug);
            }
            // Reload the game file
            match load_game_file(&cur_hex_path, None, debug) {
                Ok(game) => {
                    arduboy.reset();
                    if let Err(e) = arduboy.load_hex(&game.hex_str) {
                        eprintln!("Reload error: {}", e);
                    } else {
                        load_game_fx(arduboy, &game, debug);
                        if !no_save {
                            load_eeprom(arduboy, &eep_path, debug);
                        }
                        frame_count = 0;
                        eprintln!("Reloaded: {}", cur_hex_path);
                    }
                }
                Err(e) => eprintln!("Reload error: {}", e),
            }
        }
        prev_r = rk;

        // File browser: O = list games, N = next, P = previous
        let ok = window.is_key_down(Key::O);
        if ok && !prev_o {
            // Rescan directory and print game list
            game_list = scan_game_dir(&game_dir);
            game_index = find_game_index(&game_list, &cur_hex_path).unwrap_or(0);
            eprintln!("--- Games in {} ({} found) ---", game_dir, game_list.len());
            for (i, g) in game_list.iter().enumerate() {
                let marker = if i == game_index { " <<" } else { "" };
                let name = std::path::Path::new(g)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(g);
                eprintln!("  {:3}. {}{}", i + 1, name, marker);
            }
            eprintln!("---");
        }
        prev_o = ok;

        let nk = window.is_key_down(Key::N);
        if nk && !prev_n && !game_list.is_empty() {
            let next_idx = (game_index + 1) % game_list.len();
            let path = game_list[next_idx].clone();
            match switch_game(arduboy, &path, &eep_path, no_save, debug) {
                Ok((hp, title, ep)) => {
                    cur_hex_path = hp;
                    eep_path = ep;
                    state_path = arduboy_core::savestate::state_path(&cur_hex_path);
                    title_base = make_title(&title);
                    game_index = next_idx;
                    frame_count = 0;
                    window.set_title(&title_base);
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&path);
                    eprintln!("Loaded [{}/{}]: {}", game_index + 1, game_list.len(), name);
                }
                Err(e) => eprintln!("Load error: {}", e),
            }
        }
        prev_n = nk;

        let pk = window.is_key_down(Key::P);
        if pk && !prev_p && !game_list.is_empty() {
            let prev_idx = if game_index == 0 {
                game_list.len() - 1
            } else {
                game_index - 1
            };
            let path = game_list[prev_idx].clone();
            match switch_game(arduboy, &path, &eep_path, no_save, debug) {
                Ok((hp, title, ep)) => {
                    cur_hex_path = hp;
                    eep_path = ep;
                    state_path = arduboy_core::savestate::state_path(&cur_hex_path);
                    title_base = make_title(&title);
                    game_index = prev_idx;
                    frame_count = 0;
                    window.set_title(&title_base);
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&path);
                    eprintln!("Loaded [{}/{}]: {}", game_index + 1, game_list.len(), name);
                }
                Err(e) => eprintln!("Load error: {}", e),
            }
        }
        prev_p = pk;

        // Reg dump (D)
        let d = window.is_key_down(Key::D);
        if d && !prev_d {
            eprintln!(
                "--- Regs (frame {}) ---\n{}\nNext: {}\n---",
                frame_count,
                arduboy.dump_regs(),
                arduboy.disasm_at_pc()
            );
        }
        prev_d = d;

        // Quick Save (F5)
        let f5 = window.is_key_down(Key::F5);
        if f5 && !prev_f5 {
            let state = arduboy.save_full_state();
            let cpu_byte = arduboy.cpu_type_byte();
            match arduboy_core::savestate::save_to_file(
                &state,
                cpu_byte,
                std::path::Path::new(&state_path),
            ) {
                Ok(()) => {
                    let size = std::fs::metadata(&state_path).map(|m| m.len()).unwrap_or(0);
                    eprintln!("State saved: {} ({} bytes)", state_path, size);
                    notify_msg = Some("State saved".to_string());
                    notify_until = Instant::now() + Duration::from_secs(2);
                }
                Err(e) => {
                    eprintln!("Save state error: {}", e);
                    notify_msg = Some(format!("Save error: {}", e));
                    notify_until = Instant::now() + Duration::from_secs(3);
                }
            }
        }
        prev_f5 = f5;

        // Quick Load (F9)
        let f9 = window.is_key_down(Key::F9);
        if f9 && !prev_f9 {
            let cpu_byte = arduboy.cpu_type_byte();
            match arduboy_core::savestate::load_from_file(
                std::path::Path::new(&state_path),
                cpu_byte,
            ) {
                Ok(state) => {
                    arduboy.load_full_state(&state);
                    rewind.clear();
                    eprintln!("State loaded: {}", state_path);
                    notify_msg = Some("State loaded".to_string());
                    notify_until = Instant::now() + Duration::from_secs(2);
                }
                Err(e) => {
                    eprintln!("Load state error: {}", e);
                    notify_msg = Some(format!("Load error: {}", e));
                    notify_until = Instant::now() + Duration::from_secs(3);
                }
            }
        }
        prev_f9 = f9;

        // Input
        arduboy.set_button(Button::Up, window.is_key_down(Key::Up) || gp.eff_up());
        arduboy.set_button(Button::Down, window.is_key_down(Key::Down) || gp.eff_down());
        arduboy.set_button(Button::Left, window.is_key_down(Key::Left) || gp.eff_left());
        arduboy.set_button(
            Button::Right,
            window.is_key_down(Key::Right) || gp.eff_right(),
        );
        arduboy.set_button(Button::A, window.is_key_down(Key::Z) || gp.a);
        arduboy.set_button(Button::B, window.is_key_down(Key::X) || gp.b);

        // Rewind (Backspace) — restore previous snapshot instead of running
        let bksp = window.is_key_down(Key::Backspace);
        if bksp {
            if let Some(snap) = rewind.pop() {
                arduboy.restore_snapshot(&snap);
                if !prev_backspace {
                    eprintln!("Rewind: {} snapshots remaining", rewind.len());
                }
                prev_backspace = true;
                // Skip normal frame execution when rewinding
                // Still render below
            } else {
                if !prev_backspace {
                    eprintln!("Rewind: no more snapshots");
                }
                prev_backspace = true;
            }
        } else {
            prev_backspace = false;

            arduboy.run_frame();
            frame_count += 1;
            fps_frames += 1;

            // Diagnostic output for first few frames when debugging
            if debug && (frame_count == 1 || frame_count == 60 || frame_count == 120) {
                let fb = arduboy.framebuffer_rgba();
                let fb_nonzero = fb.chunks(4).any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);
                let display_cmds = arduboy.display.dbg_cmd_count;
                let display_data = arduboy.display.dbg_data_count;
                eprintln!("[Frame {}] display_type={:?}, SPI_writes={}, FX_transfers={}, display_cmds={}, display_data={}, fb_has_content={}, PC=0x{:04X}",
                    frame_count, arduboy.display_type,
                    arduboy.dbg_spdr_writes, arduboy.dbg_fx_transfers,
                    display_cmds, display_data, fb_nonzero,
                    arduboy.cpu.pc);
                if frame_count == 1 {
                    eprintln!(
                        "  DDRD=0x{:02X} PORTD=0x{:02X} FX_loaded={}",
                        arduboy.mem.data[0x2A], arduboy.mem.data[0x2B], arduboy.fx_flash.loaded
                    );
                }
            }
            // Always print FX diagnostics at frame 1 (helps debug FX games)
            if frame_count == 1 && arduboy.fx_flash.loaded {
                let fb = arduboy.framebuffer_rgba();
                let fb_nonzero = fb.chunks(4).any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);
                eprintln!("[FX diag] frame=1 DDRD=0x{:02X} PORTD=0x{:02X} SPI={} FX={} display={:?} cmds={} data={} fb={}",
                    arduboy.mem.data[0x2A], arduboy.mem.data[0x2B],
                    arduboy.dbg_spdr_writes, arduboy.dbg_fx_transfers,
                    arduboy.display_type,
                    arduboy.display.dbg_cmd_count, arduboy.display.dbg_data_count,
                    if fb_nonzero { "content" } else { "EMPTY" });
            }

            // Save rewind snapshot at interval
            if rewind.tick_frame() {
                rewind.push(arduboy.save_snapshot());
            }
        }

        if !bksp && arduboy.breakpoint_hit {
            eprintln!(
                "*** Breakpoint: {} ***\n{}",
                arduboy.disasm_at_pc(),
                arduboy.dump_regs()
            );
            arduboy.breakpoint_hit = false;
        }

        if serial_enabled {
            let out = arduboy.take_serial_output();
            if !out.is_empty() {
                let _ = std::io::stderr().write_all(&out);
                let _ = std::io::stderr().flush();
            }
        }

        // GIF recording: capture frame
        if let Some(ref mut enc) = gif_encoder {
            let fb = arduboy.framebuffer_rgba();
            let mono: Vec<bool> = (0..SCREEN_WIDTH * SCREEN_HEIGHT)
                .map(|i| fb[i * 4] > 128)
                .collect();
            enc.add_frame_mono(&mono);
        }

        if !muted {
            let (lh, rh) = arduboy.get_audio_tone();
            if arduboy.audio_buf.needs_render() {
                arduboy.audio_buf.render_samples(
                    &mut pcm_buf,
                    AUDIO_SAMPLE_RATE,
                    arduboy_core::CLOCK_HZ,
                    AUDIO_VOLUME,
                );
                if let Ok(mut ring) = audio_ring.lock() {
                    let max_buf = AUDIO_SAMPLE_RATE as usize / 5;
                    if ring.len() < max_buf {
                        ring.extend(pcm_buf.iter());
                    }
                }
                freq_l.store(0.0f32.to_bits(), Ordering::Relaxed);
                freq_r.store(0.0f32.to_bits(), Ordering::Relaxed);
            } else {
                freq_l.store(lh.to_bits(), Ordering::Relaxed);
                freq_r.store(rh.to_bits(), Ordering::Relaxed);
            }
        }

        // EEPROM auto-save (every 10 seconds if dirty)
        if !no_save && arduboy.eeprom_dirty && last_eeprom_save.elapsed() >= Duration::from_secs(10)
        {
            save_eeprom(arduboy, &eep_path, debug);
            arduboy.eeprom_dirty = false;
            last_eeprom_save = Instant::now();
        }

        // Adapt buffer to window resize (maintain aspect ratio)
        if !fullscreen {
            let (win_w, win_h) = window.get_size();
            let (base_w, base_h) = if portrait {
                (SCREEN_HEIGHT, SCREEN_WIDTH)
            } else {
                (SCREEN_WIDTH, SCREEN_HEIGHT)
            };
            let fit_scale_w = win_w / base_w;
            let fit_scale_h = win_h / base_h;
            let fit_scale = fit_scale_w.min(fit_scale_h).max(1).min(12);
            let new_w = SCREEN_WIDTH * fit_scale;
            let new_h = SCREEN_HEIGHT * fit_scale;
            if new_w != scaled_w || new_h != scaled_h {
                scale = fit_scale.min(6).max(1);
                scaled_w = new_w;
                scaled_h = new_h;
                scaled_buf.resize(scaled_w * scaled_h, 0);
            }
        }

        // ── Render pipeline ──────────────────────────────────────────────
        let raw_pixels = arduboy.framebuffer_u32();
        let cur_scale = scaled_w / SCREEN_WIDTH;
        let is_pcd = matches!(arduboy.display_type, DisplayType::Pcd8544);

        // (1) Color palette + (3) Temporal blend → lcd_pixels 128×64
        if lcd_effect {
            // SSD1306 OLED palette: ON → blue-white, OFF → near-black
            // PCD8544 LCD palette:  ON → dark gray-green, OFF → yellow-green
            let (col_on, col_off): ((f32, f32, f32), (f32, f32, f32)) = if is_pcd {
                (
                    (0x3C as f32, 0x48 as f32, 0x28 as f32),
                    (0xC0 as f32, 0xD8 as f32, 0x78 as f32),
                )
            } else {
                (
                    (0xA0 as f32, 0xD0 as f32, 0xFF as f32),
                    (0x05 as f32, 0x05 as f32, 0x08 as f32),
                )
            };
            // Temporal blend factor: PCD8544 20% previous, SSD1306 5%
            let ghost = if is_pcd { 0.20f32 } else { 0.05f32 };
            let fresh = 1.0 - ghost;

            for i in 0..(SCREEN_WIDTH * SCREEN_HEIGHT) {
                let raw = raw_pixels[i];
                // Determine if pixel is "on" (any channel > 0x40)
                let on = (raw & 0xFFFFFF) > 0x404040;
                let (tr, tg, tb) = if on { col_on } else { col_off };
                // Blend with previous frame
                let (pr, pg, pb) = prev_frame[i];
                let nr = tr * fresh + pr * ghost;
                let ng = tg * fresh + pg * ghost;
                let nb = tb * fresh + pb * ghost;
                prev_frame[i] = (nr, ng, nb);
            }

            // Scale up from prev_frame
            for y in 0..SCREEN_HEIGHT {
                for x in 0..SCREEN_WIDTH {
                    let (fr, fg, fb) = prev_frame[y * SCREEN_WIDTH + x];
                    let c = ((fr as u32) << 16) | ((fg as u32) << 8) | (fb as u32);
                    for sy in 0..cur_scale {
                        let base = (y * cur_scale + sy) * scaled_w + x * cur_scale;
                        for sx in 0..cur_scale {
                            if base + sx < scaled_buf.len() {
                                scaled_buf[base + sx] = c;
                            }
                        }
                    }
                }
            }

            // (2) Pixel grid lines + (4) Corner rounding (need scale ≥ 3)
            if cur_scale >= 3 {
                // Grid line darkness: darken the last row and column of each pixel cell
                let grid_dim = if is_pcd { 0.55f32 } else { 0.70f32 };
                // Corner darkness
                let corner_dim = if is_pcd { 0.40f32 } else { 0.50f32 };

                for py in 0..SCREEN_HEIGHT {
                    for px in 0..SCREEN_WIDTH {
                        let bx = px * cur_scale;
                        let by = py * cur_scale;

                        for sy in 0..cur_scale {
                            for sx in 0..cur_scale {
                                let gx = bx + sx;
                                let gy = by + sy;
                                let idx = gy * scaled_w + gx;
                                if idx >= scaled_buf.len() {
                                    continue;
                                }

                                // Is this sub-pixel on a grid edge?
                                let on_right = sx == cur_scale - 1;
                                let on_bottom = sy == cur_scale - 1;
                                // Is this sub-pixel a corner of the pixel block?
                                let is_inner_corner = (sx == 0 && sy == 0)
                                    || (sx == 0 && sy == cur_scale - 1)
                                    || (sx == cur_scale - 1 && sy == 0)
                                    || (sx == cur_scale - 1 && sy == cur_scale - 1);

                                let dim = if is_inner_corner {
                                    corner_dim
                                } else if on_right || on_bottom {
                                    grid_dim
                                } else {
                                    1.0
                                };

                                if dim < 1.0 {
                                    let c = scaled_buf[idx];
                                    let r = (((c >> 16) & 0xFF) as f32 * dim) as u32;
                                    let g = (((c >> 8) & 0xFF) as f32 * dim) as u32;
                                    let b = ((c & 0xFF) as f32 * dim) as u32;
                                    scaled_buf[idx] = (r << 16) | (g << 8) | b;
                                }
                            }
                        }
                    }
                }
            } else if cur_scale == 2 {
                // At 2× only do subtle grid on right/bottom edge
                let grid_dim = if is_pcd { 0.70f32 } else { 0.80f32 };
                for py in 0..SCREEN_HEIGHT {
                    for px in 0..SCREEN_WIDTH {
                        let bx = px * 2;
                        let by = py * 2;
                        // Right column
                        for sy in 0..2 {
                            let idx = (by + sy) * scaled_w + bx + 1;
                            if idx < scaled_buf.len() {
                                let c = scaled_buf[idx];
                                let r = (((c >> 16) & 0xFF) as f32 * grid_dim) as u32;
                                let g = (((c >> 8) & 0xFF) as f32 * grid_dim) as u32;
                                let b = ((c & 0xFF) as f32 * grid_dim) as u32;
                                scaled_buf[idx] = (r << 16) | (g << 8) | b;
                            }
                        }
                        // Bottom row
                        for sx in 0..2 {
                            let idx = (by + 1) * scaled_w + bx + sx;
                            if idx < scaled_buf.len() {
                                let c = scaled_buf[idx];
                                let r = (((c >> 16) & 0xFF) as f32 * grid_dim) as u32;
                                let g = (((c >> 8) & 0xFF) as f32 * grid_dim) as u32;
                                let b = ((c & 0xFF) as f32 * grid_dim) as u32;
                                scaled_buf[idx] = (r << 16) | (g << 8) | b;
                            }
                        }
                    }
                }
            }
        } else {
            // Normal rendering (no LCD effect)
            for y in 0..SCREEN_HEIGHT {
                for x in 0..SCREEN_WIDTH {
                    let c = raw_pixels[y * SCREEN_WIDTH + x];
                    for sy in 0..cur_scale {
                        let base = (y * cur_scale + sy) * scaled_w + x * cur_scale;
                        for sx in 0..cur_scale {
                            if base + sx < scaled_buf.len() {
                                scaled_buf[base + sx] = c;
                            }
                        }
                    }
                }
            }
        }

        // Soft blur pass (B key toggle) — applied after LCD effects
        let use_blur = blur_enabled && cur_scale >= 2;
        if use_blur {
            if blur_buf.len() != scaled_buf.len() {
                blur_buf.resize(scaled_buf.len(), 0);
            }
            let w = scaled_w;
            let h = scaled_h;
            for y in 0..h {
                for x in 0..w {
                    let idx = y * w + x;
                    let c = scaled_buf[idx];
                    let cr = (c >> 16) & 0xFF;
                    let cg = (c >> 8) & 0xFF;
                    let cb = c & 0xFF;
                    let (mut sr, mut sg, mut sb) = (cr * 4, cg * 4, cb * 4);
                    for &(dx, dy) in &[(0isize, -1isize), (0, 1), (-1, 0), (1, 0)] {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx >= 0 && nx < w as isize && ny >= 0 && ny < h as isize {
                            let n = scaled_buf[ny as usize * w + nx as usize];
                            sr += ((n >> 16) & 0xFF) * 2;
                            sg += ((n >> 8) & 0xFF) * 2;
                            sb += (n & 0xFF) * 2;
                        } else {
                            sr += cr * 2;
                            sg += cg * 2;
                            sb += cb * 2;
                        }
                    }
                    for &(dx, dy) in &[(-1isize, -1isize), (1, -1), (-1, 1), (1, 1)] {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx >= 0 && nx < w as isize && ny >= 0 && ny < h as isize {
                            let n = scaled_buf[ny as usize * w + nx as usize];
                            sr += (n >> 16) & 0xFF;
                            sg += (n >> 8) & 0xFF;
                            sb += n & 0xFF;
                        } else {
                            sr += cr;
                            sg += cg;
                            sb += cb;
                        }
                    }
                    blur_buf[idx] = ((sr / 16) << 16) | ((sg / 16) << 8) | (sb / 16);
                }
            }
        }

        // Display output (with optional portrait rotation)
        let final_src = if use_blur { &blur_buf } else { &scaled_buf };
        if portrait {
            // Rotate 90° CCW: left side → bottom (portrait orientation)
            let rw = scaled_h; // rotated width  = landscape height
            let rh = scaled_w; // rotated height = landscape width
            rot_buf.resize(rw * rh, 0);
            for y in 0..scaled_h {
                for x in 0..scaled_w {
                    let nx = y;
                    let ny = scaled_w - 1 - x;
                    rot_buf[ny * rw + nx] = final_src[y * scaled_w + x];
                }
            }
            window.update_with_buffer(&rot_buf, rw, rh).expect("update");
        } else {
            window
                .update_with_buffer(final_src, scaled_w, scaled_h)
                .expect("update");
        }

        if last_fps_time.elapsed() >= Duration::from_secs(2) {
            let fps = fps_frames as f64 / last_fps_time.elapsed().as_secs_f64();
            let (lh, rh) = arduboy.get_audio_tone();
            let mut ti = String::new();
            if lh > 0.0 {
                ti.push_str(&format!(" L:{:.0}Hz", lh));
            }
            if rh > 0.0 {
                ti.push_str(&format!(" R:{:.0}Hz", rh));
            }
            let ms = if muted { " [MUTE]" } else { "" };
            let fs = if fps_unlimited { " [∞]" } else { "" };
            let rec = if gif_encoder.is_some() { " [REC]" } else { "" };
            // LED status
            let (lr, lg, lb) = arduboy.get_led_state();
            let led = if lr > 0 || lg > 0 || lb > 0 {
                format!(" LED({},{},{})", lr, lg, lb)
            } else {
                String::new()
            };
            let tx = if arduboy.led_tx { " TX" } else { "" };
            let rx = if arduboy.led_rx { " RX" } else { "" };
            let lcd = if lcd_effect { " [LCD]" } else { "" };
            let blr = if blur_enabled { " [BLUR]" } else { "" };
            let prf = if arduboy.profiler.enabled {
                " [PROF]"
            } else {
                ""
            };
            let flt = if arduboy.audio_buf.filters_enabled {
                " [FILT]"
            } else {
                ""
            };
            let prt = if portrait { " [PORT]" } else { "" };
            let ntf = if notify_msg.is_some() && Instant::now() < notify_until {
                format!(" [{}]", notify_msg.as_ref().unwrap())
            } else {
                if notify_msg.is_some() && Instant::now() >= notify_until {
                    notify_msg = None;
                }
                String::new()
            };
            window.set_title(&format!(
                "{} - {:.0} FPS{}{}{}{}{}{}{}{}{}{}{}{}{} ({}x)",
                title_base,
                fps,
                ti,
                ms,
                fs,
                rec,
                led,
                tx,
                rx,
                lcd,
                blr,
                prf,
                flt,
                prt,
                ntf,
                cur_scale,
            ));
            fps_frames = 0;
            last_fps_time = Instant::now();
        }
    }

    // Final GIF save if still recording
    if let Some(encoder) = gif_encoder.take() {
        let frames = encoder.frame_count();
        let gif_data = encoder.finish();
        let fname = format!("recording_{:04}.gif", gif_file_n);
        if let Ok(()) = fs::write(&fname, &gif_data) {
            eprintln!(
                "GIF saved on exit: {} ({} frames, {} bytes)",
                fname,
                frames,
                gif_data.len()
            );
        }
    }

    // Final EEPROM save
    if !no_save && arduboy.eeprom_dirty {
        save_eeprom(arduboy, &eep_path, debug);
    }

    if debug {
        let e = start_time.elapsed().as_secs_f64();
        eprintln!(
            "{} frames in {:.1}s ({:.1} FPS), {} cycles",
            frame_count,
            e,
            frame_count as f64 / e,
            arduboy.cpu.tick
        );
    }
}

// ─── Step Mode ──────────────────────────────────────────────────────────────

fn run_step_mode(args: &[String], arduboy: &mut Arduboy) {
    let max_steps: usize = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    println!("Interactive Debugger v0.8.1");
    println!("Commands:");
    println!("  <Enter>/<N>  Step 1 or N instructions");
    println!("  r/run        Run to breakpoint/watchpoint");
    println!("  f/frame      Run one frame (216000 cycles)");
    println!("  d/dump       Register dump");
    println!("  ram <addr> [len]  Hex dump (default len=128)");
    println!("  io           Show non-zero I/O registers");
    println!("  io all       Show all I/O registers");
    println!("  b <addr>     Add breakpoint (byte address)");
    println!("  bl           List breakpoints");
    println!("  bd <idx>     Delete breakpoint");
    println!("  w <addr> [r|w|rw]  Add watchpoint (data addr)");
    println!("  wl           List watchpoints");
    println!("  wd <idx>     Delete watchpoint");
    println!("  prof start   Start profiler");
    println!("  prof stop    Stop and show report");
    println!("  prof report  Show profiler report");
    println!("  q/quit       Exit");
    println!();
    println!("{}", arduboy.dump_regs());
    println!("Next: {}", arduboy.disasm_at_pc());

    let stdin = std::io::stdin();
    let mut steps = 0usize;
    let mut ram_snapshot: Option<Vec<u8>> = None;

    loop {
        let mut line = String::new();
        print!("dbg> ");
        let _ = std::io::stdout().flush();
        if stdin.read_line(&mut line).is_err() {
            break;
        }
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            // Empty line = step 1
            let asm = arduboy.step_one();
            steps += 1;
            println!("  {}", asm);
            check_watch_hit(arduboy);
            println!("{}", arduboy.dump_regs());
            println!("Next: {}", arduboy.disasm_at_pc());
            continue;
        }

        match parts[0] {
            "q" | "quit" => break,

            "d" | "dump" => {
                println!("{}", arduboy.dump_regs());
            }

            "r" | "run" => {
                let limit = if parts.len() > 1 {
                    parts[1].parse().unwrap_or(max_steps)
                } else {
                    max_steps
                };
                for _ in 0..limit {
                    if !arduboy.breakpoints.is_empty()
                        && arduboy.breakpoints.contains(&arduboy.cpu.pc)
                    {
                        println!("*** Breakpoint: {} ***", arduboy.disasm_at_pc());
                        break;
                    }
                    arduboy.step_one();
                    steps += 1;
                    if check_watch_hit(arduboy) {
                        break;
                    }
                }
                println!("{}", arduboy.dump_regs());
                println!("Next: {}", arduboy.disasm_at_pc());
            }

            "f" | "frame" => {
                let n: usize = if parts.len() > 1 {
                    parts[1].parse().unwrap_or(1)
                } else {
                    1
                };
                for _ in 0..n {
                    arduboy.run_frame();
                    if arduboy.breakpoint_hit {
                        println!("*** Break: {} ***", arduboy.disasm_at_pc());
                        arduboy.breakpoint_hit = false;
                        check_watch_hit(arduboy);
                        break;
                    }
                }
                println!("{}", arduboy.dump_regs());
                println!("Next: {}", arduboy.disasm_at_pc());
            }

            "ram" => {
                let addr: u16 = if parts.len() > 1 {
                    parse_cli_hex(parts[1]).unwrap_or(0x100) as u16
                } else {
                    0x100
                };
                let len: u16 = if parts.len() > 2 {
                    parse_cli_hex(parts[2]).unwrap_or(128) as u16
                } else {
                    128
                };
                println!("{}", arduboy.dump_ram(addr, len));
            }

            "ramdiff" => {
                let addr: u16 = if parts.len() > 1 {
                    parse_cli_hex(parts[1]).unwrap_or(0x100) as u16
                } else {
                    0x100
                };
                let len: u16 = if parts.len() > 2 {
                    parse_cli_hex(parts[2]).unwrap_or(128) as u16
                } else {
                    128
                };
                if let Some(ref old) = ram_snapshot {
                    println!(
                        "{}",
                        arduboy_core::debugger::dump_ram_diff(old, &arduboy.mem.data, addr, len)
                    );
                } else {
                    println!("No snapshot. Use 'snap' to take a RAM snapshot first.");
                }
            }

            "snap" => {
                ram_snapshot = Some(arduboy.mem.data.clone());
                println!("RAM snapshot taken ({} bytes)", arduboy.mem.data.len());
            }

            "io" => {
                if parts.len() > 1 && parts[1] == "all" {
                    println!("{}", arduboy.dump_io_all());
                } else {
                    println!("{}", arduboy.dump_io());
                }
            }

            "b" => {
                if parts.len() > 1 {
                    if let Some(addr) = parse_cli_hex(parts[1]) {
                        let word_addr = (addr as u16) / 2;
                        arduboy.breakpoints.push(word_addr);
                        println!(
                            "Breakpoint added: 0x{:04X} (word 0x{:04X})",
                            addr, word_addr
                        );
                    }
                } else {
                    println!("Usage: b <hex-byte-addr>");
                }
            }

            "bl" => {
                if arduboy.breakpoints.is_empty() {
                    println!("No breakpoints.");
                } else {
                    for (i, &bp) in arduboy.breakpoints.iter().enumerate() {
                        println!("  [{}] 0x{:04X} (byte 0x{:04X})", i, bp, bp * 2);
                    }
                }
            }

            "bd" => {
                if parts.len() > 1 {
                    if let Ok(idx) = parts[1].parse::<usize>() {
                        if idx < arduboy.breakpoints.len() {
                            let removed = arduboy.breakpoints.remove(idx);
                            println!("Removed breakpoint [{}] at 0x{:04X}", idx, removed * 2);
                        } else {
                            println!("Invalid index.");
                        }
                    }
                }
            }

            "w" => {
                if parts.len() > 1 {
                    if let Some(addr) = parse_cli_hex(parts[1]) {
                        let kind = if parts.len() > 2 {
                            match parts[2] {
                                "r" => arduboy_core::debugger::WatchKind::Read,
                                "w" => arduboy_core::debugger::WatchKind::Write,
                                _ => arduboy_core::debugger::WatchKind::ReadWrite,
                            }
                        } else {
                            arduboy_core::debugger::WatchKind::ReadWrite
                        };
                        let idx = arduboy.debugger.add_watchpoint(addr as u16, kind);
                        println!("Watchpoint [{}]: 0x{:04X} {:?}", idx, addr, kind);
                    }
                } else {
                    println!("Usage: w <hex-addr> [r|w|rw]");
                }
            }

            "wl" => {
                print!("{}", arduboy.debugger.list_watchpoints());
            }

            "wd" => {
                if parts.len() > 1 {
                    if let Ok(idx) = parts[1].parse::<usize>() {
                        if arduboy.debugger.remove_watchpoint(idx) {
                            println!("Watchpoint [{}] removed.", idx);
                        } else {
                            println!("Invalid index.");
                        }
                    }
                }
            }

            "prof" => {
                if parts.len() < 2 {
                    println!("Usage: prof start|stop|report");
                    continue;
                }
                match parts[1] {
                    "start" => {
                        arduboy.profiler.start(arduboy.cpu.tick);
                        println!("Profiler started.");
                    }
                    "stop" => {
                        arduboy.profiler.stop(arduboy.cpu.tick);
                        println!("{}", arduboy.profiler_report());
                    }
                    "report" => {
                        println!("{}", arduboy.profiler_report());
                    }
                    _ => println!("Usage: prof start|stop|report"),
                }
            }

            // Numeric: step N instructions
            _ => {
                let n: usize = parts[0].parse().unwrap_or(1);
                for i in 0..n {
                    let asm = arduboy.step_one();
                    steps += 1;
                    if n <= 20 {
                        println!("  {}", asm);
                    } else if i == n - 1 {
                        println!("  ... {} steps, last: {}", n, asm);
                    }
                    if check_watch_hit(arduboy) {
                        break;
                    }
                }
                println!("{}", arduboy.dump_regs());
                println!("Next: {}", arduboy.disasm_at_pc());
            }
        }
    }
    // Show profiler report if it was running
    if arduboy.profiler.enabled {
        arduboy.profiler.stop(arduboy.cpu.tick);
        println!("{}", arduboy.profiler_report());
    }
    println!("Total: {} steps, {} cycles", steps, arduboy.cpu.tick);
}

/// Check and display watchpoint hit, return true if hit.
fn check_watch_hit(arduboy: &mut Arduboy) -> bool {
    if let Some(hit) = arduboy.debugger.take_hit() {
        let name =
            arduboy_core::debugger::io_name(hit.addr, arduboy.cpu_type == CpuType::Atmega328p)
                .unwrap_or("");
        println!(
            "*** Watchpoint [{}]: {:?} at 0x{:04X}{} {:02X} → {:02X} ***",
            hit.index,
            hit.access,
            hit.addr,
            if name.is_empty() {
                String::new()
            } else {
                format!(" ({})", name)
            },
            hit.old_val,
            hit.new_val
        );
        true
    } else {
        false
    }
}

/// Parse hex string with optional 0x prefix.
fn parse_cli_hex(s: &str) -> Option<u32> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(s, 16).ok()
}

// ─── GDB Server Mode ────────────────────────────────────────────────────────

fn run_gdb_mode(arduboy: &mut Arduboy, port: u16, debug: bool) {
    use arduboy_core::gdb_server::{GdbAction, GdbServer};

    let server = GdbServer::bind(port).expect("Failed to bind GDB server");
    eprintln!("Waiting for GDB connection on port {}...", port);
    let mut session = server.accept().expect("Failed to accept GDB connection");

    // Initial halt — GDB expects the target to be stopped
    loop {
        let regs = arduboy.gdb_regs();
        let action = session
            .process_packet(
                &regs,
                arduboy.cpu.sreg,
                arduboy.cpu.sp,
                arduboy.cpu.pc,
                &arduboy.mem.flash,
                &mut arduboy.mem.data,
            )
            .expect("GDB packet error");

        match action {
            GdbAction::Continue => {
                // Run until breakpoint or GDB interrupt
                session.set_nonblocking(true).ok();
                loop {
                    // Check GDB breakpoints
                    let pc_word = arduboy.cpu.pc;
                    if session.breakpoints.iter().any(|&bp| pc_word == bp as u16) {
                        break;
                    }

                    // Check emulator breakpoints
                    if !arduboy.breakpoints.is_empty() && arduboy.breakpoints.contains(&pc_word) {
                        break;
                    }

                    // Check watchpoint hits
                    if arduboy.debugger.watch_hit.is_some() {
                        break;
                    }

                    // Execute one instruction
                    arduboy.step_one();

                    // Check for Ctrl+C from GDB client
                    if session.has_pending() {
                        session.set_nonblocking(false).ok();
                        // Read the interrupt byte
                        let regs2 = arduboy.gdb_regs();
                        let _ = session.process_packet(
                            &regs2,
                            arduboy.cpu.sreg,
                            arduboy.cpu.sp,
                            arduboy.cpu.pc,
                            &arduboy.mem.flash,
                            &mut arduboy.mem.data,
                        );
                        break;
                    }
                }
                session.set_nonblocking(false).ok();

                if let Some(wh) = arduboy.debugger.take_hit() {
                    if debug {
                        eprintln!(
                            "GDB: watchpoint hit at 0x{:04X} ({:02X} → {:02X})",
                            wh.addr, wh.old_val, wh.new_val
                        );
                    }
                }
                session.send_stop_reply().expect("GDB send error");
            }

            GdbAction::Step => {
                arduboy.step_one();
                session.send_stop_reply().expect("GDB send error");
            }

            GdbAction::Disconnect => {
                eprintln!("GDB client disconnected.");
                break;
            }

            GdbAction::None => {
                // Response already sent, loop back to read next packet
            }
        }

        if session.done {
            break;
        }
    }
}

// ─── Headless Mode ──────────────────────────────────────────────────────────

fn run_headless(args: &[String], arduboy: &mut Arduboy, serial_enabled: bool) {
    let frames: usize = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let debug = args.iter().any(|a| a == "--debug");
    let press_frame: Option<usize> = args
        .iter()
        .position(|a| a == "--press")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());
    let mut snapshots: Vec<usize> = Vec::new();
    {
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--snapshot" {
                if let Some(f) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    snapshots.push(f);
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    if debug {
        if let Some(pf) = press_frame {
            println!("Press A on frame {}", pf);
        }
        println!("Running {} frames...", frames);
    }
    for frame in 0..frames {
        if let Some(pf) = press_frame {
            if frame == pf {
                arduboy.set_button(Button::A, true);
                if debug {
                    println!("  >> A pressed");
                }
            } else if frame == pf + 5 {
                arduboy.set_button(Button::A, false);
                if debug {
                    println!("  >> A released");
                }
            }
        }
        arduboy.display.dbg_reset_counters();
        arduboy.pcd8544.dbg_reset_counters();
        arduboy.timer0.dbg_reset_counters();
        let t0 = arduboy.cpu.tick;
        let px0 = pixel_count(arduboy);
        arduboy.run_frame();
        let t1 = arduboy.cpu.tick;
        if arduboy.breakpoint_hit {
            println!(
                "*** Break: {} (frame {}) ***\n{}",
                arduboy.disasm_at_pc(),
                frame + 1,
                arduboy.dump_regs()
            );
            arduboy.breakpoint_hit = false;
            // Check for watchpoint hit
            if let Some(wh) = arduboy.debugger.take_hit() {
                let name = arduboy_core::debugger::io_name(
                    wh.addr,
                    arduboy.cpu_type == CpuType::Atmega328p,
                )
                .unwrap_or("");
                println!(
                    "  Watchpoint [{}]: {:?} at 0x{:04X}{} {:02X} → {:02X}",
                    wh.index,
                    wh.access,
                    wh.addr,
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", name)
                    },
                    wh.old_val,
                    wh.new_val
                );
            }
        }
        if serial_enabled {
            let out = arduboy.take_serial_output();
            if !out.is_empty() {
                let _ = std::io::stderr().write_all(&out);
                let _ = std::io::stderr().flush();
            }
        }
        if debug {
            let lit = pixel_count(arduboy);
            let pxc = lit != px0;
            let sd = arduboy.display.dbg_data_count + arduboy.pcd8544.dbg_data_count;
            let (lh, rh) = arduboy.get_audio_tone();
            let mut ts = String::new();
            if lh > 0.0 {
                ts.push_str(&format!("  L:{:.0}Hz", lh));
            }
            if rh > 0.0 {
                ts.push_str(&format!("  R:{:.0}Hz", rh));
            }
            let show = frame < 15
                || (frame < 100 && frame % 10 == 0)
                || (frame < 1000 && frame % 100 == 0)
                || frame == frames - 1
                || pxc
                || sd > 0
                || lh > 0.0
                || rh > 0.0
                || press_frame.map_or(false, |pf| frame >= pf && frame < pf + 20);
            if show {
                println!("  Frame {:3}: +{:6} cyc  px={:4}  t0ovf={:3}  t0int={:3}  spi={:4}  [{}] disp={:?}{}{}",
                    frame+1, t1-t0, lit, arduboy.timer0.dbg_ovf_count, arduboy.timer0.dbg_int_fire_count,
                    sd, arduboy.timer0.dbg_info(), arduboy.display_type,
                    if pxc { "  ***PX" } else { "" }, ts);
            }
        }
        if snapshots.contains(&(frame + 1)) || (debug && frame == frames - 1) {
            println!("\n  === Frame {} ===", frame + 1);
            print_display(arduboy);
        }
    }
    if debug {
        println!("\nDone. {} cycles.", arduboy.cpu.tick);
    }
}

fn pixel_count(arduboy: &Arduboy) -> usize {
    let fb = arduboy.framebuffer_rgba();
    (0..SCREEN_WIDTH * SCREEN_HEIGHT)
        .filter(|&i| fb[i * 4] > 0)
        .count()
}

fn print_display(arduboy: &Arduboy) {
    let fb = arduboy.framebuffer_rgba();
    let lit = (0..SCREEN_WIDTH * SCREEN_HEIGHT)
        .filter(|&i| fb[i * 4] > 0)
        .count();
    println!("  ({} px lit)", lit);
    for y in (0..SCREEN_HEIGHT).step_by(2) {
        let mut l = String::with_capacity(SCREEN_WIDTH + 4);
        l.push_str("  |");
        for x in 0..SCREEN_WIDTH {
            let t = fb[(y * SCREEN_WIDTH + x) * 4] > 128;
            let b = if y + 1 < SCREEN_HEIGHT {
                fb[((y + 1) * SCREEN_WIDTH + x) * 4] > 128
            } else {
                false
            };
            l.push(match (t, b) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                _ => ' ',
            });
        }
        l.push('|');
        println!("{}", l);
    }
}
