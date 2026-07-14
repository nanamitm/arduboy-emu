//! Instruction-level semantics: per-instruction cycle counts (against the
//! ATmega32u4 datasheet) and LPM/ELPM flash-addressing boundaries.
//!
//! These call [`Arduboy::execute_inst`] directly with decoded instructions, so
//! they pin the cycle cost and flash-read behaviour independently of decoding.

use arduboy_core::opcodes::Instruction as I;
use arduboy_core::{Arduboy, FLASH_SIZE};

/// Every entry is (label, instruction, size, expected cycles). Cycle counts are
/// the ATmega32u4 datasheet values for a 16-bit-PC core. Only fixed-cost
/// instructions are listed here; data-dependent branch/skip timing is covered
/// by the cpu.rs unit tests (taken=2/not-taken=1, skip 1/2/3).
#[rustfmt::skip]
fn cycle_table() -> Vec<(&'static str, I, u8, u8)> {
    vec![
        // 1 cycle: ALU / logic / compare / move / bit / I-O / SREG.
        ("nop",  I::Nop,                    1, 1),
        ("add",  I::Add  { d: 0, r: 1 },    1, 1),
        ("adc",  I::Adc  { d: 0, r: 1 },    1, 1),
        ("sub",  I::Sub  { d: 0, r: 1 },    1, 1),
        ("subi", I::Subi { d: 16, k: 1 },   1, 1),
        ("sbc",  I::Sbc  { d: 0, r: 1 },    1, 1),
        ("sbci", I::Sbci { d: 16, k: 1 },   1, 1),
        ("and",  I::And  { d: 0, r: 1 },    1, 1),
        ("andi", I::Andi { d: 16, k: 1 },   1, 1),
        ("or",   I::Or   { d: 0, r: 1 },    1, 1),
        ("ori",  I::Ori  { d: 16, k: 1 },   1, 1),
        ("eor",  I::Eor  { d: 0, r: 1 },    1, 1),
        ("com",  I::Com  { d: 0 },          1, 1),
        ("neg",  I::Neg  { d: 0 },          1, 1),
        ("inc",  I::Inc  { d: 0 },          1, 1),
        ("dec",  I::Dec  { d: 0 },          1, 1),
        ("cp",   I::Cp   { d: 0, r: 1 },    1, 1),
        ("cpc",  I::Cpc  { d: 0, r: 1 },    1, 1),
        ("cpi",  I::Cpi  { d: 16, k: 1 },   1, 1),
        ("mov",  I::Mov  { d: 0, r: 1 },    1, 1),
        ("movw", I::Movw { d: 0, r: 2 },    1, 1),
        ("ldi",  I::Ldi  { d: 16, k: 1 },   1, 1),
        ("lsr",  I::Lsr  { d: 0 },          1, 1),
        ("asr",  I::Asr  { d: 0 },          1, 1),
        ("ror",  I::Ror  { d: 0 },          1, 1),
        ("swap", I::Swap { d: 0 },          1, 1),
        ("bst",  I::Bst  { d: 0, b: 0 },    1, 1),
        ("bld",  I::Bld  { d: 0, b: 0 },    1, 1),
        ("in",   I::In   { d: 0, a: 0x20 }, 1, 1),
        ("out",  I::Out  { a: 0x20, r: 0 }, 1, 1),
        ("sec",  I::Sec,                    1, 1),
        ("clc",  I::Clc,                    1, 1),
        ("sei",  I::Sei,                    1, 1),
        ("cli",  I::Cli,                    1, 1),
        ("set",  I::Set,                    1, 1),
        ("clt",  I::Clt,                    1, 1),
        ("wdr",  I::Wdr,                    1, 1),
        // 2 cycles: multiply / word arithmetic / load-store / stack / bit-I-O / rel jump.
        ("mul",    I::Mul    { d: 0, r: 1 },    1, 2),
        ("muls",   I::Muls   { d: 16, r: 17 },  1, 2),
        ("mulsu",  I::Mulsu  { d: 16, r: 17 },  1, 2),
        ("fmul",   I::Fmul   { d: 16, r: 17 },  1, 2),
        ("fmuls",  I::Fmuls  { d: 16, r: 17 },  1, 2),
        ("fmulsu", I::Fmulsu { d: 16, r: 17 },  1, 2),
        ("adiw",   I::Adiw   { d: 24, k: 1 },   1, 2),
        ("sbiw",   I::Sbiw   { d: 24, k: 1 },   1, 2),
        ("lds",    I::Lds    { d: 0, k: 0x100 },2, 2),
        ("sts",    I::Sts    { k: 0x100, r: 0 },2, 2),
        ("ldx",    I::LdX    { d: 0 },          1, 2),
        ("ldx+",   I::LdXInc { d: 0 },          1, 2),
        ("ldx-",   I::LdXDec { d: 0 },          1, 2),
        ("ldy",    I::LdY    { d: 0 },          1, 2),
        ("ldy+",   I::LdYInc { d: 0 },          1, 2),
        ("ldy-",   I::LdYDec { d: 0 },          1, 2),
        ("ldyq",   I::LdYQ   { d: 0, q: 1 },    1, 2),
        ("ldz",    I::LdZ    { d: 0 },          1, 2),
        ("ldz+",   I::LdZInc { d: 0 },          1, 2),
        ("ldz-",   I::LdZDec { d: 0 },          1, 2),
        ("ldzq",   I::LdZQ   { d: 0, q: 1 },    1, 2),
        ("stx",    I::StX    { r: 0 },          1, 2),
        ("stx+",   I::StXInc { r: 0 },          1, 2),
        ("stx-",   I::StXDec { r: 0 },          1, 2),
        ("sty",    I::StY    { r: 0 },          1, 2),
        ("sty+",   I::StYInc { r: 0 },          1, 2),
        ("sty-",   I::StYDec { r: 0 },          1, 2),
        ("styq",   I::StYQ   { r: 0, q: 1 },    1, 2),
        ("stz",    I::StZ    { r: 0 },          1, 2),
        ("stz+",   I::StZInc { r: 0 },          1, 2),
        ("stz-",   I::StZDec { r: 0 },          1, 2),
        ("stzq",   I::StZQ   { r: 0, q: 1 },    1, 2),
        ("push",   I::Push   { r: 0 },          1, 2),
        ("pop",    I::Pop    { d: 0 },          1, 2),
        ("sbi",    I::Sbi    { a: 0x20, b: 0 }, 1, 2),
        ("cbi",    I::Cbi    { a: 0x20, b: 0 }, 1, 2),
        ("rjmp",   I::Rjmp   { k: 0 },          1, 2),
        ("ijmp",   I::Ijmp,                     1, 2),
        // 3 cycles: rel/indirect call, direct jump, program-memory loads.
        ("rcall",   I::Rcall   { k: 0 }, 1, 3),
        ("icall",   I::Icall,            1, 3),
        ("jmp",     I::Jmp     { k: 0 }, 2, 3),
        ("lpm0",    I::Lpm0,             1, 3),
        ("lpmd",    I::LpmD    { d: 0 }, 1, 3),
        ("lpmd+",   I::LpmDInc { d: 0 }, 1, 3),
        ("elpm0",   I::Elpm0,            1, 3),
        ("elpmd",   I::ElpmD   { d: 0 }, 1, 3),
        ("elpmd+",  I::ElpmDInc{ d: 0 }, 1, 3),
        // 4 cycles: direct call and returns.
        ("call", I::Call { k: 0 }, 2, 4),
        ("ret",  I::Ret,           1, 4),
        ("reti", I::Reti,          1, 4),
    ]
}

#[test]
fn instruction_cycle_counts_match_datasheet() {
    let mut failures = Vec::new();
    for (label, inst, size, expected) in cycle_table() {
        // Fresh device per instruction so side effects (SP, PC) never accumulate.
        let mut ard = Arduboy::new();
        let got = ard.execute_inst(inst, size);
        if got != expected {
            failures.push(format!("{label}: expected {expected} cycles, got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "cycle mismatches:\n{}",
        failures.join("\n")
    );
}

// --- LPM / ELPM flash addressing ---

#[test]
fn lpm_reads_flash_byte_at_z() {
    let mut ard = Arduboy::new();
    ard.mem.flash[0x1234] = 0xAB;
    ard.mem.set_z(0x1234);
    ard.execute_inst(I::LpmD { d: 5 }, 1);
    assert_eq!(ard.mem.reg(5), 0xAB);
    assert_eq!(ard.mem.z(), 0x1234, "LpmD must not modify Z");
}

#[test]
fn lpm_inc_advances_z() {
    let mut ard = Arduboy::new();
    ard.mem.flash[0x0100] = 0x11;
    ard.mem.flash[0x0101] = 0x22;
    ard.mem.set_z(0x0100);
    ard.execute_inst(I::LpmDInc { d: 5 }, 1);
    assert_eq!(ard.mem.reg(5), 0x11);
    assert_eq!(ard.mem.z(), 0x0101);
    ard.execute_inst(I::LpmDInc { d: 6 }, 1);
    assert_eq!(ard.mem.reg(6), 0x22);
    assert_eq!(ard.mem.z(), 0x0102);
}

#[test]
fn lpm_beyond_flash_reads_zero() {
    // The first address past flash (still a valid 16-bit Z) must read 0, not panic.
    let mut ard = Arduboy::new();
    ard.mem.set_z(FLASH_SIZE as u16);
    ard.execute_inst(I::LpmD { d: 5 }, 1);
    assert_eq!(ard.mem.reg(5), 0);
}

#[test]
fn elpm_with_rampz_zero_matches_lpm() {
    let mut ard = Arduboy::new();
    ard.mem.flash[0x0100] = 0x42;
    ard.mem.data[0x5B] = 0; // RAMPZ = 0
    ard.mem.set_z(0x0100);
    ard.execute_inst(I::ElpmD { d: 5 }, 1);
    assert_eq!(ard.mem.reg(5), 0x42);
}

#[test]
fn elpm_inc_carries_into_rampz() {
    let mut ard = Arduboy::new();
    ard.mem.data[0x5B] = 0; // RAMPZ = 0
    ard.mem.set_z(0xFFFF);
    ard.execute_inst(I::ElpmDInc { d: 5 }, 1);
    // Z wraps 0xFFFF → 0x0000 and the carry advances RAMPZ to 1 (24-bit pointer).
    assert_eq!(ard.mem.z(), 0x0000);
    assert_eq!(
        ard.mem.data[0x5B], 1,
        "RAMPZ should increment on Z overflow"
    );
}
