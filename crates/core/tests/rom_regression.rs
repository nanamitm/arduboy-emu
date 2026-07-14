//! End-to-end golden-framebuffer regression test.
//!
//! A tiny, self-authored AVR program (no external/licensed ROM) drives the
//! SSD1306 over the emulated SPI bus to paint a deterministic pattern, then
//! loops. Running it exercises the full pipeline — instruction fetch/decode/
//! execute, the SPI peripheral, the display CS/DC routing, and SSD1306
//! command/data handling — and the resulting framebuffer is pinned by a hash.
//! If any of those behaviours regress, the hash (and lit-pixel count) change.

use arduboy_core::Arduboy;

// --- Minimal AVR assembler (only the encodings this test needs) ---

/// LDI Rd, K  (d in 16..=31)
fn ldi(d: u8, k: u8) -> u16 {
    let d = (d - 16) as u16;
    0xE000 | ((k as u16 & 0xF0) << 4) | (d << 4) | (k as u16 & 0x0F)
}

/// OUT A, Rr  (A is an I/O address 0..=63)
fn out(a: u8, r: u8) -> u16 {
    0xB800 | (((a as u16 >> 4) & 0x3) << 9) | ((r as u16 & 0x1F) << 4) | (a as u16 & 0x0F)
}

/// RJMP k (word offset)
fn rjmp(k: i16) -> u16 {
    0xC000 | (k as u16 & 0x0FFF)
}

// I/O addresses (not data-space): PORTD = 0x0B, SPDR = 0x2E.
const IO_PORTD: u8 = 0x0B;
const IO_SPDR: u8 = 0x2E;

// PORTD pin roles for the Arduboy SSD1306: DC = PD4, CS = PD6 (active low).
const PORTD_CMD: u8 = 0x00; // CS low (selected), DC low (command)
const PORTD_DATA: u8 = 0x10; // CS low (selected), DC high (data)

/// Build the test program: init the SSD1306, set a 4×1 window, write 4 data
/// bytes, then spin. Returns the flash image bytes (little-endian words).
fn build_rom() -> Vec<u8> {
    let mut words: Vec<u16> = Vec::new();
    let send = |words: &mut Vec<u16>, byte: u8| {
        words.push(ldi(18, byte));
        words.push(out(IO_SPDR, 18));
    };

    words.push(ldi(16, PORTD_CMD));
    words.push(ldi(17, PORTD_DATA));

    // Command mode.
    words.push(out(IO_PORTD, 16));
    send(&mut words, 0xAF); // display on (>=0x80 → SSD1306 auto-detect)
    send(&mut words, 0x81); // set contrast...
    send(&mut words, 0xFF); // ...to full (lit pixels render bright)
    send(&mut words, 0x21); // set column address
    send(&mut words, 0x00); //   start 0
    send(&mut words, 0x03); //   end 3  (4-column window)
    send(&mut words, 0x22); // set page address
    send(&mut words, 0x00); //   start 0
    send(&mut words, 0x00); //   end 0  (single page)

    // Data mode: paint 4 columns of page 0 with a fixed pattern.
    words.push(out(IO_PORTD, 17));
    send(&mut words, 0xFF); // col 0: rows 0..7 on
    send(&mut words, 0x81); // col 1: rows 0 and 7
    send(&mut words, 0x18); // col 2: rows 3 and 4
    send(&mut words, 0xAA); // col 3: rows 1,3,5,7

    words.push(rjmp(-1)); // spin forever

    let mut bytes = Vec::with_capacity(words.len() * 2);
    for w in words {
        bytes.push((w & 0xFF) as u8);
        bytes.push((w >> 8) as u8);
    }
    bytes
}

/// FNV-1a hash of the framebuffer bytes.
fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[test]
fn ssd1306_golden_framebuffer() {
    let rom = build_rom();
    let mut ard = Arduboy::new();
    ard.mem.flash[..rom.len()].copy_from_slice(&rom);

    // A few frames are ample for the ~40-instruction setup plus SPI flush.
    for _ in 0..5 {
        ard.run_frame();
    }

    let fb = ard.framebuffer_u32();
    let blank = Arduboy::new().framebuffer_u32();
    let lit = fb.iter().filter(|&&p| p != blank[0]).count();
    let rgba = ard.framebuffer_rgba();
    let hash = fnv1a(rgba);

    // Print so the golden values can be captured on first run.
    println!("lit_pixels={lit} hash=0x{hash:016X}");

    // Expected: cols 0..3 of page 0 → 8 + 2 + 2 + 4 = 16 lit pixels.
    assert_eq!(lit, 16, "unexpected lit-pixel count (rendering changed)");
    assert_eq!(
        hash, GOLDEN_HASH,
        "framebuffer hash changed (rendering regressed)"
    );
}

// Captured from the emulator; frozen as the regression golden.
const GOLDEN_HASH: u64 = 0x0133_CD73_A7AA_4DC5;
