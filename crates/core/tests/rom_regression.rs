//! End-to-end golden regression tests over the emulator's hot paths.
//!
//! Each test builds a tiny, self-authored AVR program (via the little assembler
//! below — no external/licensed ROM) and runs it end-to-end, then pins a
//! deterministic result. Together they cover the display pipelines (SSD1306 and
//! PCD8544 framebuffer hashes), button input (PINF → RAM), the timer + interrupt
//! path (Timer0 overflow ISR count), and audio (Timer1 CTC tone frequency).
//! A regression anywhere in fetch/decode/execute, the SPI/display routing, the
//! GPIO input path, interrupt dispatch, or the timers flips one of the goldens.

use arduboy_core::{Arduboy, CpuType};

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

/// IN Rd, A  (A is an I/O address 0..=63)
fn in_(d: u8, a: u8) -> u16 {
    0xB000 | (((a as u16 >> 4) & 0x3) << 9) | ((d as u16 & 0x1F) << 4) | (a as u16 & 0x0F)
}

/// STS addr, Rr — 32-bit; returns both words.
fn sts(addr: u16, r: u8) -> [u16; 2] {
    [0x9200 | ((r as u16 & 0x1F) << 4), addr]
}

/// LDS Rd, addr — 32-bit; returns both words.
fn lds(d: u8, addr: u16) -> [u16; 2] {
    [0x9000 | ((d as u16 & 0x1F) << 4), addr]
}

/// JMP k — 32-bit (k < 0x10000); returns both words.
fn jmp(k: u16) -> [u16; 2] {
    [0x940C, k]
}

/// INC Rd
fn inc(d: u8) -> u16 {
    0x9400 | ((d as u16 & 0x1F) << 4) | 0x03
}

fn sei() -> u16 {
    0x9478
}
fn reti() -> u16 {
    0x9518
}

/// Push a 16-bit word as two little-endian flash bytes.
fn words_to_bytes(words: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 2);
    for &w in words {
        bytes.push((w & 0xFF) as u8);
        bytes.push((w >> 8) as u8);
    }
    bytes
}

/// Write `words` into `flash` at a word address.
fn place(flash: &mut [u8], word_addr: usize, words: &[u16]) {
    let bytes = words_to_bytes(words);
    let start = word_addr * 2;
    flash[start..start + bytes.len()].copy_from_slice(&bytes);
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

// Gamebuino Classic (328P) PCD8544: CS = PC1, DC = PC2 (active low). PORTC is
// I/O address 0x08. Command mode = both low; data mode = DC (PC2) high.
const IO_PORTC: u8 = 0x08;
const PORTC_CMD: u8 = 0x00; // CS low (selected), DC low (command)
const PORTC_DATA: u8 = 0x04; // CS low (selected), DC high (PC2) → data

/// Build a PCD8544 driver ROM: basic init, address (0,0), 4 data bytes, spin.
fn build_rom_pcd8544() -> Vec<u8> {
    let mut words: Vec<u16> = Vec::new();
    let send = |words: &mut Vec<u16>, byte: u8| {
        words.push(ldi(18, byte));
        words.push(out(IO_SPDR, 18));
    };

    words.push(ldi(16, PORTC_CMD));
    words.push(ldi(17, PORTC_DATA));

    // Command mode.
    words.push(out(IO_PORTC, 16));
    send(&mut words, 0x20); // function set: basic instruction set, horizontal addr
    send(&mut words, 0x0C); // display control: normal mode
    send(&mut words, 0x80); // set X address = 0
    send(&mut words, 0x40); // set Y address (page) = 0

    // Data mode: same 4-column pattern as the SSD1306 test.
    words.push(out(IO_PORTC, 17));
    send(&mut words, 0xFF);
    send(&mut words, 0x81);
    send(&mut words, 0x18);
    send(&mut words, 0xAA);

    words.push(rjmp(-1));

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

#[test]
fn pcd8544_golden_framebuffer() {
    let rom = build_rom_pcd8544();
    let mut ard = Arduboy::new_with_cpu(CpuType::Atmega328p);
    ard.mem.flash[..rom.len()].copy_from_slice(&rom);

    for _ in 0..5 {
        ard.run_frame();
    }

    let fb = ard.framebuffer_u32();
    let blank = Arduboy::new_with_cpu(CpuType::Atmega328p).framebuffer_u32();
    let lit = fb.iter().filter(|&&p| p != blank[0]).count();
    let rgba = ard.framebuffer_rgba();
    let hash = fnv1a(rgba);

    println!("pcd8544 lit_pixels={lit} hash=0x{hash:016X}");

    assert_eq!(
        lit, 16,
        "unexpected lit-pixel count (PCD8544 rendering changed)"
    );
    assert_eq!(
        hash, GOLDEN_HASH_PCD,
        "PCD8544 framebuffer hash changed (rendering regressed)"
    );
}

const GOLDEN_HASH_PCD: u64 = 0x6F35_C76B_1E1F_D245;

/// Input path: a ROM that continuously mirrors PINF (the Arduboy direction
/// buttons) into RAM. Pressing a button drives its pin low (active-low), and
/// the CPU must observe it through the PINF read path (DDRF=0 → external pin).
#[test]
fn button_input_reaches_ram() {
    use arduboy_core::Button;

    // loop: IN r16, PINF(0x0F); STS 0x0100, r16; RJMP loop
    let s = sts(0x0100, 16);
    let words = [in_(16, 0x0F), s[0], s[1], rjmp(-4)];
    let rom = words_to_bytes(&words);

    let mut ard = Arduboy::new();
    ard.mem.flash[..rom.len()].copy_from_slice(&rom);

    let run = |ard: &mut Arduboy| {
        for _ in 0..2 {
            ard.run_frame();
        }
        ard.mem.data[0x0100]
    };

    // Nothing pressed: PINF reads all-high.
    assert_eq!(run(&mut ard), 0xFF, "idle PINF should be 0xFF");

    // Up = PF7 (active low) → bit 7 clears.
    ard.set_button(Button::Up, true);
    assert_eq!(run(&mut ard), 0x7F, "Up should clear PF7");

    // Add Right = PF6 → bits 7 and 6 clear.
    ard.set_button(Button::Right, true);
    assert_eq!(run(&mut ard), 0x3F, "Up+Right should clear PF7 and PF6");

    // Release both → back to all-high.
    ard.set_button(Button::Up, false);
    ard.set_button(Button::Right, false);
    assert_eq!(run(&mut ard), 0xFF, "release should restore PINF");
}

/// Timer + interrupt path: Timer0 overflow interrupt (the millis() timer) with
/// TOIE0 enabled fires the ISR, which increments a RAM counter. This exercises
/// timer counting, the interrupt-enable gate (SEI), and dispatch to the correct
/// vector (0x002E) — the same machinery whose Timer4 address was once wrong.
#[test]
fn timer0_overflow_interrupt_counts() {
    // I/O addresses: TCCR0B = 0x25, prescaler clk/1024 (CS = 0b101).
    const TCCR0B: u8 = 0x25;
    // Data addresses: TIMSK0 = 0x6E (TOIE0 = bit 0), counter in SRAM at 0x0100.
    const TIMSK0: u16 = 0x006E;
    const COUNTER: u16 = 0x0100;

    let mut flash = vec![0u8; 0x400];

    // Reset vector → main (word 0x40); Timer0 OVF vector (word 0x2E) → ISR (0x50).
    place(&mut flash, 0x0000, &jmp(0x0040));
    place(&mut flash, 0x002E, &jmp(0x0050));

    // main: configure Timer0, enable its overflow interrupt, clear counter, spin.
    let ts = sts(TIMSK0, 16);
    let cs = sts(COUNTER, 16);
    let main = [
        ldi(16, 0x05),
        out(TCCR0B, 16), // clk/1024
        ldi(16, 0x01),
        ts[0],
        ts[1], // TIMSK0 = TOIE0
        ldi(16, 0x00),
        cs[0],
        cs[1], // counter = 0
        sei(),
        rjmp(-1), // spin
    ];
    place(&mut flash, 0x0040, &main);

    // ISR: counter += 1; RETI.
    let ld = lds(16, COUNTER);
    let st = sts(COUNTER, 16);
    let isr = [ld[0], ld[1], inc(16), st[0], st[1], reti()];
    place(&mut flash, 0x0050, &isr);

    let mut ard = Arduboy::new();
    ard.mem.flash[..flash.len()].copy_from_slice(&flash);

    for _ in 0..10 {
        ard.run_frame();
    }

    let count = ard.mem.data[COUNTER as usize];
    println!("timer0 overflow count = {count}");

    // ~2.16M cycles / (256 × 1024) ≈ 8 overflows; deterministic, so pin it.
    assert_eq!(
        count, GOLDEN_TIMER0_COUNT,
        "Timer0 overflow ISR count changed"
    );
}

const GOLDEN_TIMER0_COUNT: u8 = 8;

/// Audio path: configure Timer1 for CTC mode with OC1A toggle (how the Arduboy
/// tone libraries generate a square wave), and check the frequency the emulator
/// derives for the speaker. f = clk / (2 · prescale · (OCR1A + 1)).
#[test]
fn timer1_ctc_tone_frequency() {
    // Timer1 registers (data space): TCCR1A=0x80, TCCR1B=0x81, OCR1AL=0x88,
    // OCR1AH=0x89. All above the I/O range, so use STS.
    let mut prog: Vec<u16> = Vec::new();
    let set = |prog: &mut Vec<u16>, addr: u16, val: u8| {
        prog.push(ldi(16, val));
        let s = sts(addr, 16);
        prog.push(s[0]);
        prog.push(s[1]);
    };
    set(&mut prog, 0x80, 0x40); // TCCR1A: COM1A = 01 (toggle OC1A)
    set(&mut prog, 0x89, 0x00); // OCR1AH = 0
    set(&mut prog, 0x88, 0xFF); // OCR1AL = 255 → OCR1A = 255
    set(&mut prog, 0x81, 0x0A); // TCCR1B: WGM12 (CTC) + CS=010 (clk/8) → starts timer
    prog.push(rjmp(-1));
    let rom = words_to_bytes(&prog);

    let mut ard = Arduboy::new();
    ard.mem.flash[..rom.len()].copy_from_slice(&rom);
    for _ in 0..2 {
        ard.run_frame();
    }

    let (l, r) = ard.get_audio_tone();
    let tone = l.max(r);
    println!("audio tone L={l} R={r}");

    // 16 MHz / (2 · 8 · 256) = 3906.25 Hz.
    let expected = 16_000_000.0 / (2.0 * 8.0 * 256.0);
    assert!(
        (tone - expected).abs() < 1.0,
        "Timer1 CTC tone {tone} Hz != expected {expected} Hz"
    );
}
