//! # arduboy-ffi
//!
//! A thin, `extern "C"` FFI layer over [`arduboy_core`], intended for native
//! frontends written in other languages (a Qt6/C++ client, in particular).
//!
//! The whole surface is plain C ABI — an opaque [`Emu`] handle plus free
//! functions that borrow it. No Rust types cross the boundary; buffers are
//! passed as raw pointers with explicit lengths, and strings as UTF-8
//! `const char*`. See `include/arduboy_ffi.h` for the matching C declarations.
//!
//! ## Ownership & threading
//!
//! - [`abemu_new`] returns a heap-allocated `Emu*`; the caller must release it
//!   with [`abemu_free`]. Passing a null or dangling handle is undefined.
//! - The handle is **not** thread-safe. Call all functions for a given handle
//!   from a single thread (typically the GUI/emulation thread).
//! - Pointers returned by [`abemu_framebuffer`] are owned by the handle and stay
//!   valid until the next mutating call or [`abemu_free`]; copy out if needed.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uchar, c_uint};
use std::path::Path;

use arduboy_core::{
    detect_cpu_type, savestate, Arduboy, Button, CpuType, SCREEN_HEIGHT, SCREEN_WIDTH,
};

/// Audio sample rate the sample renderer configures its filters for by default.
/// The Qt client can request any rate at call time.
const CLOCK_HZ: u32 = arduboy_core::CLOCK_HZ;

/// Opaque emulator handle exposed to C. Wraps the [`Arduboy`] core plus the
/// small amount of frontend-side state the FFI owns (loaded paths, last error,
/// an in-progress GIF recording, and scratch buffers).
pub struct Emu {
    ard: Arduboy,
    /// Path of the currently loaded ROM (used to derive `.eep` / `.state` paths).
    rom_path: String,
    /// Human-readable title (from a `.arduboy` archive), empty otherwise.
    title: String,
    /// Last error message, surfaced via [`abemu_last_error`].
    last_error: CString,
    /// Reusable scratch strings returned to C (kept alive between calls).
    scratch_str: CString,
    /// Active GIF recording, if any: (encoder, output path).
    gif: Option<(arduboy_core::gif::GifEncoder, String)>,
    /// Reusable f32 buffer for audio rendering.
    audio_scratch: Vec<f32>,
}

impl Emu {
    fn set_error(&mut self, msg: impl Into<Vec<u8>>) {
        self.last_error = CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap());
    }
    fn clear_error(&mut self) {
        self.last_error = CString::new("").unwrap();
    }
}

/// Convert a C string pointer to a Rust `&str`. Returns `None` on null / invalid UTF-8.
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Borrow the handle mutably. Callers must pass a valid, non-null `Emu*`.
unsafe fn handle<'a>(h: *mut Emu) -> Option<&'a mut Emu> {
    if h.is_null() {
        None
    } else {
        Some(&mut *h)
    }
}

// ─── Lifecycle ──────────────────────────────────────────────────────────────

/// Create a new, empty emulator (ATmega32u4, no ROM loaded).
/// The caller owns the returned pointer and must free it with [`abemu_free`].
#[no_mangle]
pub extern "C" fn abemu_new() -> *mut Emu {
    let emu = Box::new(Emu {
        ard: Arduboy::new(),
        rom_path: String::new(),
        title: String::new(),
        last_error: CString::new("").unwrap(),
        scratch_str: CString::new("").unwrap(),
        gif: None,
        audio_scratch: Vec::new(),
    });
    Box::into_raw(emu)
}

/// Free an emulator handle previously returned by [`abemu_new`]. Null is ignored.
#[no_mangle]
pub extern "C" fn abemu_free(h: *mut Emu) {
    if !h.is_null() {
        unsafe {
            drop(Box::from_raw(h));
        }
    }
}

/// Return the most recent error message as a NUL-terminated UTF-8 string.
/// The pointer is valid until the next FFI call on this handle. Never null.
#[no_mangle]
pub extern "C" fn abemu_last_error(h: *mut Emu) -> *const c_char {
    match unsafe { handle(h) } {
        Some(e) => e.last_error.as_ptr(),
        None => b"null handle\0".as_ptr() as *const c_char,
    }
}

// ─── Loading ────────────────────────────────────────────────────────────────

/// Locate a companion FX `.bin` next to a ROM, mirroring the desktop frontend:
/// `game.hex` → `game.bin`, else `game-fx.bin`.
fn auto_find_fx(rom_path: &str) -> Option<Vec<u8>> {
    let bin = rom_path
        .replace(".hex", ".bin")
        .replace(".HEX", ".bin")
        .replace(".arduboy", ".bin")
        .replace(".elf", ".bin");
    if bin != rom_path && Path::new(&bin).exists() {
        if let Ok(d) = std::fs::read(&bin) {
            return Some(d);
        }
    }
    let dir = Path::new(rom_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let stem = Path::new(rom_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let fx = dir.join(format!("{}-fx.bin", stem));
    if fx.exists() {
        std::fs::read(&fx).ok()
    } else {
        None
    }
}

/// Derive the EEPROM sidecar path (`game.hex` → `game.eep`).
fn eeprom_path(rom_path: &str) -> String {
    let p = Path::new(rom_path);
    let dir = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    dir.join(format!("{}.eep", stem))
        .to_string_lossy()
        .into_owned()
}

/// Load a ROM by path, auto-detecting the format from its extension:
/// `.arduboy` (ZIP), `.elf`, or plain Intel `.hex`. Companion FX `.bin` and
/// `.eep` EEPROM files are loaded automatically when present. The CPU type is
/// auto-detected from the flash image.
///
/// Returns 0 on success, non-zero on failure (see [`abemu_last_error`]).
#[no_mangle]
pub extern "C" fn abemu_load_file(h: *mut Emu, path: *const c_char) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    let path = match unsafe { cstr(path) } {
        Some(p) => p.to_string(),
        None => {
            emu.set_error("invalid path");
            return -1;
        }
    };
    emu.clear_error();

    let lower = path.to_lowercase();
    let (hex_str, elf_data, mut fx_data, fx_save, title) = if lower.ends_with(".arduboy") {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                emu.set_error(format!("{}: {}", path, e));
                return -1;
            }
        };
        match arduboy_core::arduboy_file::parse_arduboy(&data) {
            Ok(ab) => {
                let hex = match ab.hex {
                    Some(h) => h,
                    None => {
                        emu.set_error("no HEX in .arduboy file");
                        return -1;
                    }
                };
                (hex, None, ab.fx_data, ab.fx_save, ab.title)
            }
            Err(e) => {
                emu.set_error(e);
                return -1;
            }
        }
    } else if lower.ends_with(".elf") {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                emu.set_error(format!("{}: {}", path, e));
                return -1;
            }
        };
        (
            String::new(),
            Some(data),
            auto_find_fx(&path),
            None,
            String::new(),
        )
    } else {
        let hex = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                emu.set_error(format!("{}: {}", path, e));
                return -1;
            }
        };
        (hex, None, auto_find_fx(&path), None, String::new())
    };

    // Determine CPU type by parsing the flash image for detection.
    let cpu_type = if let Some(ref elf) = elf_data {
        // Parse ELF flash for detection without disturbing the live emulator.
        match arduboy_core::elf::parse_elf(elf) {
            Ok(e) => detect_cpu_type(&e.flash),
            Err(_) => CpuType::Atmega32u4,
        }
    } else {
        let mut tmp = vec![0u8; arduboy_core::FLASH_SIZE];
        if arduboy_core::hex::parse_hex(&hex_str, &mut tmp).is_ok() {
            detect_cpu_type(&tmp)
        } else {
            CpuType::Atmega32u4
        }
    };

    // Recreate the core for the detected CPU type.
    emu.ard = Arduboy::new_with_cpu(cpu_type);

    if let Some(elf) = elf_data {
        if let Err(e) = emu.ard.load_elf(&elf) {
            emu.set_error(format!("ELF parse: {}", e));
            return -1;
        }
    } else if let Err(e) = emu.ard.load_hex(&hex_str) {
        emu.set_error(format!("HEX parse: {}", e));
        return -1;
    }

    // Load FX flash at the standard layout, if present.
    if let Some(fx) = fx_data.take() {
        emu.ard.load_fx_layout(&fx, fx_save.as_deref());
    }

    emu.rom_path = path.clone();
    emu.title = title;

    // Auto-load EEPROM sidecar if present.
    let eep = eeprom_path(&path);
    if let Ok(data) = std::fs::read(&eep) {
        emu.ard.load_eeprom(&data);
    }

    0
}

/// Explicitly load an FX flash `.bin` file (overrides any auto-detected FX).
/// Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_load_fx_file(h: *mut Emu, path: *const c_char) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    let path = match unsafe { cstr(path) } {
        Some(p) => p,
        None => {
            emu.set_error("invalid path");
            return -1;
        }
    };
    match std::fs::read(path) {
        Ok(data) => {
            emu.ard.load_fx_layout(&data, None);
            emu.clear_error();
            0
        }
        Err(e) => {
            emu.set_error(format!("{}: {}", path, e));
            -1
        }
    }
}

// ─── Info ───────────────────────────────────────────────────────────────────

/// Display width in pixels (128).
#[no_mangle]
pub extern "C" fn abemu_screen_width() -> c_int {
    SCREEN_WIDTH as c_int
}

/// Display height in pixels (64).
#[no_mangle]
pub extern "C" fn abemu_screen_height() -> c_int {
    SCREEN_HEIGHT as c_int
}

/// CPU type of the loaded ROM: 0 = ATmega32u4, 1 = ATmega328P.
#[no_mangle]
pub extern "C" fn abemu_cpu_type(h: *mut Emu) -> c_int {
    match unsafe { handle(h) } {
        Some(e) => match e.ard.cpu_type {
            CpuType::Atmega32u4 => 0,
            CpuType::Atmega328p => 1,
        },
        None => -1,
    }
}

/// Copy the ROM title (from a `.arduboy` archive) into `out` as a NUL-terminated
/// UTF-8 string. Returns the number of bytes written (excluding the terminator).
#[no_mangle]
pub extern "C" fn abemu_title(h: *mut Emu, out: *mut c_char, max: c_int) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return 0,
    };
    copy_str_out(&emu.title, out, max)
}

// ─── Control ────────────────────────────────────────────────────────────────

/// Reset the CPU and peripherals to power-on state (flash/FX preserved).
#[no_mangle]
pub extern "C" fn abemu_reset(h: *mut Emu) {
    if let Some(e) = unsafe { handle(h) } {
        e.ard.reset();
    }
}

/// Run one video frame of emulation (~216000 cycles at 16 MHz).
#[no_mangle]
pub extern "C" fn abemu_run_frame(h: *mut Emu) {
    if let Some(e) = unsafe { handle(h) } {
        e.ard.run_frame();
        // If a GIF recording is active, capture this frame.
        if e.gif.is_some() {
            let indices = framebuffer_to_indices(&e.ard);
            if let Some((enc, _)) = e.gif.as_mut() {
                enc.add_frame(&indices);
            }
        }
    }
}

/// Set a button state. `btn`: 0=Up 1=Down 2=Left 3=Right 4=A 5=B.
/// `pressed`: non-zero = pressed.
#[no_mangle]
pub extern "C" fn abemu_set_button(h: *mut Emu, btn: c_int, pressed: c_int) {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return,
    };
    let button = match btn {
        0 => Button::Up,
        1 => Button::Down,
        2 => Button::Left,
        3 => Button::Right,
        4 => Button::A,
        5 => Button::B,
        _ => return,
    };
    emu.ard.set_button(button, pressed != 0);
}

// ─── Display ────────────────────────────────────────────────────────────────

/// Return a pointer to the current display framebuffer as RGBA8 bytes,
/// `width * height * 4` in size (128×64×4 = 32768 bytes). The buffer is owned by
/// the handle and remains valid until the next mutating call.
#[no_mangle]
pub extern "C" fn abemu_framebuffer(h: *mut Emu) -> *const c_uchar {
    match unsafe { handle(h) } {
        Some(e) => e.ard.framebuffer_rgba().as_ptr(),
        None => std::ptr::null(),
    }
}

// ─── Audio ──────────────────────────────────────────────────────────────────

/// Render this frame's stereo audio into `out` (interleaved L,R f32 samples).
/// `max_pairs` is the capacity of `out` in *stereo pairs* (so `out` must hold
/// `max_pairs * 2` floats). Returns the number of stereo pairs written.
///
/// Call once per frame, after [`abemu_run_frame`].
#[no_mangle]
pub extern "C" fn abemu_render_audio(
    h: *mut Emu,
    out: *mut f32,
    max_pairs: c_int,
    sample_rate: c_uint,
    volume: f32,
) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return 0,
    };
    if out.is_null() || max_pairs <= 0 {
        return 0;
    }
    // render_samples fills a Vec of interleaved L,R and returns the pair count.
    let scratch = std::mem::take(&mut emu.audio_scratch);
    let mut scratch = scratch;
    let pairs = emu
        .ard
        .audio_buf
        .render_samples(&mut scratch, sample_rate, CLOCK_HZ, volume);
    let n = pairs.min(max_pairs as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(scratch.as_ptr(), out, n * 2);
    }
    emu.audio_scratch = scratch;
    n as c_int
}

// ─── LEDs & serial ──────────────────────────────────────────────────────────

/// Read the RGB LED state (0–255 each). Any pointer may be null to skip it.
#[no_mangle]
pub extern "C" fn abemu_led_rgb(h: *mut Emu, r: *mut c_uchar, g: *mut c_uchar, b: *mut c_uchar) {
    if let Some(e) = unsafe { handle(h) } {
        let (rr, gg, bb) = e.ard.get_led_state();
        unsafe {
            if !r.is_null() {
                *r = rr;
            }
            if !g.is_null() {
                *g = gg;
            }
            if !b.is_null() {
                *b = bb;
            }
        }
    }
}

/// TX LED state (1 = on).
#[no_mangle]
pub extern "C" fn abemu_led_tx(h: *mut Emu) -> c_int {
    unsafe { handle(h) }.map_or(0, |e| e.ard.led_tx as c_int)
}

/// RX LED state (1 = on).
#[no_mangle]
pub extern "C" fn abemu_led_rx(h: *mut Emu) -> c_int {
    unsafe { handle(h) }.map_or(0, |e| e.ard.led_rx as c_int)
}

/// Drain accumulated USB serial output into `out`. Returns the byte count.
#[no_mangle]
pub extern "C" fn abemu_take_serial(h: *mut Emu, out: *mut c_uchar, max: c_int) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return 0,
    };
    if out.is_null() || max <= 0 {
        return 0;
    }
    let bytes = emu.ard.take_serial_output();
    let n = bytes.len().min(max as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, n);
    }
    n as c_int
}

// ─── EEPROM ─────────────────────────────────────────────────────────────────

/// Whether EEPROM has unsaved changes (1 = dirty).
#[no_mangle]
pub extern "C" fn abemu_eeprom_dirty(h: *mut Emu) -> c_int {
    unsafe { handle(h) }.map_or(0, |e| e.ard.eeprom_dirty as c_int)
}

/// Save EEPROM to the sidecar `.eep` next to the loaded ROM. Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_save_eeprom(h: *mut Emu) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    if emu.rom_path.is_empty() {
        emu.set_error("no ROM loaded");
        return -1;
    }
    let path = eeprom_path(&emu.rom_path);
    let data = emu.ard.save_eeprom();
    match std::fs::write(&path, &data) {
        Ok(_) => {
            emu.ard.eeprom_dirty = false;
            emu.clear_error();
            0
        }
        Err(e) => {
            emu.set_error(format!("{}: {}", path, e));
            -1
        }
    }
}

// ─── Save state (quick save / load) ─────────────────────────────────────────

/// Quick-save full emulator state to `game.state` next to the ROM. Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_save_state(h: *mut Emu) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    if emu.rom_path.is_empty() {
        emu.set_error("no ROM loaded");
        return -1;
    }
    let path = savestate::state_path(&emu.rom_path);
    let state = emu.ard.save_full_state();
    let cpu_byte = emu.ard.cpu_type_byte();
    match savestate::save_to_file(&state, cpu_byte, Path::new(&path)) {
        Ok(_) => {
            emu.clear_error();
            0
        }
        Err(e) => {
            emu.set_error(e);
            -1
        }
    }
}

/// Quick-load full emulator state from `game.state`. Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_load_state(h: *mut Emu) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    if emu.rom_path.is_empty() {
        emu.set_error("no ROM loaded");
        return -1;
    }
    let path = savestate::state_path(&emu.rom_path);
    let cpu_byte = emu.ard.cpu_type_byte();
    match savestate::load_from_file(Path::new(&path), cpu_byte) {
        Ok(state) => {
            emu.ard.load_full_state(&state);
            emu.clear_error();
            0
        }
        Err(e) => {
            emu.set_error(e);
            -1
        }
    }
}

// ─── Screenshot & GIF ───────────────────────────────────────────────────────

/// Save a PNG screenshot of the current frame (128×64) to `path`. Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_screenshot_png(h: *mut Emu, path: *const c_char) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    let path = match unsafe { cstr(path) } {
        Some(p) => p.to_string(),
        None => {
            emu.set_error("invalid path");
            return -1;
        }
    };
    let png = arduboy_core::png::encode_png(
        SCREEN_WIDTH as u32,
        SCREEN_HEIGHT as u32,
        emu.ard.framebuffer_rgba(),
    );
    match std::fs::write(&path, &png) {
        Ok(_) => {
            emu.clear_error();
            0
        }
        Err(e) => {
            emu.set_error(format!("{}: {}", path, e));
            -1
        }
    }
}

/// Convert the current RGBA framebuffer to GIF palette indices (0 = black, 1 = white).
fn framebuffer_to_indices(ard: &Arduboy) -> Vec<u8> {
    let fb = ard.framebuffer_rgba();
    let mut out = Vec::with_capacity(SCREEN_WIDTH * SCREEN_HEIGHT);
    for i in 0..(SCREEN_WIDTH * SCREEN_HEIGHT) {
        // Treat any reasonably bright pixel as "on".
        out.push(if fb[i * 4] >= 128 { 1 } else { 0 });
    }
    out
}

/// Start recording an animated GIF to `path` (finalized on [`abemu_gif_stop`]).
/// Frames are captured automatically on each [`abemu_run_frame`]. Returns 0 on success.
#[no_mangle]
pub extern "C" fn abemu_gif_start(h: *mut Emu, path: *const c_char) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    let path = match unsafe { cstr(path) } {
        Some(p) => p.to_string(),
        None => {
            emu.set_error("invalid path");
            return -1;
        }
    };
    // 3 cs delay ≈ ~33fps playback (frames are captured every emulated frame).
    let enc = arduboy_core::gif::GifEncoder::new(SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16, 3);
    emu.gif = Some((enc, path));
    emu.clear_error();
    0
}

/// Whether a GIF recording is in progress (1 = yes).
#[no_mangle]
pub extern "C" fn abemu_gif_recording(h: *mut Emu) -> c_int {
    unsafe { handle(h) }.map_or(0, |e| e.gif.is_some() as c_int)
}

/// Finalize the active GIF recording and write it to disk. Returns 0 on success,
/// 1 if no recording was active, negative on write error.
#[no_mangle]
pub extern "C" fn abemu_gif_stop(h: *mut Emu) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return -1,
    };
    match emu.gif.take() {
        Some((enc, path)) => {
            let data = enc.finish();
            match std::fs::write(&path, &data) {
                Ok(_) => {
                    emu.clear_error();
                    0
                }
                Err(e) => {
                    emu.set_error(format!("{}: {}", path, e));
                    -1
                }
            }
        }
        None => 1,
    }
}

// ─── Debug views ────────────────────────────────────────────────────────────

/// Copy a register dump (R0–R31, PC, SP, SREG, X/Y/Z) into `out` as UTF-8.
/// Returns the number of bytes written (excluding the terminator).
#[no_mangle]
pub extern "C" fn abemu_dump_regs(h: *mut Emu, out: *mut c_char, max: c_int) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return 0,
    };
    let s = emu.ard.dump_regs();
    copy_str_out(&s, out, max)
}

/// Copy a hex+ASCII dump of `length` RAM bytes starting at `start` into `out`.
/// Returns the number of bytes written (excluding the terminator).
#[no_mangle]
pub extern "C" fn abemu_dump_ram(
    h: *mut Emu,
    start: c_uint,
    length: c_uint,
    out: *mut c_char,
    max: c_int,
) -> c_int {
    let emu = match unsafe { handle(h) } {
        Some(e) => e,
        None => return 0,
    };
    let s = emu.ard.dump_ram(start as u16, length as u16);
    copy_str_out(&s, out, max)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Copy a Rust string into a caller-provided C buffer, NUL-terminated and
/// truncated to `max` bytes. Returns bytes written (excluding the terminator).
fn copy_str_out(s: &str, out: *mut c_char, max: c_int) -> c_int {
    if out.is_null() || max <= 0 {
        return 0;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min((max - 1) as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, n);
        *out.add(n) = 0;
    }
    n as c_int
}

// Keep scratch_str referenced so the field isn't flagged unused; reserved for
// future zero-copy string returns.
#[allow(dead_code)]
fn _touch_scratch(e: &Emu) -> *const c_char {
    e.scratch_str.as_ptr()
}
