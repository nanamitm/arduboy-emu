//! # arduboy-core
//!
//! Cycle-accurate emulation core for the Arduboy handheld game console (v0.8.1).
//!
//! Emulates the ATmega32u4 microcontroller (Arduboy) and ATmega328P (Gamebuino
//! Classic / Arduino Uno) with 16 MHz clock, 32 KB flash, 2–2.5 KB SRAM,
//! 1 KB EEPROM. Peripheral hardware: SSD1306 OLED display, PCD8544 Nokia LCD
//! (Gamebuino), SPI bus, Timer0/1/2/3/4, ADC, PLL, EEPROM controller,
//! W25Q128 FX external flash, and USB serial output.
//!
//! ## Architecture
//!
//! - [`Arduboy`] — Top-level emulator that wires together CPU, memory, and peripherals
//! - [`CpuType`] — Target CPU selection (ATmega32u4 or ATmega328P)
//! - [`Cpu`] — AVR CPU state (PC, SP, SREG, tick counter, sleep mode)
//! - [`Memory`] — Unified data space (registers + I/O + SRAM), flash, and EEPROM
//! - [`Ssd1306`] — SSD1306 128×64 monochrome OLED display controller
//! - [`pcd8544::Pcd8544`] — PCD8544 84×48 monochrome LCD (Gamebuino compatibility)
//! - [`peripherals`] — Timer8, Timer16, Timer4, SPI, ADC, PLL, EEPROM, FX flash
//! - [`disasm`] — Instruction disassembler for debug views
//! - [`profiler`] — Execution profiler with PC histogram and call graph
//! - [`debugger`] — RAM viewer, I/O register viewer, watchpoints
//! - [`gdb_server`] — GDB Remote Serial Protocol server for avr-gdb
//! - [`elf`] — ELF/DWARF parser for debug symbols and source-level debugging
//! - [`snapshot`] — Emulator state snapshots for rewind functionality
//! - [`savestate`] — Save state (quick save/load) with bincode serialization
//!
//! ## Audio
//!
//! Three audio generation methods are detected and reported via [`Arduboy::get_audio_tone`]:
//!
//! 1. **Timer3 CTC** — Standard Arduboy `tone()` using OC3A output compare toggle
//! 2. **Timer1 CTC** — Alternative timer-based tone generation
//! 3. **GPIO bit-bang** — Direct `digitalWrite` toggling of speaker pins
//!
//! Stereo output: Speaker 1 (PC6 on 32u4, PD3 on 328P) → left channel,
//! Speaker 2 (PB5) → right channel.

pub mod arduboy_file;
pub mod audio_buffer;
pub mod cpu;
pub mod debugger;
pub mod disasm;
pub mod display;
pub mod elf;
pub mod gdb_server;
pub mod gif;
pub mod hex;
pub mod memory;
pub mod opcodes;
pub mod pcd8544;
pub mod peripherals;
pub mod png;
pub mod profiler;
pub mod savestate;
pub mod snapshot;

pub use audio_buffer::AudioBuffer;
pub use cpu::Cpu;
pub use display::Ssd1306;
pub use memory::Memory;

// ATmega32u4 constants
/// Flash memory size: 32 KB
pub const FLASH_SIZE: usize = 32 * 1024;
/// SRAM size: 2.5 KB (2048 + 512) for ATmega32u4
pub const SRAM_SIZE: usize = 2 * 1024 + 512;
/// SRAM size: 2 KB for ATmega328P
pub const SRAM_SIZE_328P: usize = 2 * 1024;
/// EEPROM size: 1 KB
pub const EEPROM_SIZE: usize = 1024;
/// CPU clock frequency: 16 MHz
pub const CLOCK_HZ: u32 = 16_000_000;

/// SSD1306 display width in pixels
pub const SCREEN_WIDTH: usize = 128;
/// SSD1306 display height in pixels
pub const SCREEN_HEIGHT: usize = 64;

/// Number of general-purpose registers (R0–R31)
pub const REG_COUNT: usize = 32;
/// I/O + extended I/O register space size (0x20..0xFF)
pub const IO_SIZE: usize = 224;
/// Total data space (ATmega32u4): registers + I/O + SRAM
pub const DATA_SIZE: usize = REG_COUNT + IO_SIZE + SRAM_SIZE;
/// Total data space (ATmega328P)
pub const DATA_SIZE_328P: usize = REG_COUNT + IO_SIZE + SRAM_SIZE_328P;

/// Target CPU type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuType {
    /// ATmega32u4 (Arduboy, Leonardo)
    Atmega32u4,
    /// ATmega328P (Gamebuino Classic, Arduino Uno)
    Atmega328p,
}

/// Auto-detect CPU type from flash contents by examining the interrupt vector table.
///
/// ATmega328P has 26 vectors (byte addresses 0x00–0x64), while ATmega32u4 has
/// 43 vectors (0x00–0xA8). We check byte addresses 0x68–0xA8 (vectors 27–43):
/// if most are JMP/RJMP instructions, the binary targets ATmega32u4; otherwise
/// it targets ATmega328P (those addresses contain regular code, not vectors).
pub fn detect_cpu_type(flash: &[u8]) -> CpuType {
    if flash.len() < 0xAA {
        // Too small to tell — very short programs are likely 328P sketches
        return CpuType::Atmega328p;
    }

    // Count JMP/RJMP instructions in the 32u4-only vector region (0x68..0xA8)
    let mut jmp_count = 0;
    let mut checked = 0;
    let mut addr = 0x68;
    while addr <= 0xA8 {
        let w = (flash[addr] as u16) | ((flash[addr + 1] as u16) << 8);
        // JMP: 1001_010k_kkkk_110k → (w & 0xFE0E) == 0x940C
        // RJMP: 1100_kkkk_kkkk_kkkk → (w & 0xF000) == 0xC000
        if (w & 0xFE0E) == 0x940C || (w & 0xF000) == 0xC000 {
            jmp_count += 1;
        }
        checked += 1;
        // JMP is 4 bytes, RJMP is 2 bytes; vector entries are always 4 bytes (2 words)
        addr += 4;
    }

    // If ≥60% of the checked slots look like vector entries → 32u4
    if jmp_count * 10 >= checked * 6 {
        CpuType::Atmega32u4
    } else {
        CpuType::Atmega328p
    }
}

// SREG bit positions
pub const SREG_C: u8 = 0;
pub const SREG_Z: u8 = 1;
pub const SREG_N: u8 = 2;
pub const SREG_V: u8 = 3;
pub const SREG_S: u8 = 4;
pub const SREG_H: u8 = 5;
pub const SREG_T: u8 = 6;
pub const SREG_I: u8 = 7;

// I/O register addresses (data space addresses, not I/O addresses)
pub const SREG_ADDR: u16 = 0x5F;
pub const SPH_ADDR: u16 = 0x5E;
pub const SPL_ADDR: u16 = 0x5D;
/// Watchdog control register (same address on ATmega32u4 and ATmega328P).
pub const WDTCSR_ADDR: u16 = 0x60;
/// MCU Status Register (same address on ATmega32u4 and ATmega328P).
pub const MCUSR_ADDR: u16 = 0x54;
/// Watchdog Reset Flag bit within MCUSR.
pub const WDRF_BIT: u8 = 3;

/// Arduboy button identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
}

/// Main Arduboy emulator combining all subsystems
pub struct Arduboy {
    pub cpu: Cpu,
    pub mem: Memory,
    pub display: Ssd1306,
    pub timer0: peripherals::Timer8,
    pub timer1: peripherals::Timer16,
    pub timer3: peripherals::Timer16,
    pub timer4: peripherals::Timer4,
    /// Timer2 (ATmega328P only, 8-bit async)
    pub timer2: peripherals::Timer8,
    pub watchdog: peripherals::Watchdog,
    pub spi: peripherals::Spi,
    pub pll: peripherals::Pll,
    pub adc: peripherals::Adc,
    pub eeprom_ctrl: peripherals::EepromCtrl,
    /// Arduboy FX external SPI flash
    pub fx_flash: peripherals::FxFlash,
    /// SPI data received from flash (MISO byte)
    spdr_in: u8,
    /// Pin states for GPIO (active-low buttons etc)
    pub pin_b: u8,
    pub pin_c: u8,
    pub pin_d: u8,
    pub pin_e: u8,
    pub pin_f: u8,
    /// SPI output buffer with raw port state per byte
    spi_out: Vec<(u8, u8, u8, u8)>, // (byte, portd_val, portf_val, portc_val)
    /// Random state for ADC
    rng_state: u32,
    /// Debug counter: total SPDR writes since reset
    pub dbg_spdr_writes: u64,
    /// Count of unknown/illegal opcodes executed since reset (0 for well-behaved
    /// ROMs; non-zero usually means the CPU ran off into data as code).
    pub unknown_ops: u64,
    /// Display type detection
    pub display_type: DisplayType,
    /// PCD8544 display (Gamebuino)
    pub pcd8544: pcd8544::Pcd8544,
    /// Frame counter for debug
    frame_count: u32,
    /// Set when a watchdog reset restarts the MCU mid-frame, so `run_frame`
    /// can stop cleanly (the cycle counter has jumped back to 0).
    did_reset: bool,
    /// Track previous PD1 state for FX CS edge detection
    fx_cs_prev: bool,
    /// PCD8544 CS bit position in PORTC (0xFF = not yet detected, ATmega328P only)
    pcd_cs_bit: u8,
    /// PCD8544 DC bit position in PORTC (0xFF = not yet detected, ATmega328P only)
    pcd_dc_bit: u8,
    /// Debug: FX SPI transfer count
    pub dbg_fx_transfers: u64,
    /// Debug: FX CS select/deselect count
    pub dbg_fx_cs_count: u64,
    /// Debug: bytes in current FX CS transaction
    dbg_fx_bytes_in_cs: u32,
    /// Enable debug output (eprintln)
    pub debug: bool,
    /// GPIO speaker 1: previous state for edge detection
    /// ATmega32u4: PC6 (Arduboy Speaker 1)
    /// ATmega328P: PD3 (Gamebuino Classic speaker)
    speaker_prev_pc6: bool,
    /// GPIO speaker 1: tick of last edge
    speaker_last_edge: u64,
    /// GPIO speaker 1: measured half-period in ticks
    speaker_half_period: u64,
    /// GPIO speaker 1: tick when last tone was detected
    speaker_last_active: u64,
    /// GPIO speaker 2 (PB5): previous state for edge detection
    speaker2_prev_pb5: bool,
    /// GPIO speaker 2: tick of last PB5 edge
    speaker2_last_edge: u64,
    /// GPIO speaker 2: measured half-period in ticks
    speaker2_half_period: u64,
    /// GPIO speaker 2: tick when last tone was detected
    speaker2_last_active: u64,
    /// Breakpoint addresses (word addresses)
    pub breakpoints: Vec<u16>,
    /// True if execution stopped at a breakpoint
    pub breakpoint_hit: bool,
    /// USB Serial output buffer (UEDATX writes)
    pub serial_buf: Vec<u8>,
    /// SPI byte trace for diagnostics (first 50 entries when enabled)
    pub spi_trace: Vec<String>,
    pub spi_trace_enabled: bool,
    /// USB endpoint number (UENUM register)
    usb_uenum: u8,
    /// USB device configured flag
    usb_configured: bool,
    /// Sample-accurate audio waveform buffer
    pub audio_buf: AudioBuffer,
    /// RGB LED state: (red, green, blue) brightness 0–255
    pub led_rgb: (u8, u8, u8),
    /// TX LED state (PD5, active-low)
    pub led_tx: bool,
    /// RX LED state (PB0, active-low)
    pub led_rx: bool,
    /// EEPROM dirty flag (true if modified since last save)
    pub eeprom_dirty: bool,
    /// Target CPU type
    pub cpu_type: CpuType,
    /// Actual SRAM size (varies by CPU type)
    sram_size: usize,
    /// Execution profiler (zero-cost when disabled)
    pub profiler: profiler::Profiler,
    /// Advanced debugger (watchpoints, RAM viewer)
    pub debugger: debugger::Debugger,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayType {
    Unknown,
    Ssd1306,
    Pcd8544,
}

impl Arduboy {
    /// Create a new Arduboy emulator (ATmega32u4) with all peripherals in reset state.
    pub fn new() -> Self {
        Self::new_with_cpu(CpuType::Atmega32u4)
    }

    /// Create a new emulator for the specified CPU type.
    pub fn new_with_cpu(cpu_type: CpuType) -> Self {
        let sram_size = match cpu_type {
            CpuType::Atmega32u4 => SRAM_SIZE,
            CpuType::Atmega328p => SRAM_SIZE_328P,
        };
        let data_size = REG_COUNT + IO_SIZE + sram_size;

        // Timer0: same register addresses on both chips, different interrupt vectors
        let timer0_addrs = match cpu_type {
            CpuType::Atmega32u4 => peripherals::Timer8Addrs {
                tifr: 0x35,
                tccr_a: 0x44,
                tccr_b: 0x45,
                ocr_a: 0x47,
                ocr_b: 0x48,
                timsk: 0x6E,
                tcnt: 0x46,
                int_ovf: peripherals::INT_TIMER0_OVF,
                int_compa: peripherals::INT_TIMER0_COMPA,
                int_compb: peripherals::INT_TIMER0_COMPB,
                is_timer2: false,
            },
            CpuType::Atmega328p => peripherals::Timer8Addrs {
                tifr: 0x35,
                tccr_a: 0x44,
                tccr_b: 0x45,
                ocr_a: 0x47,
                ocr_b: 0x48,
                timsk: 0x6E,
                tcnt: 0x46,
                int_ovf: peripherals::INT_328P_TIMER0_OVF,
                int_compa: peripherals::INT_328P_TIMER0_COMPA,
                int_compb: peripherals::INT_328P_TIMER0_COMPB,
                is_timer2: false,
            },
        };

        // Timer1: same register addresses, different vectors
        let timer1_addrs = match cpu_type {
            CpuType::Atmega32u4 => peripherals::Timer16Addrs {
                tifr: 0x36,
                tccr_a: 0x80,
                tccr_b: 0x81,
                tccr_c: 0x82,
                ocr_ah: 0x89,
                ocr_al: 0x88,
                ocr_bh: 0x8B,
                ocr_bl: 0x8A,
                ocr_ch: 0x8D,
                ocr_cl: 0x8C,
                timsk: 0x6F,
                tcnth: 0x85,
                tcntl: 0x84,
                int_ovf: peripherals::INT_TIMER1_OVF,
                int_compa: peripherals::INT_TIMER1_COMPA,
                int_compb: peripherals::INT_TIMER1_COMPB,
                int_compc: peripherals::INT_TIMER1_COMPC,
            },
            CpuType::Atmega328p => peripherals::Timer16Addrs {
                tifr: 0x36,
                tccr_a: 0x80,
                tccr_b: 0x81,
                tccr_c: 0x82,
                ocr_ah: 0x89,
                ocr_al: 0x88,
                ocr_bh: 0x8B,
                ocr_bl: 0x8A,
                ocr_ch: 0x8D,
                ocr_cl: 0x8C, // 328P has no OCR1C but addr harmless
                timsk: 0x6F,
                tcnth: 0x85,
                tcntl: 0x84,
                int_ovf: peripherals::INT_328P_TIMER1_OVF,
                int_compa: peripherals::INT_328P_TIMER1_COMPA,
                int_compb: peripherals::INT_328P_TIMER1_COMPB,
                int_compc: 0, // no compare C on 328P
            },
        };

        // Timer3: ATmega32u4 only
        let timer3_addrs = peripherals::Timer16Addrs {
            tifr: 0x38,
            tccr_a: 0x90,
            tccr_b: 0x91,
            tccr_c: 0x92,
            ocr_ah: 0x99,
            ocr_al: 0x98,
            ocr_bh: 0x9B,
            ocr_bl: 0x9A,
            ocr_ch: 0x9D,
            ocr_cl: 0x9C,
            timsk: 0x71,
            tcnth: 0x94,
            tcntl: 0x95,
            int_ovf: peripherals::INT_TIMER3_OVF,
            int_compa: peripherals::INT_TIMER3_COMPA,
            int_compb: peripherals::INT_TIMER3_COMPB,
            int_compc: peripherals::INT_TIMER3_COMPC,
        };

        // Timer2: ATmega328P only (8-bit, different addresses from Timer0)
        let timer2_addrs = peripherals::Timer8Addrs {
            tifr: 0x37,
            tccr_a: 0xB0,
            tccr_b: 0xB1,
            ocr_a: 0xB3,
            ocr_b: 0xB4,
            timsk: 0x70,
            tcnt: 0xB2,
            int_ovf: peripherals::INT_328P_TIMER2_OVF,
            int_compa: peripherals::INT_328P_TIMER2_COMPA,
            int_compb: peripherals::INT_328P_TIMER2_COMPB,
            is_timer2: true,
        };

        let mut ard = Arduboy {
            cpu: Cpu::new(),
            mem: Memory::new_with_size(data_size),
            display: Ssd1306::new(),
            timer0: peripherals::Timer8::new(timer0_addrs),
            timer1: peripherals::Timer16::new(timer1_addrs),
            timer3: peripherals::Timer16::new(timer3_addrs),
            timer4: peripherals::Timer4::new(),
            timer2: peripherals::Timer8::new(timer2_addrs),
            watchdog: peripherals::Watchdog::new(
                if cpu_type == CpuType::Atmega328p {
                    peripherals::INT_328P_WDT
                } else {
                    peripherals::INT_WDT
                },
                WDTCSR_ADDR,
            ),
            spi: peripherals::Spi::new(),
            pll: peripherals::Pll::new(),
            adc: peripherals::Adc::new(),
            eeprom_ctrl: peripherals::EepromCtrl::new(),
            fx_flash: peripherals::FxFlash::new(),
            spdr_in: 0,
            pin_b: 0xFF,
            pin_c: 0xFF,
            pin_d: 0xFF,
            pin_e: 0xFF,
            pin_f: 0xFF,
            spi_out: Vec::new(),
            rng_state: 0xDEAD_BEEF,
            dbg_spdr_writes: 0,
            unknown_ops: 0,
            display_type: if cpu_type == CpuType::Atmega328p {
                DisplayType::Pcd8544
            } else {
                DisplayType::Unknown
            },
            pcd8544: pcd8544::Pcd8544::new(),
            frame_count: 0,
            did_reset: false,
            fx_cs_prev: true,
            // Default Gamebuino Classic pin mapping: DC=PC2(A2), CS=PC1(A1)
            // Auto-detection in flush_spi may override these.
            pcd_cs_bit: if cpu_type == CpuType::Atmega328p {
                1
            } else {
                0xFF
            },
            pcd_dc_bit: if cpu_type == CpuType::Atmega328p {
                2
            } else {
                0xFF
            },
            dbg_fx_transfers: 0,
            dbg_fx_cs_count: 0,
            dbg_fx_bytes_in_cs: 0,
            debug: false,
            speaker_prev_pc6: false,
            speaker_last_edge: 0,
            speaker_half_period: 0,
            speaker_last_active: 0,
            speaker2_prev_pb5: false,
            speaker2_last_edge: 0,
            speaker2_half_period: 0,
            speaker2_last_active: 0,
            breakpoints: Vec::new(),
            breakpoint_hit: false,
            serial_buf: Vec::new(),
            spi_trace: Vec::new(),
            spi_trace_enabled: false,
            usb_uenum: 0,
            usb_configured: false,
            audio_buf: AudioBuffer::new(),
            led_rgb: (0, 0, 0),
            led_tx: false,
            led_rx: false,
            eeprom_dirty: false,
            cpu_type,
            sram_size,
            profiler: profiler::Profiler::new(),
            debugger: debugger::Debugger::new(),
        };
        // Initialize SP to top of SRAM
        let sp = (data_size - 1) as u16;
        ard.mem.data[SPH_ADDR as usize] = (sp >> 8) as u8;
        ard.mem.data[SPL_ADDR as usize] = (sp & 0xFF) as u8;
        ard.cpu.sp = sp;

        // ATmega328P defaults: PCD8544 display, DC=PC2(A2), CS=PC1(A1).
        // Auto-detection in flush_spi may override CS/DC pins for non-standard configs.

        ard
    }

    /// Load an Intel HEX file into flash memory and reset the CPU.
    ///
    /// Returns the number of bytes loaded on success.
    pub fn load_hex(&mut self, hex_str: &str) -> Result<usize, String> {
        let size = hex::parse_hex(hex_str, &mut self.mem.flash)?;
        self.reset();
        Ok(size)
    }

    /// Load FX flash data from binary at offset 0. Use load_fx_layout for correct placement.
    pub fn load_fx_data(&mut self, bin: &[u8]) {
        self.fx_flash.load_data(bin);
    }

    /// Load FX flash data at a specific offset.
    pub fn load_fx_data_at(&mut self, bin: &[u8], offset: usize) {
        self.fx_flash.load_data_at(bin, offset);
    }

    /// Load FX data + save at the standard ArduboyFX flash layout.
    ///
    /// The 16MB W25Q128 flash is laid out as:
    /// ```text
    /// [... empty ...][FX data (page-aligned)][FX save (4KB-aligned)][end = 16MB]
    /// ```
    ///
    /// Returns (data_page, save_page) for diagnostic display.
    pub fn load_fx_layout(&mut self, data: &[u8], save: Option<&[u8]>) -> (u16, u16) {
        const TOTAL_PAGES: usize = 65536; // 16MB / 256
        let save_len = save.map(|s| s.len()).unwrap_or(0);
        // Save area: 4KB (sector) aligned, in pages (16 pages per 4KB)
        let save_pages = if save_len > 0 {
            ((save_len + 4095) / 4096) * 16
        } else {
            0
        };
        // Data area: 256-byte (page) aligned
        let data_pages = (data.len() + 255) / 256;

        let save_start_page = TOTAL_PAGES - save_pages;
        let data_start_page = save_start_page - data_pages;

        let data_offset = data_start_page * 256;
        let save_offset = save_start_page * 256;

        self.fx_flash.load_data_at(data, data_offset);
        if let Some(save_data) = save {
            if !save_data.is_empty() {
                self.fx_flash.load_data_at(save_data, save_offset);
            }
        }

        (data_start_page as u16, save_start_page as u16)
    }

    /// Reset the CPU and all peripherals to power-on state.
    ///
    /// Flash and FX flash data are preserved (they represent ROM content).
    pub fn reset(&mut self) {
        self.cpu = Cpu::new();
        self.mem.data.fill(0);
        let data_size = REG_COUNT + IO_SIZE + self.sram_size;
        let sp = (data_size - 1) as u16;
        self.mem.data[SPH_ADDR as usize] = (sp >> 8) as u8;
        self.mem.data[SPL_ADDR as usize] = (sp & 0xFF) as u8;
        self.cpu.sp = sp;
        self.display = Ssd1306::new();
        self.pcd8544 = pcd8544::Pcd8544::new();
        self.display_type = if self.cpu_type == CpuType::Atmega328p {
            DisplayType::Pcd8544
        } else {
            DisplayType::Unknown
        };
        self.timer0.reset();
        self.timer1.reset();
        self.timer3.reset();
        self.timer4.reset();
        self.timer2.reset();
        self.watchdog.reset();
        self.spi.reset();
        self.pll.reset();
        self.adc.reset();
        self.eeprom_ctrl.reset();
        self.pin_b = 0xFF;
        self.pin_c = 0xFF;
        self.pin_d = 0xFF;
        self.pin_e = 0xFF;
        self.pin_f = 0xFF;
        self.spi_out.clear();
        self.spdr_in = 0;
        self.fx_cs_prev = true;
        // Default Gamebuino Classic: DC=PC2, CS=PC1
        self.pcd_cs_bit = if self.cpu_type == CpuType::Atmega328p {
            1
        } else {
            0xFF
        };
        self.pcd_dc_bit = if self.cpu_type == CpuType::Atmega328p {
            2
        } else {
            0xFF
        };
        self.unknown_ops = 0;
        self.dbg_fx_transfers = 0;
        self.dbg_fx_cs_count = 0;
        self.dbg_fx_bytes_in_cs = 0;
        self.speaker_prev_pc6 = false;
        self.speaker_last_edge = 0;
        self.speaker_half_period = 0;
        self.speaker_last_active = 0;
        self.speaker2_prev_pb5 = false;
        self.speaker2_last_edge = 0;
        self.speaker2_half_period = 0;
        self.speaker2_last_active = 0;
        self.breakpoint_hit = false;
        self.serial_buf.clear();
        self.spi_trace.clear();
        self.usb_uenum = 0;
        self.usb_configured = false;
        self.led_rgb = (0, 0, 0);
        self.led_tx = false;
        self.led_rx = false;
        // USART0 initial state (328P): UDRE0=1 (ready to transmit)
        if self.cpu_type == CpuType::Atmega328p {
            self.mem.data[0xC0] = 0x20; // UCSR0A: UDRE0=1
        }
        // Note: eeprom_dirty is NOT cleared on reset (tracks unsaved changes)
        // Note: FX flash data is NOT cleared on reset (persistent storage)
        // Note: breakpoints are NOT cleared on reset
    }

    /// Set button state (true = pressed)
    pub fn set_button(&mut self, btn: Button, pressed: bool) {
        // Active-low: pressed = bit cleared, released = bit set

        match self.cpu_type {
            CpuType::Atmega32u4 => {
                // --- Arduboy pin mapping (32u4) ---
                // UP=PF7, DOWN=PF4, LEFT=PF5, RIGHT=PF6, A=PE6, B=PB4
                if self.display_type != DisplayType::Pcd8544 {
                    let (pin, bit): (&mut u8, u8) = match btn {
                        Button::Up => (&mut self.pin_f, 7),
                        Button::Down => (&mut self.pin_f, 4),
                        Button::Left => (&mut self.pin_f, 5),
                        Button::Right => (&mut self.pin_f, 6),
                        Button::A => (&mut self.pin_e, 6),
                        Button::B => (&mut self.pin_b, 4),
                    };
                    if pressed {
                        *pin &= !(1 << bit);
                    } else {
                        *pin |= 1 << bit;
                    }
                }

                // --- Gamebuino pin mapping (32u4 with PCD8544) ---
                // UP=PB5(9), DOWN=PD7(6), LEFT=PB4(8), RIGHT=PE6(7), A=PD4(4), B=PD1(2)
                if self.display_type != DisplayType::Ssd1306 {
                    let (pin2, bit2): (&mut u8, u8) = match btn {
                        Button::Up => (&mut self.pin_b, 5),
                        Button::Down => (&mut self.pin_d, 7),
                        Button::Left => (&mut self.pin_b, 4),
                        Button::Right => (&mut self.pin_e, 6),
                        Button::A => (&mut self.pin_d, 4),
                        Button::B => (&mut self.pin_d, 1),
                    };
                    if pressed {
                        *pin2 &= !(1 << bit2);
                    } else {
                        *pin2 |= 1 << bit2;
                    }
                }
            }
            CpuType::Atmega328p => {
                // --- Gamebuino Classic pin mapping (328P) ---
                // UP=PB1(D9), DOWN=PD6(D6), LEFT=PB0(D8), RIGHT=PD7(D7)
                // A=PD4(D4), B=PD2(D2)
                let (pin, bit): (&mut u8, u8) = match btn {
                    Button::Up => (&mut self.pin_b, 1),
                    Button::Down => (&mut self.pin_d, 6),
                    Button::Left => (&mut self.pin_b, 0),
                    Button::Right => (&mut self.pin_d, 7),
                    Button::A => (&mut self.pin_d, 4),
                    Button::B => (&mut self.pin_d, 2),
                };
                if pressed {
                    *pin &= !(1 << bit);
                } else {
                    *pin |= 1 << bit;
                }
            }
        }
    }

    /// Run one frame of emulation (~13.5ms = ~216000 cycles at 16MHz)
    pub fn run_frame(&mut self) {
        let cycles = (CLOCK_HZ as u64 * 135) / 10000; // 216000
        let end_tick = self.cpu.tick + cycles;
        let mut last_update = self.cpu.tick;
        self.did_reset = false;

        // Begin sample-accurate audio recording for this frame
        self.audio_buf.begin_frame(self.cpu.tick);

        // PC sampling for stuck detection (debug only)
        let mut pc_counts: Option<std::collections::HashMap<u16, u32>> = if self.debug {
            Some(std::collections::HashMap::new())
        } else {
            None
        };
        let mut last_sample = self.cpu.tick;

        while self.cpu.tick < end_tick {
            if !self.cpu.sleeping {
                let pc_byte = self.cpu.pc as usize * 2;
                if pc_byte >= self.mem.flash.len() {
                    self.cpu.pc = 0;
                }

                // Check breakpoints
                if !self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc) {
                    self.breakpoint_hit = true;
                    return;
                }

                // Check watchpoint hits
                if self.debugger.watch_hit.is_some() {
                    self.breakpoint_hit = true;
                    return;
                }

                if let Some(ref mut counts) = pc_counts {
                    if self.cpu.tick - last_sample >= 64 {
                        last_sample = self.cpu.tick;
                        *counts.entry(self.cpu.pc).or_insert(0) += 1;
                    }
                }

                self.step();
            } else {
                self.cpu.tick += 4;
            }

            if self.cpu.tick - last_update >= 128 {
                last_update = self.cpu.tick;
                self.flush_spi();
                self.update_peripherals();
                // A watchdog reset restarted the MCU (tick jumped back to 0);
                // end the frame here so the stale local counters aren't reused.
                if self.did_reset {
                    self.flush_spi();
                    return;
                }
            }
        }
        self.update_peripherals();
        self.flush_spi();

        // End sample-accurate audio recording for this frame
        self.audio_buf.end_frame(self.cpu.tick);

        self.frame_count += 1;

        // Per-frame diagnostics (first 10 frames)
        if self.debug && self.frame_count <= 10 {
            eprintln!("Frame {}: SPI={} FX={} disp_cmd={} disp_data={} sleeping={} pc=0x{:04X} display_type={:?}",
                self.frame_count, self.dbg_spdr_writes, self.dbg_fx_transfers,
                self.display.dbg_cmd_count, self.display.dbg_data_count,
                self.cpu.sleeping, self.cpu.pc, self.display_type);
        }
        // PCD8544 diagnostics (debug mode only)
        if self.debug && self.cpu_type == CpuType::Atmega328p && self.frame_count <= 5 {
            eprintln!("[PCD] F{}: SPI={} pcd_cmd={} pcd_data={} type={:?} cs_bit={} dc_bit={} DDRC=0x{:02X} PORTC=0x{:02X} vram[0..4]={:02X},{:02X},{:02X},{:02X} dmode={}",
                self.frame_count, self.dbg_spdr_writes,
                self.pcd8544.dbg_cmd_count, self.pcd8544.dbg_data_count,
                self.display_type, self.pcd_cs_bit, self.pcd_dc_bit,
                self.mem.data[0x27], self.mem.data[0x28],
                self.pcd8544.vram[0], self.pcd8544.vram[1], self.pcd8544.vram[2], self.pcd8544.vram[3],
                self.pcd8544.display_mode);
        }
        // FX diagnostics for first 5 frames
        if self.debug && self.fx_flash.loaded && self.frame_count <= 5 {
            eprintln!("[FX-diag] F{}: SPI_total={} FX_xfer={} disp_cmd={} disp_data={} sleeping={} pc=0x{:04X} DDRD=0x{:02X} PORTD=0x{:02X} display={:?}",
                self.frame_count, self.dbg_spdr_writes, self.dbg_fx_transfers,
                self.display.dbg_cmd_count, self.display.dbg_data_count,
                self.cpu.sleeping, self.cpu.pc,
                self.mem.data[0x2A], self.mem.data[0x2B],
                self.display_type);
        }

        if let Some(pc_counts) = pc_counts {
            if self.frame_count <= 5 && !pc_counts.is_empty() {
                let mut top: Vec<_> = pc_counts.into_iter().collect();
                top.sort_by(|a, b| b.1.cmp(&a.1));
                let top5: Vec<String> = top
                    .iter()
                    .take(5)
                    .map(|(pc, cnt)| {
                        let byte_addr = (*pc as usize) * 2;
                        let opcode = if byte_addr + 1 < self.mem.flash.len() {
                            (self.mem.flash[byte_addr] as u16)
                                | ((self.mem.flash[byte_addr + 1] as u16) << 8)
                        } else {
                            0
                        };
                        format!("0x{:04X}(op=0x{:04X})x{}", pc, opcode, cnt)
                    })
                    .collect();
                eprintln!("  PC hotspots F{}: {}", self.frame_count, top5.join(", "));
            }
        }
    }

    /// Execute a single instruction
    fn step(&mut self) {
        let pc = self.cpu.pc as usize;
        let word = self.mem.read_program_word(pc);
        let next_word = if pc + 1 < FLASH_SIZE / 2 {
            self.mem.read_program_word(pc + 1)
        } else {
            0
        };
        let (inst, size) = opcodes::decode(word, next_word);

        // Profiler: record PC hit and call/ret tracking
        if self.profiler.enabled {
            self.profiler.record(self.cpu.pc);
            match inst {
                opcodes::Instruction::Call { k } => {
                    self.profiler.record_call(self.cpu.pc, k as u16);
                }
                opcodes::Instruction::Rcall { k } => {
                    let target = (self.cpu.pc as i32 + 1 + k as i32) as u16;
                    self.profiler.record_call(self.cpu.pc, target);
                }
                opcodes::Instruction::Icall => {
                    let z = self.mem.z();
                    self.profiler.record_call(self.cpu.pc, z);
                }
                opcodes::Instruction::Eicall => {
                    let z = self.mem.z();
                    self.profiler.record_call(self.cpu.pc, z);
                }
                opcodes::Instruction::Ret | opcodes::Instruction::Reti => {
                    self.profiler.record_ret();
                }
                _ => {}
            }
        }

        let cycles = self.execute_inst(inst, size);
        self.cpu.tick += cycles as u64;
    }

    /// Execute a single instruction and return its disassembly.
    ///
    /// Used by the debugger for step-by-step execution.
    pub fn step_one(&mut self) -> String {
        let pc = self.cpu.pc;
        let word = self.mem.read_program_word(pc as usize);
        let next_word = if (pc as usize) + 1 < FLASH_SIZE / 2 {
            self.mem.read_program_word(pc as usize + 1)
        } else {
            0
        };
        let (inst, size) = opcodes::decode(word, next_word);
        let asm = disasm::disassemble(inst, pc);
        let cycles = self.execute_inst(inst, size);
        self.cpu.tick += cycles as u64;
        // Update peripherals after each step
        self.flush_spi();
        self.update_peripherals();
        format!("0x{:04X}: {}", pc * 2, asm)
    }

    /// Disassemble the instruction at the current PC without executing it.
    pub fn disasm_at_pc(&self) -> String {
        let pc = self.cpu.pc;
        let word = self.mem.read_program_word(pc as usize);
        let next_word = if (pc as usize) + 1 < FLASH_SIZE / 2 {
            self.mem.read_program_word(pc as usize + 1)
        } else {
            0
        };
        let (inst, _) = opcodes::decode(word, next_word);
        let asm = disasm::disassemble(inst, pc);
        format!("0x{:04X}: {}", pc * 2, asm)
    }

    /// Format a register dump string with R0-R31, SP, PC, SREG.
    pub fn dump_regs(&self) -> String {
        let mut s = String::new();
        for i in 0..32 {
            if i % 8 == 0 && i > 0 {
                s.push('\n');
            }
            s.push_str(&format!("R{:2}={:02X} ", i, self.mem.data[i]));
        }
        s.push_str(&format!(
            "\nPC={:04X} SP={:04X} SREG={} (0x{:02X})",
            self.cpu.pc * 2,
            self.cpu.sp,
            disasm::format_sreg(self.cpu.sreg),
            self.cpu.sreg
        ));
        s.push_str(&format!(
            "\nX={:04X} Y={:04X} Z={:04X}",
            self.mem.x(),
            self.mem.y(),
            self.mem.z()
        ));
        s
    }

    /// Dump RAM region as hex + ASCII.
    pub fn dump_ram(&self, start: u16, length: u16) -> String {
        debugger::dump_ram(&self.mem.data, start, length)
    }

    /// Dump I/O registers with names and non-zero values.
    pub fn dump_io(&self) -> String {
        debugger::dump_io_regs(&self.mem.data, self.cpu_type == CpuType::Atmega328p)
    }

    /// Dump all I/O registers (compact format).
    pub fn dump_io_all(&self) -> String {
        debugger::dump_io_regs_all(&self.mem.data, self.cpu_type == CpuType::Atmega328p)
    }

    /// Get profiler report string.
    pub fn profiler_report(&self) -> String {
        self.profiler.report(&self.mem.flash)
    }

    /// Get register values as a 32-byte array (for GDB).
    pub fn gdb_regs(&self) -> [u8; 32] {
        let mut r = [0u8; 32];
        r.copy_from_slice(&self.mem.data[0..32]);
        r
    }

    /// Take and clear accumulated USB serial output bytes.
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.serial_buf)
    }

    /// Save EEPROM contents to a byte vector.
    pub fn save_eeprom(&self) -> Vec<u8> {
        self.mem.eeprom.clone()
    }

    /// Load EEPROM contents from a byte slice.
    pub fn load_eeprom(&mut self, data: &[u8]) {
        let len = data.len().min(EEPROM_SIZE);
        self.mem.eeprom[..len].copy_from_slice(&data[..len]);
        self.eeprom_dirty = false;
    }

    /// Get current RGB LED state as (red, green, blue).
    ///
    /// Arduboy LED pins: Red=PB6(OC1B), Green=PB7(OC1C), Blue=PB5(OC1A).
    /// Returns PWM duty or digital on/off approximation.
    pub fn get_led_state(&self) -> (u8, u8, u8) {
        self.led_rgb
    }

    /// Read from data space with peripheral hooks
    pub fn read_data(&mut self, addr: u16) -> u8 {
        let a = addr as usize;

        // GPIO PIN reads: merge input (buttons/external) with output state
        // For output pins (DDRx bit = 1): return PORTx value
        // For input pins (DDRx bit = 0): return pin_x (external input/buttons)
        match addr {
            0x23 => {
                // PINB
                let ddr = self.mem.data[0x24];
                let port = self.mem.data[0x25];
                return (port & ddr) | (self.pin_b & !ddr);
            }
            0x26 => {
                // PINC
                let ddr = self.mem.data[0x27];
                let port = self.mem.data[0x28];
                return (port & ddr) | (self.pin_c & !ddr);
            }
            0x29 => {
                // PIND
                let ddr = self.mem.data[0x2A];
                let port = self.mem.data[0x2B];
                return (port & ddr) | (self.pin_d & !ddr);
            }
            0x2C => {
                // PINE
                let ddr = self.mem.data[0x2D];
                let port = self.mem.data[0x2E];
                return (port & ddr) | (self.pin_e & !ddr);
            }
            0x2F => {
                // PINF
                let ddr = self.mem.data[0x30];
                let port = self.mem.data[0x31];
                return (port & ddr) | (self.pin_f & !ddr);
            }
            _ => {}
        }

        // Timer0 reads
        if let Some(v) = self.timer0.read(addr, self.cpu.tick, &self.mem.data) {
            return v;
        }
        // Timer1 reads
        if let Some(v) = self.timer1.read(addr, self.cpu.tick, &self.mem.data) {
            return v;
        }
        // Timer3 reads
        if let Some(v) = self.timer3.read(addr, self.cpu.tick, &self.mem.data) {
            return v;
        }
        // Timer4 reads (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            if let Some(v) = self.timer4.read(addr) {
                return v;
            }
        }
        // Timer2 reads (ATmega328P only)
        if self.cpu_type == CpuType::Atmega328p {
            if let Some(v) = self.timer2.read(addr, self.cpu.tick, &self.mem.data) {
                return v;
            }
        }
        // SPI reads
        if let Some(v) = self.spi.read(addr) {
            return v;
        }
        // PLL read
        if addr == 0x49 {
            return self.pll.read();
        }
        // EEPROM data read
        if addr == 0x40 {
            let ea = self.mem.data[0x41] as u16 | ((self.mem.data[0x42] as u16) << 8);
            return self.mem.eeprom.get(ea as usize).copied().unwrap_or(0xFF);
        }
        // ADC reads
        if let Some(v) = self.adc.read(addr) {
            return v;
        }

        // USB Serial register reads (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            match addr {
                0xE8 => {
                    // UEINTX - always report ready to send
                    return 0xA1;
                }
                0xE9 => return self.usb_uenum, // UENUM
                0xEE => return 0x61,           // UESTA0X
                0xEF => return 0x00,           // UESTA1X
                0xF2 => return 0x40,           // UEBCLX
                0xF3 => return 0x00,           // UEBCHX
                0xD8 => {
                    // USBCON
                    return if self.usb_configured { 0x80 } else { 0 };
                }
                0xD9 => return 0x08, // USBSTA
                0xE3 => return 0x80, // UDADDR
                _ => {}
            }
        }

        // USART0 register reads (ATmega328P only)
        if self.cpu_type == CpuType::Atmega328p {
            match addr {
                0xC0 => {
                    // UCSR0A — always report UDRE0=1 (ready), TXC0
                    return 0x20 | (self.mem.data[0xC0] & 0x40);
                }
                0xC1 => return self.mem.data[0xC1], // UCSR0B
                0xC6 => return 0x00,                // UDR0 — no receive data
                _ => {}
            }
        }

        if a < self.mem.data.len() {
            let v = self.mem.data[a];
            if !self.debugger.watchpoints.is_empty() {
                self.debugger.check_read(addr, v);
            }
            v
        } else {
            0
        }
    }

    /// Write to data space with peripheral hooks
    pub fn write_data(&mut self, addr: u16, value: u8) {
        let a = addr as usize;
        let old = if a < self.mem.data.len() {
            self.mem.data[a]
        } else {
            0
        };

        // Watchpoint check (fast path: skip if no watchpoints)
        if !self.debugger.watchpoints.is_empty() {
            self.debugger.check_write(addr, old, value);
        }

        // PINx toggle writes: writing 1 to PINx bit toggles PORTx bit
        match addr {
            0x23 => {
                // PINB → toggles PORTB
                let new_portb = self.mem.data[0x25] ^ value;
                // Re-invoke write_data for PORTB so speaker/LED side effects fire
                self.write_data(0x25, new_portb);
                return;
            }
            0x26 => {
                // PINC → toggles PORTC
                let new_portc = self.mem.data[0x28] ^ value;
                // Re-invoke write_data for PORTC so speaker side effects fire
                self.write_data(0x28, new_portc);
                return;
            }
            0x29 => {
                // PIND → toggles PORTD
                let new_portd = self.mem.data[0x2B] ^ value;
                // Re-invoke write_data for PORTD so all side effects fire
                self.write_data(0x2B, new_portd);
                return;
            }
            0x2C => {
                // PINE → toggles PORTE
                let new_porte = self.mem.data[0x2E] ^ value;
                self.write_data(0x2E, new_porte);
                return;
            }
            0x2F => {
                // PINF → toggles PORTF
                let new_portf = self.mem.data[0x31] ^ value;
                self.write_data(0x31, new_portf);
                return;
            }
            _ => {}
        }

        // GPIO DDR/PORT writes - track pin changes
        match addr {
            0x24 | 0x25 => {
                // DDRB, PORTB
                if a < self.mem.data.len() {
                    // Detect PB5 (speaker pin 2) transitions for GPIO-driven audio
                    if addr == 0x25 {
                        let new_pb5 = value & (1 << 5) != 0;
                        if new_pb5 != self.speaker2_prev_pb5 {
                            let tick = self.cpu.tick;
                            // Record edge in sample-accurate audio buffer
                            self.audio_buf.right.push(tick, new_pb5);
                            if self.speaker2_last_edge > 0 {
                                let half = tick.saturating_sub(self.speaker2_last_edge);
                                if half >= 400 && half <= 270000 {
                                    self.speaker2_half_period = half;
                                    self.speaker2_last_active = tick;
                                }
                            }
                            self.speaker2_last_edge = tick;
                            self.speaker2_prev_pb5 = new_pb5;
                        }
                    }
                    self.mem.data[a] = value;
                    // Track LED states from PORTB
                    // RX LED = PB0 (active-low)
                    self.led_rx = value & (1 << 0) == 0;
                    // RGB LED digital: Blue=PB5, Red=PB6, Green=PB7 (active-high)
                    self.led_rgb.2 = if value & (1 << 5) != 0 { 255 } else { 0 }; // Blue
                    self.led_rgb.0 = if value & (1 << 6) != 0 { 255 } else { 0 }; // Red
                    self.led_rgb.1 = if value & (1 << 7) != 0 { 255 } else { 0 };
                    // Green
                }
                return;
            }
            0x27 | 0x28 => {
                // DDRC, PORTC
                if a < self.mem.data.len() {
                    // Trace PORTC/DDRC writes for diagnostics
                    if self.spi_trace_enabled && self.spi_trace.len() < 200 {
                        let old = self.mem.data[a];
                        let reg_name = if addr == 0x28 { "PORTC" } else { "DDRC" };
                        self.spi_trace.push(format!(
                            "{}_WRITE old=0x{:02X} new=0x{:02X} PC=0x{:04X}",
                            reg_name, old, value, self.cpu.pc
                        ));
                    }
                    // Detect PC6 (speaker pin 1) transitions for GPIO-driven audio
                    if addr == 0x28 {
                        let new_pc6 = value & (1 << 6) != 0;
                        if new_pc6 != self.speaker_prev_pc6 {
                            let tick = self.cpu.tick;
                            // Record edge in sample-accurate audio buffer
                            self.audio_buf.left.push(tick, new_pc6);
                            if self.speaker_last_edge > 0 {
                                let half = tick.saturating_sub(self.speaker_last_edge);
                                // Valid audio range: ~30Hz to ~20kHz
                                // half-period: 16MHz/(2*20000)=400 to 16MHz/(2*30)=266666
                                if half >= 400 && half <= 270000 {
                                    self.speaker_half_period = half;
                                    self.speaker_last_active = tick;
                                }
                            }
                            self.speaker_last_edge = tick;
                            self.speaker_prev_pc6 = new_pc6;
                        }
                    }
                    self.mem.data[a] = value;
                }
                return;
            }
            0x2A => {
                // DDRD
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            0x2B => {
                // PORTD
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                // TX LED = PD5 (active-low)
                self.led_tx = value & (1 << 5) == 0;

                // Gamebuino Classic speaker: PD3 (Arduino D3)
                // Reuses speaker1 fields (PC6 is unused on 328P)
                if self.cpu_type == CpuType::Atmega328p {
                    let new_pd3 = value & (1 << 3) != 0;
                    if new_pd3 != self.speaker_prev_pc6 {
                        let tick = self.cpu.tick;
                        self.audio_buf.left.push(tick, new_pd3);
                        if self.speaker_last_edge > 0 {
                            let half = tick.saturating_sub(self.speaker_last_edge);
                            if half >= 400 && half <= 270000 {
                                self.speaker_half_period = half;
                                self.speaker_last_active = tick;
                            }
                        }
                        self.speaker_last_edge = tick;
                        self.speaker_prev_pc6 = new_pd3;
                    }
                }

                // FX Flash CS = PD1 (Arduino D2): detect rising edge (deselect)
                // Only when PD1 is configured as output (DDR check)
                if self.fx_flash.loaded && (self.mem.data[0x2A] & (1 << 1) != 0) {
                    let new_cs_high = value & (1 << 1) != 0;
                    if new_cs_high && !self.fx_cs_prev {
                        if self.debug && self.dbg_fx_cs_count < 20 {
                            eprintln!(
                                "  FX CS↑ (deselect) after {} SPI bytes, state={:?}",
                                self.dbg_fx_bytes_in_cs, self.fx_flash.state
                            );
                        }
                        self.fx_flash.deselect();
                        self.dbg_fx_cs_count += 1;
                    }
                    if !new_cs_high && self.fx_cs_prev {
                        // CS going LOW: start of new transaction
                        self.dbg_fx_bytes_in_cs = 0;
                        if self.debug && self.dbg_fx_cs_count < 20 {
                            eprintln!("  FX CS↓ (select) transaction #{}", self.dbg_fx_cs_count);
                        }
                    }
                    self.fx_cs_prev = new_cs_high;
                }
                return;
            }
            0x2D | 0x2E => {
                // DDRE, PORTE
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            0x30 | 0x31 => {
                // DDRF, PORTF
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            _ => {}
        }

        // SP writes
        match addr {
            SPH_ADDR => {
                self.cpu.sp = (self.cpu.sp & 0x00FF) | ((value as u16) << 8);
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            SPL_ADDR => {
                self.cpu.sp = (self.cpu.sp & 0xFF00) | value as u16;
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            SREG_ADDR => {
                self.cpu.sreg = value;
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
            _ => {}
        }

        // Watchdog control register
        if addr == WDTCSR_ADDR {
            self.watchdog
                .write_wdtcsr(value, self.cpu.tick, &mut self.mem.data);
            return;
        }
        // Timer0 writes
        if self.timer0.write(addr, value, old, &mut self.mem.data) {
            return;
        }
        // Timer1 writes
        if self.timer1.write(addr, value, old, &mut self.mem.data) {
            return;
        }
        // Timer3 writes
        if self.timer3.write(addr, value, old, &mut self.mem.data) {
            return;
        }
        // Timer4 writes (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            if self.timer4.write(addr, value) {
                if a < self.mem.data.len() {
                    self.mem.data[a] = value;
                }
                return;
            }
        }
        // Timer2 writes (ATmega328P only)
        if self.cpu_type == CpuType::Atmega328p {
            let was_pwm = self.timer2.is_pwm_dac_active();
            let old_ocr_b = self.timer2.ocr_b();
            if self.timer2.write(addr, value, old, &mut self.mem.data) {
                // PWM DAC audio: when Timer2 is in PWM mode with OC2B output
                // enabled, OCR2B changes represent audio samples. The Timer1
                // ISR updates OCR2B at ~57 kHz to produce waveforms via PWM.
                if self.timer2.is_pwm_dac_active() || was_pwm {
                    let new_ocr_b = self.timer2.ocr_b();
                    if new_ocr_b != old_ocr_b {
                        let tick = self.cpu.tick;
                        self.audio_buf.push_pwm_sample(tick, new_ocr_b);
                    }
                }

                return;
            }
        }

        // SPI writes
        if self.spi.write(addr, value) {
            // Store value in mem.data so reads return correct value
            if a < self.mem.data.len() {
                self.mem.data[a] = value;
            }
            // If SPDR written, data goes to SPI output with current DC state
            if addr == 0x4E {
                let portd = self.mem.data[0x2B];
                let portf = self.mem.data[0x31];
                let ddrd = self.mem.data[0x2A];

                // SPI bus is shared: both FX flash and display receive
                // every byte simultaneously, just like real hardware.
                // Each chip only acts on bytes when its own CS is LOW.

                // FX Flash CS = PD1 (Arduino D2, active LOW)
                let fx_cs_active = self.fx_flash.loaded
                    && (ddrd & (1 << 1) != 0)   // PD1 configured as output
                    && (portd & (1 << 1) == 0); // PD1 driven LOW

                // FX flash: transfer byte and capture MISO response
                if fx_cs_active {
                    let response = self.fx_flash.transfer(value);
                    self.spdr_in = response;
                    self.mem.data[0x4E] = response;
                    self.dbg_fx_transfers += 1;
                    self.dbg_fx_bytes_in_cs += 1;
                    if self.debug && self.dbg_fx_transfers <= 20 {
                        eprintln!(
                            "[FX-xfer] #{} MOSI=0x{:02X} MISO=0x{:02X} state={:?} PC=0x{:04X}",
                            self.dbg_fx_transfers,
                            value,
                            response,
                            self.fx_flash.state,
                            self.cpu.pc
                        );
                    }
                } else {
                    self.spdr_in = 0xFF;
                }

                // Display: always push to display SPI buffer.
                // flush_spi() checks the display's own CS (PD6 for SSD1306,
                // PF6 for PCD8544) and discards bytes when CS is HIGH.
                if self.debug
                    && (self.dbg_spdr_writes < 30
                        || (self.dbg_spdr_writes >= 85 && self.dbg_spdr_writes < 100)
                        || (self.dbg_spdr_writes >= 1024 && self.dbg_spdr_writes < 1040))
                {
                    eprintln!(
                        "  SPI#{:3} val=0x{:02X} PD4={} PD6={} PF5={} PF6={} FX_CS={}",
                        self.dbg_spdr_writes,
                        value,
                        (portd >> 4) & 1,
                        (portd >> 6) & 1,
                        (portf >> 5) & 1,
                        (portf >> 6) & 1,
                        if fx_cs_active { "LO" } else { "HI" }
                    );
                }
                let portc = self.mem.data[0x28];
                if self.spi_trace_enabled && self.spi_trace.len() < 200 {
                    let ddrc = self.mem.data[0x27];
                    let portb = self.mem.data[0x25];
                    let ddrb = self.mem.data[0x24];
                    let ddrd = self.mem.data[0x2A];
                    self.spi_trace.push(format!("SPDR val=0x{:02X} PC=0x{:04X} PORTB=0x{:02X}(DDR={:02X}) PORTC=0x{:02X}(DDR={:02X}) PORTD=0x{:02X}(DDR={:02X})",
                        value, self.cpu.pc, portb, ddrb, portc, ddrc, portd, ddrd));
                }
                self.spi_out.push((value, portd, portf, portc));
                self.dbg_spdr_writes += 1;
            }
            return;
        }

        // PLL write
        if addr == 0x49 {
            self.pll.write(value);
            if a < self.mem.data.len() {
                self.mem.data[a] = value;
            }
            return;
        }

        // EEPROM control write
        if addr == 0x3F {
            let ea = self.mem.data[0x41] as u16 | ((self.mem.data[0x42] as u16) << 8);
            if value & 0x02 != 0 {
                let data_val = self.mem.data[0x40];
                if (ea as usize) < self.mem.eeprom.len() {
                    self.mem.eeprom[ea as usize] = data_val;
                    self.eeprom_dirty = true;
                }
            }
            if a < self.mem.data.len() {
                self.mem.data[a] = value & !2;
            }
            return;
        }

        // ADC writes
        if self.adc.write(addr, value, &mut self.rng_state) {
            if a < self.mem.data.len() {
                self.mem.data[a] = value;
            }
            return;
        }

        // USB Serial registers (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            match addr {
            0xE9 => { // UENUM - endpoint select
                self.usb_uenum = value & 0x07;
                if a < self.mem.data.len() { self.mem.data[a] = value; }
                return;
            }
            0xF1 => { // UEDATX - write data to endpoint
                // Capture serial output from CDC endpoint (typically EP3)
                if self.usb_uenum >= 3 {
                    self.serial_buf.push(value);
                }
                return;
            }
            0xE8 => { // UEINTX - clear interrupt flags by writing 0
                if a < self.mem.data.len() { self.mem.data[a] = value; }
                return;
            }
            0xD8 => { // USBCON
                self.usb_configured = value & 0x80 != 0; // USBE bit
                if a < self.mem.data.len() { self.mem.data[a] = value; }
                return;
            }
            0xE3 => { // UDADDR
                if a < self.mem.data.len() { self.mem.data[a] = value | 0x80; } // ADDEN always set
                return;
            }
            0xE1 | 0xE2 | // UDINT, UDIEN
            0xEA | // UERST
            0xEB | // UECONX
            0xEC | // UECFG0X
            0xED | // UECFG1X
            0xF0   // UEIENX
            => {
                if a < self.mem.data.len() { self.mem.data[a] = value; }
                return;
            }
            _ => {}
            }
        }

        // USART0 registers (ATmega328P only)
        if self.cpu_type == CpuType::Atmega328p {
            match addr {
            0xC0 => { // UCSR0A — writing TXC0 bit clears it
                if a < self.mem.data.len() {
                    let cleared = self.mem.data[a] & !(value & 0x40);
                    self.mem.data[a] = cleared;
                }
                return;
            }
            0xC1 | // UCSR0B — control (TXEN, RXEN, interrupts)
            0xC2 | // UCSR0C — frame format
            0xC4 | // UBRR0L — baud rate low
            0xC5   // UBRR0H — baud rate high
            => {
                if a < self.mem.data.len() { self.mem.data[a] = value; }
                return;
            }
            0xC6 => { // UDR0 — transmit data
                // Capture serial output if TXEN0 is set (bit 3 of UCSR0B)
                let ucsr0b = self.mem.data[0xC1];
                if ucsr0b & (1 << 3) != 0 {
                    self.serial_buf.push(value);
                    if self.debug {
                        let ch = if value >= 0x20 && value < 0x7F {
                            value as char
                        } else { '.' };
                        eprint!("{}", ch);
                    }
                }
                // Set TXC0 and UDRE0 in UCSR0A
                if a < self.mem.data.len() {
                    self.mem.data[0xC0] |= 0x60; // UDRE0 + TXC0
                }
                return;
            }
            _ => {}
            }
        }

        // Default write
        if a < self.mem.data.len() {
            self.mem.data[a] = value;
        }
    }

    /// Write a bit in data space
    pub fn write_bit(&mut self, addr: u16, bit: u8, bvalue: bool) {
        // addr is already in data space (decoder adds 0x20 for CBI/SBI)
        let val = self.read_data(addr);
        let new_val = if bvalue {
            val | (1 << bit)
        } else {
            val & !(1 << bit)
        };
        self.write_data(addr, new_val);
    }

    /// Flush SPI output to display
    fn flush_spi(&mut self) {
        let bytes: Vec<(u8, u8, u8, u8)> = self.spi_out.drain(..).collect();
        for (byte, portd, portf, portc) in bytes {
            // Decode DC and CS based on display type and CPU
            // Arduboy (32u4):           DC=PD4(bit4), CS=PD6(bit6) - active LOW
            // Gamebuino (32u4 PCD8544): DC=PF5(bit5), CS=PF6(bit6) - active LOW
            // Gamebuino Classic (328P): DC=PC2(bit2), CS=PC1(bit1) - active LOW (defaults)
            //   The Gamebuino library allows configurable pins; auto-detected at runtime.
            let (is_data, cs_high) = if self.cpu_type == CpuType::Atmega328p {
                if self.pcd_cs_bit == 0xFF {
                    // Auto-detect: look for PCD8544 init commands with PORTC bits LOW
                    // Standard Gamebuino Classic: CS=PC1, DC=PC2
                    // When sending the first command (0x21 = extended mode), both CS and
                    // DC are LOW. Scan PORTC for exactly 2 LOW bits driven as outputs.
                    let ddrc = self.mem.data[0x27];
                    let low_out_bits: Vec<u8> = (0..6)
                        .filter(|&b| ddrc & (1 << b) != 0 && portc & (1 << b) == 0)
                        .collect();
                    if self.debug && self.dbg_spdr_writes < 20 {
                        eprintln!("[PCD-detect] SPI#{} val=0x{:02X} DDRC=0x{:02X} PORTC=0x{:02X} low_out={:?}",
                            self.dbg_spdr_writes, byte, ddrc, portc, low_out_bits);
                    }
                    if low_out_bits.len() >= 2 && (byte == 0x21 || byte == 0x20) {
                        // Heuristic: lowest bit is DC, next is CS (matches standard layout)
                        self.pcd_dc_bit = low_out_bits[0];
                        self.pcd_cs_bit = low_out_bits[1];
                        self.display_type = DisplayType::Pcd8544;
                        if self.debug {
                            eprintln!("PCD8544 auto-detected: CS=PC{}, DC=PC{} (cmd=0x{:02X}, PORTC=0x{:02X}, DDRC=0x{:02X})",
                                self.pcd_cs_bit, self.pcd_dc_bit, byte, portc, ddrc);
                        }
                        (false, false) // is_data=false (command), cs_high=false (selected)
                    } else {
                        (true, true) // not yet detected → skip this byte
                    }
                } else {
                    let is_d = portc & (1 << self.pcd_dc_bit) != 0;
                    let cs_h = portc & (1 << self.pcd_cs_bit) != 0;
                    if self.debug && self.pcd8544.dbg_cmd_count + self.pcd8544.dbg_data_count < 10 {
                        eprintln!(
                            "[PCD] val=0x{:02X} PORTC=0x{:02X} dc={} cs_hi={}",
                            byte, portc, is_d, cs_h
                        );
                    }
                    (is_d, cs_h)
                }
            } else {
                match self.display_type {
                    DisplayType::Ssd1306 => (portd & (1 << 4) != 0, portd & (1 << 6) != 0),
                    DisplayType::Pcd8544 => (portf & (1 << 5) != 0, portf & (1 << 6) != 0),
                    DisplayType::Unknown => {
                        let ardu_cs_active = portd & (1 << 6) == 0;
                        let ardu_dc_cmd = portd & (1 << 4) == 0;
                        let gb_cs_active = portf & (1 << 6) == 0;
                        let gb_dc_cmd = portf & (1 << 5) == 0;

                        if self.debug && self.dbg_spdr_writes < 30 {
                            eprintln!(
                                "  DETECT: val=0x{:02X} ardu(cs={} dc_cmd={}) gb(cs={} dc_cmd={})",
                                byte, ardu_cs_active, ardu_dc_cmd, gb_cs_active, gb_dc_cmd
                            );
                        }

                        if ardu_cs_active && ardu_dc_cmd {
                            if byte >= 0x80 {
                                self.display_type = DisplayType::Ssd1306;
                                if self.debug {
                                    eprintln!("Display auto-detected: SSD1306 (first cmd: 0x{:02X}, PD4=0 PD6=0)", byte);
                                }
                            }
                        }
                        if self.display_type == DisplayType::Unknown && gb_cs_active && gb_dc_cmd {
                            if byte == 0x21 || byte == 0x20 {
                                self.display_type = DisplayType::Pcd8544;
                                if self.debug {
                                    eprintln!("Display auto-detected: PCD8544 (first cmd: 0x{:02X}, PF5=0 PF6=0)", byte);
                                }
                            }
                        }

                        match self.display_type {
                            DisplayType::Pcd8544 => (portf & (1 << 5) != 0, portf & (1 << 6) != 0),
                            _ => (portd & (1 << 4) != 0, portd & (1 << 6) != 0),
                        }
                    }
                }
            };

            // Skip SPI bytes when display CS is HIGH (not selected)
            if cs_high {
                if self.spi_trace_enabled && self.spi_trace.len() < 200 {
                    self.spi_trace.push(format!(
                        "SKIP val=0x{:02X} PORTC=0x{:02X} cs_high=true",
                        byte, portc
                    ));
                }
                continue;
            }

            if self.spi_trace_enabled && self.spi_trace.len() < 200 {
                self.spi_trace.push(format!(
                    "{} val=0x{:02X} PORTC=0x{:02X} dc_bit={} cs_bit={}",
                    if is_data { "DATA" } else { "CMD " },
                    byte,
                    portc,
                    self.pcd_dc_bit,
                    self.pcd_cs_bit
                ));
            }

            match self.display_type {
                DisplayType::Pcd8544 => {
                    if is_data {
                        self.pcd8544.receive_data(byte);
                    } else {
                        self.pcd8544.receive_command(byte);
                    }
                }
                _ => {
                    if is_data {
                        self.display.receive_data(byte);
                    } else {
                        self.display.receive_command(byte);
                    }
                }
            }
        }
        if self.display_type == DisplayType::Pcd8544 {
            self.pcd8544.render_to_framebuffer();
        }
    }

    /// Update all peripherals and handle interrupts
    fn update_peripherals(&mut self) {
        let ie = self.cpu.sreg & (1 << SREG_I) != 0;
        let tick = self.cpu.tick;

        // Flush SPI to display
        self.flush_spi();

        // Timer0
        self.timer0.update(tick, &mut self.mem.data);
        if ie {
            if let Some(vec_addr) = self.timer0.check_interrupt() {
                self.cpu.sleeping = false;
                self.do_interrupt(vec_addr);
                return;
            }
        }

        // Timer1
        self.timer1.update(tick, &mut self.mem.data);
        if ie {
            if let Some(vec_addr) = self.timer1.check_interrupt() {
                self.cpu.sleeping = false;
                self.do_interrupt(vec_addr);
                return;
            }
        }

        // Timer3 (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            self.timer3.update(tick, &mut self.mem.data);
            if ie {
                if let Some(vec_addr) = self.timer3.check_interrupt() {
                    self.cpu.sleeping = false;
                    self.do_interrupt(vec_addr);
                    return;
                }
            }
        }

        // Timer4 (ATmega32u4 only)
        if self.cpu_type == CpuType::Atmega32u4 {
            self.timer4.update(tick, &mut self.mem.data);
            if ie {
                if let Some(vec_addr) = self.timer4.check_interrupt() {
                    self.cpu.sleeping = false;
                    self.do_interrupt(vec_addr);
                    return;
                }
            }
        }

        // Timer2 (ATmega328P only)
        if self.cpu_type == CpuType::Atmega328p {
            self.timer2.update(tick, &mut self.mem.data);
            if ie {
                if let Some(vec_addr) = self.timer2.check_interrupt() {
                    self.cpu.sleeping = false;
                    self.do_interrupt(vec_addr);
                    return;
                }
            }
        }

        // SPI
        if ie {
            if let Some(vec_addr) = self.spi.check_interrupt() {
                self.cpu.sleeping = false;
                self.do_interrupt(vec_addr);
                return;
            }
        }

        // USART0 interrupts (328P only — 32u4 uses USB serial)
        if ie && self.cpu_type == CpuType::Atmega328p {
            let ucsr0a = self.mem.data[0xC0];
            let ucsr0b = self.mem.data[0xC1];
            // UDRE interrupt: UDRIE0(bit5) && UDRE0(bit5)
            if (ucsr0b & 0x20 != 0) && (ucsr0a & 0x20 != 0) {
                self.cpu.sleeping = false;
                self.do_interrupt(peripherals::INT_328P_USART_UDRE);
                return;
            }
            // TX Complete interrupt: TXCIE0(bit6) && TXC0(bit6)
            if (ucsr0b & 0x40 != 0) && (ucsr0a & 0x40 != 0) {
                self.cpu.sleeping = false;
                // TXC0 is auto-cleared when executing the interrupt
                self.mem.data[0xC0] &= !0x40;
                self.do_interrupt(peripherals::INT_328P_USART_TX);
                return;
            }
            // RX Complete interrupt: RXCIE0(bit7) && RXC0(bit7)
            if (ucsr0b & 0x80 != 0) && (ucsr0a & 0x80 != 0) {
                self.cpu.sleeping = false;
                self.do_interrupt(peripherals::INT_328P_USART_RX);
                return;
            }
        }

        // ADC
        self.adc.update(&mut self.rng_state);
        if ie {
            if let Some(vec_addr) = self.adc.check_interrupt() {
                self.cpu.sleeping = false;
                self.do_interrupt(vec_addr);
                return;
            }
        }

        // Watchdog
        match self.watchdog.update(tick, &mut self.mem.data) {
            peripherals::WatchdogEvent::Reset => {
                // Watchdog system reset: restart the MCU and latch WDRF so the
                // sketch can tell the reset came from the watchdog.
                self.reset();
                self.mem.data[MCUSR_ADDR as usize] |= 1 << WDRF_BIT;
                self.did_reset = true;
                return;
            }
            peripherals::WatchdogEvent::None => {}
        }
        if let Some(vec_addr) = self.watchdog.take_interrupt(ie) {
            self.cpu.sleeping = false;
            self.do_interrupt(vec_addr);
        }
    }

    /// Execute an interrupt: push PC, jump to vector
    fn do_interrupt(&mut self, vector: u16) {
        let pc = self.cpu.pc;
        // Push return address (same order as push_word/CALL)
        self.mem.data[self.cpu.sp as usize] = (pc >> 8) as u8;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.mem.data[self.cpu.sp as usize] = pc as u8;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        // Sync SP to memory registers
        self.mem.data[SPH_ADDR as usize] = (self.cpu.sp >> 8) as u8;
        self.mem.data[SPL_ADDR as usize] = self.cpu.sp as u8;
        // Disable interrupts
        self.cpu.sreg &= !(1 << SREG_I);
        self.mem.data[SREG_ADDR as usize] = self.cpu.sreg;
        self.cpu.pc = vector;
        self.cpu.tick += 5;
    }

    /// Get display pixel buffer as RGBA u32 slice (for minifb etc)
    pub fn framebuffer_u32(&self) -> Vec<u32> {
        match self.display_type {
            DisplayType::Pcd8544 => {
                let fb = &self.pcd8544.framebuffer;
                let mut buf = Vec::with_capacity(SCREEN_WIDTH * SCREEN_HEIGHT);
                for i in 0..(SCREEN_WIDTH * SCREEN_HEIGHT) {
                    let offset = i * 4;
                    let r = fb[offset] as u32;
                    let g = fb[offset + 1] as u32;
                    let b = fb[offset + 2] as u32;
                    buf.push((r << 16) | (g << 8) | b);
                }
                buf
            }
            _ => self.display.as_pixel_buffer(),
        }
    }

    /// Get display framebuffer RGBA bytes
    pub fn framebuffer_rgba(&self) -> &[u8] {
        match self.display_type {
            DisplayType::Pcd8544 => &self.pcd8544.framebuffer,
            _ => &self.display.framebuffer,
        }
    }

    /// Simple xorshift PRNG
    pub fn next_random(&mut self) -> u8 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        (self.rng_state & 0xFF) as u8
    }

    /// Get current tone frequencies for stereo audio output.
    ///
    /// Returns `(left_hz, right_hz)`:
    /// - Left channel: Timer3 CTC tone → Timer4 CTC tone → GPIO PC6 bit-bang (Speaker 1)
    /// - Right channel: Timer1 CTC tone → GPIO PB5 bit-bang (Speaker 2)
    ///
    /// Priority within each channel: hardware timer > GPIO bit-bang.
    pub fn get_audio_tone(&self) -> (f32, f32) {
        let t1 = self.timer1.get_tone_hz(CLOCK_HZ);

        // Timer3/Timer4 only on 32u4
        let t3 = if self.cpu_type == CpuType::Atmega32u4 {
            self.timer3.get_tone_hz(CLOCK_HZ)
        } else {
            0.0
        };
        let t4 = if self.cpu_type == CpuType::Atmega32u4 {
            self.timer4.get_tone_hz(CLOCK_HZ)
        } else {
            0.0
        };

        // Timer2 only on 328P (Gamebuino sound)
        let t2 = if self.cpu_type == CpuType::Atmega328p {
            self.timer2.get_tone_hz(CLOCK_HZ)
        } else {
            0.0
        };

        // GPIO bit-bang speaker 1: derive frequency from toggle rate
        // ATmega32u4: PC6 (Arduboy), ATmega328P: PD3 (Gamebuino Classic)
        let gpio1_hz = if self.speaker_half_period > 0 {
            let age = self.cpu.tick.saturating_sub(self.speaker_last_active);
            if age < 250_000 {
                CLOCK_HZ as f32 / (2.0 * self.speaker_half_period as f32)
            } else {
                0.0
            }
        } else {
            0.0
        };

        // GPIO bit-bang speaker 2 (PB5): derive frequency from toggle rate
        let gpio2_hz = if self.speaker2_half_period > 0 {
            let age = self.cpu.tick.saturating_sub(self.speaker2_last_active);
            if age < 250_000 {
                CLOCK_HZ as f32 / (2.0 * self.speaker2_half_period as f32)
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Left: Timer3 > Timer4 > Timer2 > GPIO speaker 1 (PC6 on 32u4, PD3 on 328P)
        let left = if t3 > 0.0 {
            t3
        } else if t4 > 0.0 {
            t4
        } else if t2 > 0.0 {
            t2
        } else {
            gpio1_hz
        };
        // Right: Timer1 > GPIO PB5
        let right = if t1 > 0.0 { t1 } else { gpio2_hz };

        (left, right)
    }

    /// Save current state as a snapshot (for rewind).
    pub fn save_snapshot(&self) -> snapshot::Snapshot {
        let fb = match self.display_type {
            DisplayType::Pcd8544 => self.pcd8544.framebuffer.clone(),
            _ => self.display.framebuffer.clone(),
        };
        snapshot::Snapshot {
            pc: self.cpu.pc,
            sp: self.cpu.sp,
            sreg: self.cpu.sreg,
            tick: self.cpu.tick,
            sleeping: self.cpu.sleeping,
            data: self.mem.data.clone(),
            eeprom: self.mem.eeprom.clone(),
            framebuffer: fb.to_vec(),
            frame: self.frame_count,
        }
    }

    /// Restore state from a snapshot (rewind).
    pub fn restore_snapshot(&mut self, snap: &snapshot::Snapshot) {
        self.cpu.pc = snap.pc;
        self.cpu.sp = snap.sp;
        self.cpu.sreg = snap.sreg;
        self.cpu.tick = snap.tick;
        self.cpu.sleeping = snap.sleeping;
        let len = snap.data.len().min(self.mem.data.len());
        self.mem.data[..len].copy_from_slice(&snap.data[..len]);
        let elen = snap.eeprom.len().min(self.mem.eeprom.len());
        self.mem.eeprom[..elen].copy_from_slice(&snap.eeprom[..elen]);
        match self.display_type {
            DisplayType::Pcd8544 => {
                let flen = snap.framebuffer.len().min(self.pcd8544.framebuffer.len());
                self.pcd8544.framebuffer[..flen].copy_from_slice(&snap.framebuffer[..flen]);
            }
            _ => {
                let flen = snap.framebuffer.len().min(self.display.framebuffer.len());
                self.display.framebuffer[..flen].copy_from_slice(&snap.framebuffer[..flen]);
            }
        }
        self.frame_count = snap.frame;
    }

    /// Load flash from an ELF file, returning parsed debug info.
    pub fn load_elf(&mut self, data: &[u8]) -> Result<elf::ElfFile, String> {
        let elf = elf::parse_elf(data)?;
        let flash_len = elf.flash.len().min(self.mem.flash.len());
        self.mem.flash[..flash_len].copy_from_slice(&elf.flash[..flash_len]);
        self.reset();
        Ok(elf)
    }

    // ─── Save state (quick save / quick load) ──────────────────────────────

    /// CPU type as a byte for save state header.
    pub fn cpu_type_byte(&self) -> u8 {
        match self.cpu_type {
            CpuType::Atmega32u4 => 0,
            CpuType::Atmega328p => 1,
        }
    }

    /// Capture the full emulator state for save state.
    pub fn save_full_state(&self) -> savestate::SaveState {
        savestate::SaveState {
            // CPU
            pc: self.cpu.pc,
            sp: self.cpu.sp,
            sreg: self.cpu.sreg,
            tick: self.cpu.tick,
            sleeping: self.cpu.sleeping,

            // Memory
            data: self.mem.data.clone(),
            eeprom: self.mem.eeprom.clone(),

            // Display
            display: self.display.save_state(),
            pcd8544: self.pcd8544.save_state(),
            display_type: match self.display_type {
                DisplayType::Unknown => 0,
                DisplayType::Ssd1306 => 1,
                DisplayType::Pcd8544 => 2,
            },

            // Timers
            timer0: self.timer0.save_state(),
            timer1: self.timer1.save_state(),
            timer2: self.timer2.save_state(),
            timer3: self.timer3.save_state(),
            timer4: self.timer4.save_state(),

            // Peripherals
            spi: self.spi.save_state(),
            adc: self.adc.save_state(),
            pll: self.pll.save_state(),
            fx_flash: self.fx_flash.save_state(),

            // GPIO
            pin_b: self.pin_b,
            pin_c: self.pin_c,
            pin_d: self.pin_d,
            pin_e: self.pin_e,
            pin_f: self.pin_f,

            // Misc
            spdr_in: self.spdr_in,
            rng_state: self.rng_state,
            frame_count: self.frame_count,
            fx_cs_prev: self.fx_cs_prev,
            pcd_cs_bit: self.pcd_cs_bit,
            pcd_dc_bit: self.pcd_dc_bit,
            speaker_prev_pc6: self.speaker_prev_pc6,
            speaker_last_edge: self.speaker_last_edge,
            speaker_half_period: self.speaker_half_period,
            speaker_last_active: self.speaker_last_active,
            speaker2_prev_pb5: self.speaker2_prev_pb5,
            speaker2_last_edge: self.speaker2_last_edge,
            speaker2_half_period: self.speaker2_half_period,
            speaker2_last_active: self.speaker2_last_active,
            usb_uenum: self.usb_uenum,
            usb_configured: self.usb_configured,
            led_rgb: self.led_rgb,
            led_tx: self.led_tx,
            led_rx: self.led_rx,
            audio_left_level: self.audio_buf.left.level,
            audio_right_level: self.audio_buf.right.level,
        }
    }

    /// Restore the full emulator state from a save state.
    /// Clears the rewind buffer state (caller should also clear external RewindBuffer).
    pub fn load_full_state(&mut self, s: &savestate::SaveState) {
        // CPU
        self.cpu.pc = s.pc;
        self.cpu.sp = s.sp;
        self.cpu.sreg = s.sreg;
        self.cpu.tick = s.tick;
        self.cpu.sleeping = s.sleeping;

        // Memory
        let len = s.data.len().min(self.mem.data.len());
        self.mem.data[..len].copy_from_slice(&s.data[..len]);
        let elen = s.eeprom.len().min(self.mem.eeprom.len());
        self.mem.eeprom[..elen].copy_from_slice(&s.eeprom[..elen]);

        // Display
        self.display.load_state(&s.display);
        self.pcd8544.load_state(&s.pcd8544);
        self.display_type = match s.display_type {
            1 => DisplayType::Ssd1306,
            2 => DisplayType::Pcd8544,
            _ => DisplayType::Unknown,
        };

        // Timers
        self.timer0.load_state(&s.timer0);
        self.timer1.load_state(&s.timer1);
        self.timer2.load_state(&s.timer2);
        self.timer3.load_state(&s.timer3);
        self.timer4.load_state(&s.timer4);

        // Peripherals
        self.spi.load_state(&s.spi);
        self.adc.load_state(&s.adc);
        self.pll.load_state(&s.pll);
        self.fx_flash.load_state(savestate::FxFlashState {
            data: s.fx_flash.data.clone(),
            loaded: s.fx_flash.loaded,
            write_enabled: s.fx_flash.write_enabled,
            powered_down: s.fx_flash.powered_down,
        });

        // GPIO
        self.pin_b = s.pin_b;
        self.pin_c = s.pin_c;
        self.pin_d = s.pin_d;
        self.pin_e = s.pin_e;
        self.pin_f = s.pin_f;

        // Misc
        self.spdr_in = s.spdr_in;
        self.rng_state = s.rng_state;
        self.frame_count = s.frame_count;
        self.fx_cs_prev = s.fx_cs_prev;
        self.pcd_cs_bit = s.pcd_cs_bit;
        self.pcd_dc_bit = s.pcd_dc_bit;
        self.speaker_prev_pc6 = s.speaker_prev_pc6;
        self.speaker_last_edge = s.speaker_last_edge;
        self.speaker_half_period = s.speaker_half_period;
        self.speaker_last_active = s.speaker_last_active;
        self.speaker2_prev_pb5 = s.speaker2_prev_pb5;
        self.speaker2_last_edge = s.speaker2_last_edge;
        self.speaker2_half_period = s.speaker2_half_period;
        self.speaker2_last_active = s.speaker2_last_active;
        self.usb_uenum = s.usb_uenum;
        self.usb_configured = s.usb_configured;
        self.led_rgb = s.led_rgb;
        self.led_tx = s.led_tx;
        self.led_rx = s.led_rx;
        self.audio_buf.left.level = s.audio_left_level;
        self.audio_buf.right.level = s.audio_right_level;

        // Clear transient state
        self.spi_out.clear();
        self.serial_buf.clear();
        self.breakpoint_hit = false;
        self.eeprom_dirty = false;
    }
}

impl Default for Arduboy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arduboy_creation() {
        let ard = Arduboy::new();
        assert_eq!(ard.cpu.pc, 0);
        assert_eq!(ard.cpu.sp, (DATA_SIZE - 1) as u16);
        assert_eq!(ard.cpu_type, CpuType::Atmega32u4);
    }

    #[test]
    fn test_328p_creation() {
        let ard = Arduboy::new_with_cpu(CpuType::Atmega328p);
        assert_eq!(ard.cpu.pc, 0);
        assert_eq!(ard.cpu.sp, (DATA_SIZE_328P - 1) as u16);
        assert_eq!(ard.cpu_type, CpuType::Atmega328p);
        // 328P defaults to PCD8544 with Gamebuino Classic pin mapping
        assert_eq!(ard.display_type, DisplayType::Pcd8544);
        assert_eq!(ard.pcd_cs_bit, 1); // PC1 = A1
        assert_eq!(ard.pcd_dc_bit, 2); // PC2 = A2 = D16
    }

    #[test]
    fn test_detect_cpu_32u4() {
        // Simulate ATmega32u4 vector table: JMP instructions at 0x00..0xA8
        let mut flash = vec![0u8; 32768];
        // Fill all 43 vectors (0x00..0xA8, step 4) with JMP 0x0000
        // JMP encoding: 0x940C 0x0000 (little-endian: 0C 94 00 00)
        for addr in (0..=0xA8).step_by(4) {
            flash[addr] = 0x0C;
            flash[addr + 1] = 0x94;
            flash[addr + 2] = 0x00;
            flash[addr + 3] = 0x00;
        }
        assert_eq!(detect_cpu_type(&flash), CpuType::Atmega32u4);
    }

    #[test]
    fn test_detect_cpu_328p() {
        // Simulate ATmega328P: JMP for vectors 0..25 (0x00..0x64), then code
        let mut flash = vec![0u8; 32768];
        for addr in (0..=0x64).step_by(4) {
            flash[addr] = 0x0C;
            flash[addr + 1] = 0x94;
            flash[addr + 2] = 0x00;
            flash[addr + 3] = 0x00;
        }
        // 0x68+ is regular code (not JMP/RJMP) — e.g. LDI, MOV, etc.
        for addr in (0x68..0xB0).step_by(2) {
            flash[addr] = 0x0F;
            flash[addr + 1] = 0xEF; // LDI r16, 0xFF
        }
        assert_eq!(detect_cpu_type(&flash), CpuType::Atmega328p);
    }

    #[test]
    fn test_watchdog_system_reset() {
        let mut ard = Arduboy::new();
        // Sentinel in RAM and a non-zero PC to prove a real reset happened.
        let marker = 0x200usize;
        ard.mem.data[marker] = 0xAB;
        ard.cpu.pc = 0x40;
        // wdt_enable(WDTO_15MS): WDCE|WDE arm, then WDE with WDP=0 (~256k cycles).
        ard.write_data(WDTCSR_ADDR, (1 << 4) | (1 << 3));
        ard.write_data(WDTCSR_ADDR, 1 << 3);
        // Flash is all-NOP, so the dog is never pet. Two frames (~432k cycles)
        // comfortably exceed the ~256k-cycle timeout.
        ard.run_frame();
        ard.run_frame();
        // A watchdog system reset clears RAM, latches WDRF, and restarts at 0.
        assert_eq!(ard.mem.data[marker], 0x00);
        assert_eq!(
            ard.mem.data[MCUSR_ADDR as usize] & (1 << WDRF_BIT),
            1 << WDRF_BIT
        );
        assert_eq!(ard.cpu.pc, 0);
    }

    #[test]
    fn test_button_press() {
        let mut ard = Arduboy::new();
        assert_eq!(ard.pin_f & (1 << 7), 1 << 7); // UP released
        ard.set_button(Button::Up, true);
        assert_eq!(ard.pin_f & (1 << 7), 0); // UP pressed (active low)
        ard.set_button(Button::Up, false);
        assert_eq!(ard.pin_f & (1 << 7), 1 << 7); // UP released
    }

    #[test]
    fn test_328p_button_press() {
        let mut ard = Arduboy::new_with_cpu(CpuType::Atmega328p);
        // 328P Gamebuino: UP=PB1
        assert_eq!(ard.pin_b & (1 << 1), 1 << 1);
        ard.set_button(Button::Up, true);
        assert_eq!(ard.pin_b & (1 << 1), 0);
        ard.set_button(Button::Up, false);
        assert_eq!(ard.pin_b & (1 << 1), 1 << 1);
    }

    #[test]
    fn test_load_hex() {
        let mut ard = Arduboy::new();
        let hex = ":100000000C9434000C944E000C944E000C944E0052\n:00000001FF\n";
        let result = ard.load_hex(hex);
        assert!(result.is_ok());
        assert_eq!(ard.mem.flash[0], 0x0C);
        assert_eq!(ard.mem.flash[1], 0x94);
    }

    /// Diagnostic test: loads a Gamebuino Classic HEX and runs frames,
    /// printing detailed SPI/display state to find black screen causes.
    /// Run with: cargo test test_328p_display_diag -- --nocapture
    #[test]
    fn test_328p_display_diag() {
        let mut ard = Arduboy::new_with_cpu(CpuType::Atmega328p);

        // Try to load 3D-DEMO.HEX from known locations
        // cargo test runs from workspace root or crate root
        let hex_paths = [
            "test_roms/3D-DEMO.HEX",
            "crates/core/test_roms/3D-DEMO.HEX",
            "../test_roms/3D-DEMO.HEX",
            "../../test_roms/3D-DEMO.HEX",
            "3D-DEMO.HEX",
        ];
        let mut loaded = false;
        for path in &hex_paths {
            if let Ok(hex_str) = std::fs::read_to_string(path) {
                match ard.load_hex(&hex_str) {
                    Ok(size) => {
                        println!("[DIAG] Loaded {} ({} bytes)", path, size);
                        loaded = true;
                        break;
                    }
                    Err(e) => println!("[DIAG] Failed to load {}: {}", path, e),
                }
            }
        }
        if !loaded {
            println!("[DIAG] No HEX file found, running with empty flash.");
            println!("[DIAG] Place 3D-DEMO.HEX in test_roms/ or project root to test.");
            return;
        }

        // Check initial state
        println!(
            "[DIAG] display_type={:?} pcd_cs_bit={} pcd_dc_bit={}",
            ard.display_type, ard.pcd_cs_bit, ard.pcd_dc_bit
        );
        println!(
            "[DIAG] Reset vector: flash[0..4] = {:02X} {:02X} {:02X} {:02X}",
            ard.mem.flash[0], ard.mem.flash[1], ard.mem.flash[2], ard.mem.flash[3]
        );

        // Enable SPI byte trace
        ard.spi_trace_enabled = true;

        // Run frames and collect diagnostics
        for frame in 1..=2 {
            let spi_before = ard.dbg_spdr_writes;
            ard.run_frame();

            let spi_count = ard.dbg_spdr_writes - spi_before;
            let ddrc = ard.mem.data[0x27];
            let portc = ard.mem.data[0x28];
            let spcr = ard.mem.data[0x4C];

            println!(
                "[DIAG] Frame {}: pc=0x{:04X} SPI_this_frame={} SPI_total={} \
                     DDRC=0x{:02X} PORTC=0x{:02X} SPCR=0x{:02X} sleeping={}",
                frame,
                ard.cpu.pc,
                spi_count,
                ard.dbg_spdr_writes,
                ddrc,
                portc,
                spcr,
                ard.cpu.sleeping
            );
            println!(
                "[DIAG]   pcd_cmd={} pcd_data={} display_mode={} display_type={:?}",
                ard.pcd8544.dbg_cmd_count,
                ard.pcd8544.dbg_data_count,
                ard.pcd8544.display_mode,
                ard.display_type
            );
            println!(
                "[DIAG]   vram[0..8]={:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                ard.pcd8544.vram[0],
                ard.pcd8544.vram[1],
                ard.pcd8544.vram[2],
                ard.pcd8544.vram[3],
                ard.pcd8544.vram[4],
                ard.pcd8544.vram[5],
                ard.pcd8544.vram[6],
                ard.pcd8544.vram[7]
            );

            // Check PORTC bit states for DC/CS pins
            let dc_val = (portc >> ard.pcd_dc_bit) & 1;
            let cs_val = (portc >> ard.pcd_cs_bit) & 1;
            println!(
                "[DIAG]   DC(PC{})={} CS(PC{})={}",
                ard.pcd_dc_bit, dc_val, ard.pcd_cs_bit, cs_val
            );
        }

        // Final state summary
        let vram_nonzero = ard.pcd8544.vram.iter().filter(|&&b| b != 0).count();
        let fb_nonzero = ard
            .pcd8544
            .framebuffer
            .chunks(4)
            .filter(|px| px[0] != 0 || px[1] != 0 || px[2] != 0)
            .count();
        println!("\n[DIAG] === SUMMARY ===");
        println!("[DIAG] Total SPI writes: {}", ard.dbg_spdr_writes);
        println!("[DIAG] PCD8544 commands: {}", ard.pcd8544.dbg_cmd_count);
        println!("[DIAG] PCD8544 data bytes: {}", ard.pcd8544.dbg_data_count);
        println!(
            "[DIAG] PCD8544 display_mode: {} (0=blank, 4=normal, 5=inverse)",
            ard.pcd8544.display_mode
        );
        println!(
            "[DIAG] VRAM non-zero bytes: {} / {}",
            vram_nonzero,
            ard.pcd8544.vram.len()
        );
        println!("[DIAG] Framebuffer lit pixels: {}", fb_nonzero);
        println!("[DIAG] display_type: {:?}", ard.display_type);

        if ard.dbg_spdr_writes == 0 {
            println!("[DIAG] *** NO SPI WRITES AT ALL - game may be stuck before SPI init ***");
            // Dump current PC vicinity
            let pc = ard.cpu.pc as usize * 2;
            if pc + 3 < ard.mem.flash.len() {
                println!(
                    "[DIAG] PC vicinity: flash[0x{:04X}] = {:02X} {:02X} {:02X} {:02X}",
                    pc,
                    ard.mem.flash[pc],
                    ard.mem.flash[pc + 1],
                    ard.mem.flash[pc + 2],
                    ard.mem.flash[pc + 3]
                );
            }
        }
        if ard.pcd8544.dbg_cmd_count == 0 && ard.dbg_spdr_writes > 0 {
            println!("[DIAG] *** SPI writes happened but no PCD8544 commands received ***");
            println!("[DIAG] *** CS/DC routing problem! Bytes are being discarded ***");
        }
        if ard.pcd8544.dbg_data_count == 0 && ard.pcd8544.dbg_cmd_count > 0 {
            println!("[DIAG] *** Commands received but no data - display_mode or cursor issue ***");
        }
        if vram_nonzero > 0 && fb_nonzero == 0 {
            println!(
                "[DIAG] *** VRAM has data but framebuffer empty - render_to_framebuffer issue ***"
            );
        }

        // Dump SPI trace
        let portc_writes = ard
            .spi_trace
            .iter()
            .filter(|s| s.contains("PORTC_WRITE"))
            .count();
        let ddrc_writes = ard
            .spi_trace
            .iter()
            .filter(|s| s.contains("DDRC_WRITE"))
            .count();
        let spdr_writes_in_trace = ard
            .spi_trace
            .iter()
            .filter(|s| s.starts_with("SPDR"))
            .count();
        let skip_count = ard
            .spi_trace
            .iter()
            .filter(|s| s.starts_with("SKIP"))
            .count();
        let cmd_count = ard
            .spi_trace
            .iter()
            .filter(|s| s.starts_with("CMD"))
            .count();
        let data_count = ard
            .spi_trace
            .iter()
            .filter(|s| s.starts_with("DATA"))
            .count();

        println!("\n[DIAG] === TRACE SUMMARY (v2) ===");
        println!(
            "[DIAG] trace_entries={} PORTC_WRITE={} DDRC_WRITE={} SPDR={} SKIP={} CMD={} DATA={}",
            ard.spi_trace.len(),
            portc_writes,
            ddrc_writes,
            spdr_writes_in_trace,
            skip_count,
            cmd_count,
            data_count
        );

        if portc_writes == 0 {
            println!("[DIAG] *** ZERO PORTC writes detected! ***");
            println!(
                "[DIAG] *** The game's digitalWrite to PORTC is NOT reaching write_data(0x28)! ***"
            );
            println!("[DIAG] *** Possible cause: ST X / LD X instructions with wrong X value ***");
        }

        println!(
            "\n[DIAG] === SPI BYTE TRACE (first {} of {} entries) ===",
            ard.spi_trace.len().min(200),
            ard.spi_trace.len()
        );
        for (i, entry) in ard.spi_trace.iter().take(200).enumerate() {
            println!("[TRACE {:3}] {}", i, entry);
        }
    }
}
