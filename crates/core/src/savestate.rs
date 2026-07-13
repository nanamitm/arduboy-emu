//! Save state (quick save / quick load) for the Arduboy emulator.
//!
//! Captures the full emulator state to a file using bincode serialization
//! with deflate compression. Users can save/load gameplay at any point
//! with a single key press (F5 save, F9 load).
//!
//! ## File format
//!
//! ```text
//! +------------------+
//! | Magic "ABES"     |  4 bytes
//! +------------------+
//! | Format version   |  u32 little-endian (currently 1)
//! +------------------+
//! | CPU type         |  u8 (0 = ATmega32u4, 1 = ATmega328P)
//! +------------------+
//! | Compressed data  |  deflate-compressed bincode payload
//! +------------------+
//! ```

use serde::{Serialize, Deserialize};
use std::path::Path;

/// Magic bytes identifying an arduboy-emu save state file.
const MAGIC: &[u8; 4] = b"ABES";
/// Current save state format version.
const FORMAT_VERSION: u32 = 1;

// ─── Per-component state structs ────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Timer8State {
    pub tick: u64,
    pub prescale: u32,
    pub cs: u8,
    pub mode: u8,
    pub wgm00: bool,
    pub wgm01: bool,
    pub wgm02: bool,
    pub com_a: u8,
    pub com_b: u8,
    pub ocr0a: u8,
    pub ocr0b: u8,
    pub tcnt_shadow: u8,
    pub tov0: u32,
    pub ocf0a: u32,
    pub ocf0b: u32,
    pub toie0: bool,
    pub ocie0a: bool,
    pub ocie0b: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Timer16State {
    pub tick: u64,
    pub prescale: u32,
    pub tcnt: u16,
    pub top: u16,
    pub ctc: bool,
    pub wgm: [bool; 4],
    pub cs: u8,
    pub com_a: u8,
    pub com_b: u8,
    pub com_c: u8,
    pub ocr_a: u16,
    pub ocr_b: u16,
    pub ocr_c: u16,
    pub foc_a: bool,
    pub foc_b: bool,
    pub foc_c: bool,
    pub tov: u32,
    pub ocf_a: u32,
    pub ocf_b: u32,
    pub ocf_c: u32,
    pub toie: bool,
    pub ocie_a: bool,
    pub ocie_b: bool,
    pub ocie_c: bool,
    pub old_wgm: u8,
}

#[derive(Serialize, Deserialize)]
pub struct Timer4State {
    pub tcnt: u16,
    pub tc4h: u8,
    pub ocr_a: u16,
    pub ocr_b: u16,
    pub ocr_c: u16,
    pub ocr_d: u16,
    pub tccr_a: u8,
    pub tccr_b: u8,
    pub tccr_c: u8,
    pub tccr_d: u8,
    pub tccr_e: u8,
    pub dt4: u8,
    pub timsk: u8,
    pub cs: u8,
    pub prescale: u32,
    pub tick: u64,
    pub wgm: u8,
    pub tov: u32,
    pub ocf_a: u32,
    pub ocf_b: u32,
    pub ocf_d: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SpiState {
    pub spif: bool,
    pub wcol: bool,
    pub spi2x: bool,
    pub spie: bool,
    pub spe: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AdcState {
    pub aden: bool,
    pub adsc: bool,
    pub adie: bool,
    pub adif: bool,
    pub adch: u8,
    pub adcl: u8,
}

#[derive(Serialize, Deserialize)]
pub struct PllState {
    pub pindiv: bool,
    pub plle: bool,
    pub plock: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FxFlashState {
    pub data: Vec<u8>,
    pub loaded: bool,
    pub write_enabled: bool,
    pub powered_down: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Ssd1306State {
    pub framebuffer: Vec<u8>,
    pub col: u8,
    pub page: u8,
    pub col_start: u8,
    pub col_end: u8,
    pub page_start: u8,
    pub page_end: u8,
    pub inverted: bool,
    pub display_on: bool,
    pub contrast: u8,
}

#[derive(Serialize, Deserialize)]
pub struct Pcd8544State {
    pub framebuffer: Vec<u8>,
    pub vram: Vec<u8>,
    pub x_addr: u8,
    pub y_addr: u8,
    pub extended_mode: bool,
    pub display_mode: u8,
    pub power_down: bool,
    pub vertical_addressing: bool,
}

// ─── Top-level save state ───────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct SaveState {
    // CPU
    pub pc: u16,
    pub sp: u16,
    pub sreg: u8,
    pub tick: u64,
    pub sleeping: bool,

    // Memory
    pub data: Vec<u8>,
    pub eeprom: Vec<u8>,

    // Display
    pub display: Ssd1306State,
    pub pcd8544: Pcd8544State,
    pub display_type: u8, // 0=Unknown, 1=Ssd1306, 2=Pcd8544

    // Timers
    pub timer0: Timer8State,
    pub timer1: Timer16State,
    pub timer2: Timer8State,
    pub timer3: Timer16State,
    pub timer4: Timer4State,

    // Peripherals
    pub spi: SpiState,
    pub adc: AdcState,
    pub pll: PllState,
    pub fx_flash: FxFlashState,

    // GPIO pins
    pub pin_b: u8,
    pub pin_c: u8,
    pub pin_d: u8,
    pub pin_e: u8,
    pub pin_f: u8,

    // Misc emulator state
    pub spdr_in: u8,
    pub rng_state: u32,
    pub frame_count: u32,
    pub fx_cs_prev: bool,
    pub pcd_cs_bit: u8,
    pub pcd_dc_bit: u8,
    pub speaker_prev_pc6: bool,
    pub speaker_last_edge: u64,
    pub speaker_half_period: u64,
    pub speaker_last_active: u64,
    pub speaker2_prev_pb5: bool,
    pub speaker2_last_edge: u64,
    pub speaker2_half_period: u64,
    pub speaker2_last_active: u64,
    pub usb_uenum: u8,
    pub usb_configured: bool,
    pub led_rgb: (u8, u8, u8),
    pub led_tx: bool,
    pub led_rx: bool,
    pub audio_left_level: bool,
    pub audio_right_level: bool,
}

// ─── Serialization ──────────────────────────────────────────────────────────

/// Serialize a save state to a self-describing byte blob (header + deflate).
///
/// Layout: `MAGIC(4) | version(4 LE) | cpu_type(1) | deflate(bincode(state))`.
/// This is the same container [`save_to_file`] writes, so blobs are
/// interchangeable between the desktop (file) and web (IndexedDB/bytes) paths.
pub fn save_to_bytes(state: &SaveState, cpu_type_byte: u8) -> Result<Vec<u8>, String> {
    let payload = bincode::serialize(state)
        .map_err(|e| format!("Serialize error: {}", e))?;

    let compressed = miniz_oxide::deflate::compress_to_vec(&payload, 6);

    let mut out = Vec::with_capacity(9 + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.push(cpu_type_byte);
    out.extend_from_slice(&compressed);
    Ok(out)
}

/// Parse a save state blob produced by [`save_to_bytes`], verifying magic,
/// version, and CPU type.
pub fn load_from_bytes(data: &[u8], expected_cpu_type: u8) -> Result<SaveState, String> {
    if data.len() < 9 {
        return Err("Save state too small".into());
    }
    if &data[0..4] != MAGIC {
        return Err("Invalid save state (bad magic)".into());
    }
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version != FORMAT_VERSION {
        return Err(format!("Unsupported save state version {} (expected {})",
            version, FORMAT_VERSION));
    }
    let cpu_type = data[8];
    if cpu_type != expected_cpu_type {
        let names = ["ATmega32u4", "ATmega328P"];
        return Err(format!("CPU type mismatch: save={} current={}",
            names.get(cpu_type as usize).unwrap_or(&"?"),
            names.get(expected_cpu_type as usize).unwrap_or(&"?")));
    }

    let decompressed = miniz_oxide::inflate::decompress_to_vec(&data[9..])
        .map_err(|e| format!("Decompress error: {:?}", e))?;

    bincode::deserialize(&decompressed)
        .map_err(|e| format!("Deserialize error: {}", e))
}

// ─── File I/O ───────────────────────────────────────────────────────────────

/// Save state to file with header and deflate compression.
pub fn save_to_file(state: &SaveState, cpu_type_byte: u8, path: &Path) -> Result<(), String> {
    let out = save_to_bytes(state, cpu_type_byte)?;
    std::fs::write(path, &out)
        .map_err(|e| format!("Write error: {}", e))
}

/// Load state from file, verifying magic, version, and CPU type.
pub fn load_from_file(path: &Path, expected_cpu_type: u8) -> Result<SaveState, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("Read error: {}", e))?;
    load_from_bytes(&data, expected_cpu_type)
}

/// Derive save state file path from game file path.
/// `game.hex` → `game.state`, `game.arduboy` → `game.state`
pub fn state_path(game_path: &str) -> String {
    let p = Path::new(game_path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    let dir = p.parent().unwrap_or(Path::new("."));
    dir.join(format!("{}.state", stem)).to_string_lossy().into_owned()
}
