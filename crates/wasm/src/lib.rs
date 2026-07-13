//! # arduboy-wasm
//!
//! WebAssembly bindings for [`arduboy_core`], exposing a small `AbEmu` class to
//! JavaScript via `wasm-bindgen`. The browser client drives it much like the
//! native frontends: load a ROM, then each animation frame call
//! [`AbEmu::run_frame`], blit [`AbEmu::frame`] to a `<canvas>`, and feed
//! [`AbEmu::render_audio`] to the Web Audio API.
//!
//! The core is platform-independent; this crate simply avoids the OS-specific
//! paths (the GDB TCP server and file-based save/load) and moves bytes across
//! the JS boundary explicitly.

use arduboy_core::{detect_cpu_type, Arduboy, Button, CpuType, SCREEN_HEIGHT, SCREEN_WIDTH};
use wasm_bindgen::prelude::*;

/// CPU clock the audio renderer uses to convert emulated ticks to samples.
const CLOCK_HZ: u32 = arduboy_core::CLOCK_HZ;

/// Browser-facing emulator handle.
#[wasm_bindgen]
pub struct AbEmu {
    ard: Arduboy,
    /// Scratch buffer reused by [`AbEmu::render_audio`].
    audio_scratch: Vec<f32>,
}

#[wasm_bindgen]
impl AbEmu {
    /// Create a fresh emulator (ATmega32u4, no ROM). Installs a panic hook so
    /// Rust panics surface in the browser console instead of an opaque trap.
    #[wasm_bindgen(constructor)]
    pub fn new() -> AbEmu {
        console_error_panic_hook::set_once();
        AbEmu {
            ard: Arduboy::new(),
            audio_scratch: Vec::new(),
        }
    }

    /// Load a ROM by file name + bytes. `.arduboy` archives are unpacked (hex +
    /// FX); anything else is treated as Intel HEX text. The CPU type is
    /// auto-detected from the flash image. Throws on parse failure.
    #[wasm_bindgen(js_name = loadFile)]
    pub fn load_file(&mut self, name: &str, data: &[u8]) -> Result<(), JsValue> {
        let lower = name.to_lowercase();
        if lower.ends_with(".arduboy") {
            let ab = arduboy_core::arduboy_file::parse_arduboy(data)
                .map_err(|e| JsValue::from_str(&e))?;
            let hex = ab
                .hex
                .ok_or_else(|| JsValue::from_str("no HEX in .arduboy archive"))?;
            self.load_hex_str(&hex)?;
            if let Some(fx) = ab.fx_data {
                self.ard.load_fx_layout(&fx, ab.fx_save.as_deref());
            }
            Ok(())
        } else {
            let hex = std::str::from_utf8(data)
                .map_err(|_| JsValue::from_str("HEX file is not valid UTF-8 text"))?;
            self.load_hex_str(hex)
        }
    }

    /// Load an explicit FX flash image (overrides any archive FX).
    #[wasm_bindgen(js_name = loadFx)]
    pub fn load_fx(&mut self, data: &[u8]) {
        self.ard.load_fx_layout(data, None);
    }

    /// Reset the CPU and peripherals (flash/FX preserved).
    pub fn reset(&mut self) {
        self.ard.reset();
    }

    /// Run one video frame (~216000 cycles at 16 MHz).
    #[wasm_bindgen(js_name = runFrame)]
    pub fn run_frame(&mut self) {
        self.ard.run_frame();
    }

    /// Set a button state. `btn`: 0=Up 1=Down 2=Left 3=Right 4=A 5=B.
    #[wasm_bindgen(js_name = setButton)]
    pub fn set_button(&mut self, btn: u8, pressed: bool) {
        let button = match btn {
            0 => Button::Up,
            1 => Button::Down,
            2 => Button::Left,
            3 => Button::Right,
            4 => Button::A,
            5 => Button::B,
            _ => return,
        };
        self.ard.set_button(button, pressed);
    }

    /// Current display framebuffer as RGBA8 bytes (128×64×4). Returned as a copy
    /// (a `Uint8Array` in JS) suitable for `ImageData`.
    pub fn frame(&self) -> Vec<u8> {
        self.ard.framebuffer_rgba().to_vec()
    }

    /// Render this frame's audio as interleaved L,R `f32` samples (a
    /// `Float32Array` in JS). Call once per frame, after [`AbEmu::run_frame`].
    #[wasm_bindgen(js_name = renderAudio)]
    pub fn render_audio(&mut self, sample_rate: u32, volume: f32) -> Vec<f32> {
        let mut scratch = std::mem::take(&mut self.audio_scratch);
        self.ard
            .audio_buf
            .render_samples(&mut scratch, sample_rate, CLOCK_HZ, volume);
        let out = scratch.clone();
        self.audio_scratch = scratch;
        out
    }

    /// CPU type of the loaded ROM: 0 = ATmega32u4, 1 = ATmega328P.
    #[wasm_bindgen(js_name = cpuType)]
    pub fn cpu_type(&self) -> u8 {
        match self.ard.cpu_type {
            CpuType::Atmega32u4 => 0,
            CpuType::Atmega328p => 1,
        }
    }

    /// RGB LED state as `[r, g, b]` (0–255).
    #[wasm_bindgen(js_name = ledRgb)]
    pub fn led_rgb(&self) -> Vec<u8> {
        let (r, g, b) = self.ard.get_led_state();
        vec![r, g, b]
    }

    /// Snapshot EEPROM contents (for browser persistence, e.g. IndexedDB).
    #[wasm_bindgen(js_name = saveEeprom)]
    pub fn save_eeprom(&self) -> Vec<u8> {
        self.ard.save_eeprom()
    }

    /// Restore EEPROM contents previously saved with [`AbEmu::save_eeprom`].
    #[wasm_bindgen(js_name = loadEeprom)]
    pub fn load_eeprom(&mut self, data: &[u8]) {
        self.ard.load_eeprom(data);
    }

    /// Whether EEPROM has unsaved changes.
    #[wasm_bindgen(js_name = eepromDirty)]
    pub fn eeprom_dirty(&self) -> bool {
        self.ard.eeprom_dirty
    }

    /// Display width in pixels (128).
    #[wasm_bindgen(js_name = screenWidth)]
    pub fn screen_width() -> u32 {
        SCREEN_WIDTH as u32
    }

    /// Display height in pixels (64).
    #[wasm_bindgen(js_name = screenHeight)]
    pub fn screen_height() -> u32 {
        SCREEN_HEIGHT as u32
    }

    /// Detect the CPU from a HEX string, recreate the core, and load it.
    fn load_hex_str(&mut self, hex: &str) -> Result<(), JsValue> {
        let mut tmp = vec![0u8; arduboy_core::FLASH_SIZE];
        let cpu = if arduboy_core::hex::parse_hex(hex, &mut tmp).is_ok() {
            detect_cpu_type(&tmp)
        } else {
            CpuType::Atmega32u4
        };
        self.ard = Arduboy::new_with_cpu(cpu);
        self.ard
            .load_hex(hex)
            .map(|_| ())
            .map_err(|e| JsValue::from_str(&format!("HEX parse: {}", e)))
    }
}

impl Default for AbEmu {
    fn default() -> Self {
        Self::new()
    }
}
