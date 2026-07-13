//! AVR instruction decoder for ATmega32u4.
//!
//! Decodes 16-bit (and 32-bit) AVR instruction words into a typed
//! [`Instruction`] enum. Covers 80+ instructions used by Arduino/Arduboy
//! programs compiled with avr-gcc, including all arithmetic, logic, branch,
//! load/store, I/O, multiply, shift, bit, and status register operations.

/// Decoded AVR instruction with operands.
///
/// Each variant carries its decoded operands (register indices, immediates,
/// addresses) ready for execution. Register fields `d` and `r` are 0–31,
/// `k` is an immediate constant, and `a` is an I/O address in data space.
#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    Nop,
    // Arithmetic
    Add { d: u8, r: u8 },
    Adc { d: u8, r: u8 },
    Sub { d: u8, r: u8 },
    Subi { d: u8, k: u8 },
    Sbc { d: u8, r: u8 },
    Sbci { d: u8, k: u8 },
    And { d: u8, r: u8 },
    Andi { d: u8, k: u8 },
    Or { d: u8, r: u8 },
    Ori { d: u8, k: u8 },
    Eor { d: u8, r: u8 },
    Com { d: u8 },
    Neg { d: u8 },
    Inc { d: u8 },
    Dec { d: u8 },
    Mul { d: u8, r: u8 },
    Muls { d: u8, r: u8 },
    Mulsu { d: u8, r: u8 },
    Fmul { d: u8, r: u8 },
    Fmuls { d: u8, r: u8 },
    Fmulsu { d: u8, r: u8 },
    Adiw { d: u8, k: u8 },
    Sbiw { d: u8, k: u8 },
    // Compare
    Cp { d: u8, r: u8 },
    Cpc { d: u8, r: u8 },
    Cpi { d: u8, k: u8 },
    // Data transfer
    Mov { d: u8, r: u8 },
    Movw { d: u8, r: u8 },
    Ldi { d: u8, k: u8 },
    Lds { d: u8, k: u16 },
    Sts { k: u16, r: u8 },
    LdX { d: u8 },
    LdXInc { d: u8 },
    LdXDec { d: u8 },
    LdY { d: u8 },
    LdYInc { d: u8 },
    LdYDec { d: u8 },
    LdYQ { d: u8, q: u8 },
    LdZ { d: u8 },
    LdZInc { d: u8 },
    LdZDec { d: u8 },
    LdZQ { d: u8, q: u8 },
    StX { r: u8 },
    StXInc { r: u8 },
    StXDec { r: u8 },
    StY { r: u8 },
    StYInc { r: u8 },
    StYDec { r: u8 },
    StYQ { r: u8, q: u8 },
    StZ { r: u8 },
    StZInc { r: u8 },
    StZDec { r: u8 },
    StZQ { r: u8, q: u8 },
    // Stack
    Push { r: u8 },
    Pop { d: u8 },
    // Shift/Bit
    Lsr { d: u8 },
    Asr { d: u8 },
    Ror { d: u8 },
    Swap { d: u8 },
    Bst { d: u8, b: u8 },
    Bld { d: u8, b: u8 },
    Sbi { a: u8, b: u8 },
    Cbi { a: u8, b: u8 },
    // Branch
    Rjmp { k: i16 },
    Rcall { k: i16 },
    Ret,
    Reti,
    Jmp { k: u32 },
    Call { k: u32 },
    Ijmp,
    Icall,
    Eijmp,
    Eicall,
    Cpse { d: u8, r: u8 },
    Sbrc { r: u8, b: u8 },
    Sbrs { r: u8, b: u8 },
    Sbic { a: u8, b: u8 },
    Sbis { a: u8, b: u8 },
    Brbs { s: u8, k: i8 },
    Brbc { s: u8, k: i8 },
    // I/O
    In { d: u8, a: u8 },
    Out { a: u8, r: u8 },
    // LPM
    Lpm0,
    LpmD { d: u8 },
    LpmDInc { d: u8 },
    // ELPM (Extended LPM - uses RAMPZ:Z)
    Elpm0,
    ElpmD { d: u8 },
    ElpmDInc { d: u8 },
    // Status register
    Sei,
    Cli,
    Sec,
    Clc,
    Sen,
    Cln,
    Sez,
    Clz,
    Sev,
    Clv,
    Ses,
    Cls,
    Seh,
    Clh,
    Set,
    Clt,
    // Misc
    Sleep,
    Wdr,
    Break,
    Spm,
    Unknown(u16),
}

/// Decode a 16-bit instruction word (with the next word for 32-bit instructions).
/// Returns (Instruction, size_in_words)
pub fn decode(word: u16, next_word: u16) -> (Instruction, u8) {
    // 32-bit instructions first (JMP, CALL, LDS, STS)
    // JMP: 1001 010k kkkk 110k kkkk kkkk kkkk kkkk
    if word & 0xFE0E == 0x940C {
        let k = ((((word as u32 >> 3) & 0x3E) | (word as u32 & 1)) << 16) | next_word as u32;
        return (Instruction::Jmp { k }, 2);
    }
    // CALL: 1001 010k kkkk 111k kkkk kkkk kkkk kkkk
    if word & 0xFE0E == 0x940E {
        let k = ((((word as u32 >> 3) & 0x3E) | (word as u32 & 1)) << 16) | next_word as u32;
        return (Instruction::Call { k }, 2);
    }
    // LDS Rd,k: 1001 000d dddd 0000 kkkk kkkk kkkk kkkk
    if word & 0xFE0F == 0x9000 {
        let d = ((word >> 4) & 0x1F) as u8;
        return (Instruction::Lds { d, k: next_word }, 2);
    }
    // STS k,Rr: 1001 001d dddd 0000 kkkk kkkk kkkk kkkk
    if word & 0xFE0F == 0x9200 {
        let r = ((word >> 4) & 0x1F) as u8;
        return (Instruction::Sts { k: next_word, r }, 2);
    }

    // 16-bit instructions
    match word {
        0x0000 => return (Instruction::Nop, 1),
        0x9508 => return (Instruction::Ret, 1),
        0x9518 => return (Instruction::Reti, 1),
        0x9409 => return (Instruction::Ijmp, 1),
        0x9509 => return (Instruction::Icall, 1),
        0x9419 => return (Instruction::Eijmp, 1),
        0x9519 => return (Instruction::Eicall, 1),
        0x9588 => return (Instruction::Sleep, 1),
        0x95A8 => return (Instruction::Wdr, 1),
        0x9598 => return (Instruction::Break, 1),
        0x95E8 | 0x95F8 => return (Instruction::Spm, 1),
        0x95C8 => return (Instruction::Lpm0, 1),
        0x95D8 => return (Instruction::Elpm0, 1),
        // BSET/BCLR for individual flags
        0x9408 => return (Instruction::Sec, 1),
        0x9488 => return (Instruction::Clc, 1),
        0x9418 => return (Instruction::Sez, 1),
        0x9498 => return (Instruction::Clz, 1),
        0x9428 => return (Instruction::Sen, 1),
        0x94A8 => return (Instruction::Cln, 1),
        0x9438 => return (Instruction::Sev, 1),
        0x94B8 => return (Instruction::Clv, 1),
        0x9448 => return (Instruction::Ses, 1),
        0x94C8 => return (Instruction::Cls, 1),
        0x9458 => return (Instruction::Seh, 1),
        0x94D8 => return (Instruction::Clh, 1),
        0x9468 => return (Instruction::Set, 1),
        0x94E8 => return (Instruction::Clt, 1),
        0x9478 => return (Instruction::Sei, 1),
        0x94F8 => return (Instruction::Cli, 1),
        _ => {}
    }

    // Pattern match by upper nibble / prefix
    let upper4 = word >> 12;

    match upper4 {
        // 0000 xxxx - NOP, MOVW, MULS, MULSU, FMUL, FMULS, MUL variants, SBC, ADD/CPC
        0x0 => {
            let upper8 = word >> 8;
            match upper8 & 0xFC {
                // MOVW: 0000 0001 dddd rrrr
                0x00 if word & 0xFF00 == 0x0100 => {
                    let d = (((word >> 4) & 0xF) * 2) as u8;
                    let r = ((word & 0xF) * 2) as u8;
                    return (Instruction::Movw { d, r }, 1);
                }
                // MULS: 0000 0010 dddd rrrr
                0x00 if word & 0xFF00 == 0x0200 => {
                    let d = (((word >> 4) & 0xF) + 16) as u8;
                    let r = ((word & 0xF) + 16) as u8;
                    return (Instruction::Muls { d, r }, 1);
                }
                // MULSU: 0000 0011 0ddd 0rrr
                0x00 if word & 0xFF88 == 0x0300 => {
                    let d = (((word >> 4) & 0x7) + 16) as u8;
                    let r = ((word & 0x7) + 16) as u8;
                    return (Instruction::Mulsu { d, r }, 1);
                }
                // FMUL: 0000 0011 0ddd 1rrr
                0x00 if word & 0xFF88 == 0x0308 => {
                    let d = (((word >> 4) & 0x7) + 16) as u8;
                    let r = ((word & 0x7) + 16) as u8;
                    return (Instruction::Fmul { d, r }, 1);
                }
                // FMULS: 0000 0011 1ddd 0rrr
                0x00 if word & 0xFF88 == 0x0380 => {
                    let d = (((word >> 4) & 0x7) + 16) as u8;
                    let r = ((word & 0x7) + 16) as u8;
                    return (Instruction::Fmuls { d, r }, 1);
                }
                // FMULSU: 0000 0011 1ddd 1rrr
                0x00 if word & 0xFF88 == 0x0388 => {
                    let d = (((word >> 4) & 0x7) + 16) as u8;
                    let r = ((word & 0x7) + 16) as u8;
                    return (Instruction::Fmulsu { d, r }, 1);
                }
                _ => {}
            }
            // CPC: 0000 01rd dddd rrrr
            if word & 0xFC00 == 0x0400 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Cpc { d, r }, 1);
            }
            // SBC: 0000 10rd dddd rrrr
            if word & 0xFC00 == 0x0800 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Sbc { d, r }, 1);
            }
            // ADD: 0000 11rd dddd rrrr
            if word & 0xFC00 == 0x0C00 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Add { d, r }, 1);
            }
        }

        // 0001 xxxx - CPSE, CP, SUB, ADC
        0x1 => {
            if word & 0xFC00 == 0x1000 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Cpse { d, r }, 1);
            }
            if word & 0xFC00 == 0x1400 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Cp { d, r }, 1);
            }
            if word & 0xFC00 == 0x1800 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Sub { d, r }, 1);
            }
            if word & 0xFC00 == 0x1C00 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Adc { d, r }, 1);
            }
        }

        // 0010 xxxx - AND, EOR, OR, MOV
        0x2 => {
            if word & 0xFC00 == 0x2000 {
                let (d, r) = decode_5_5(word);
                return (Instruction::And { d, r }, 1);
            }
            if word & 0xFC00 == 0x2400 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Eor { d, r }, 1);
            }
            if word & 0xFC00 == 0x2800 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Or { d, r }, 1);
            }
            if word & 0xFC00 == 0x2C00 {
                let (d, r) = decode_5_5(word);
                return (Instruction::Mov { d, r }, 1);
            }
        }

        // 0011 xxxx - CPI
        0x3 => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Cpi { d: d + 16, k }, 1);
        }

        // 0100 xxxx - SBCI
        0x4 => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Sbci { d: d + 16, k }, 1);
        }

        // 0101 xxxx - SUBI
        0x5 => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Subi { d: d + 16, k }, 1);
        }

        // 0110 xxxx - ORI
        0x6 => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Ori { d: d + 16, k }, 1);
        }

        // 0111 xxxx - ANDI
        0x7 => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Andi { d: d + 16, k }, 1);
        }

        // 1000/1010 - LDD/STD with displacement, LD/ST Y/Z
        0x8 | 0xA => {
            let q = (((word >> 13) & 1) << 5) | (((word >> 10) & 3) << 3) | (word & 7);
            let q = q as u8;
            let d_r = ((word >> 4) & 0x1F) as u8;
            let is_store = word & 0x0200 != 0; // bit 9
            let is_y = word & 0x0008 != 0; // bit 3

            if is_y {
                if !is_store {
                    if q == 0 {
                        return (Instruction::LdY { d: d_r }, 1);
                    } else {
                        return (Instruction::LdYQ { d: d_r, q }, 1);
                    }
                } else {
                    if q == 0 {
                        return (Instruction::StY { r: d_r }, 1);
                    } else {
                        return (Instruction::StYQ { r: d_r, q }, 1);
                    }
                }
            } else {
                if !is_store {
                    if q == 0 {
                        return (Instruction::LdZ { d: d_r }, 1);
                    } else {
                        return (Instruction::LdZQ { d: d_r, q }, 1);
                    }
                } else {
                    if q == 0 {
                        return (Instruction::StZ { r: d_r }, 1);
                    } else {
                        return (Instruction::StZQ { r: d_r, q }, 1);
                    }
                }
            }
        }

        // 1001 xxxx - many single-register ops, LD/ST, PUSH/POP, etc.
        0x9 => {
            return decode_1001(word, next_word);
        }

        // 1011 xxxx - IN/OUT
        0xB => {
            let d_r = ((word >> 4) & 0x1F) as u8;
            let a = (((word >> 9) & 3) << 4 | (word & 0xF)) as u8;
            if word & 0x0800 == 0 {
                // IN
                return (
                    Instruction::In {
                        d: d_r,
                        a: a + 0x20,
                    },
                    1,
                ); // convert I/O addr to data space
            } else {
                // OUT
                return (
                    Instruction::Out {
                        a: a + 0x20,
                        r: d_r,
                    },
                    1,
                );
            }
        }

        // 1100 xxxx - RJMP
        0xC => {
            let k = sign_extend_12(word & 0x0FFF);
            return (Instruction::Rjmp { k }, 1);
        }

        // 1101 xxxx - RCALL
        0xD => {
            let k = sign_extend_12(word & 0x0FFF);
            return (Instruction::Rcall { k }, 1);
        }

        // 1110 xxxx - LDI
        0xE => {
            let (d, k) = decode_4_8(word);
            return (Instruction::Ldi { d: d + 16, k }, 1);
        }

        // 1111 xxxx - BRBS/BRBC, BLD, BST, SBRC, SBRS
        0xF => {
            return decode_1111(word);
        }

        _ => {}
    }

    (Instruction::Unknown(word), 1)
}

/// Decode 1001 xxxx instructions
fn decode_1001(word: u16, _next_word: u16) -> (Instruction, u8) {
    let d_r = ((word >> 4) & 0x1F) as u8;

    // Single-register ops: 1001 010d dddd xxxx
    if word & 0xFE00 == 0x9400 {
        match word & 0x000F {
            0x0 => return (Instruction::Com { d: d_r }, 1),
            0x1 => return (Instruction::Neg { d: d_r }, 1),
            0x2 => return (Instruction::Swap { d: d_r }, 1),
            0x3 => return (Instruction::Inc { d: d_r }, 1),
            0x5 => return (Instruction::Asr { d: d_r }, 1),
            0x6 => return (Instruction::Lsr { d: d_r }, 1),
            0x7 => return (Instruction::Ror { d: d_r }, 1),
            0xA => return (Instruction::Dec { d: d_r }, 1),
            _ => {}
        }
    }

    // ADIW: 1001 0110 KKdd KKKK
    if word & 0xFF00 == 0x9600 {
        let d = ((((word >> 4) & 3) * 2) + 24) as u8;
        let k = ((((word >> 6) & 0x03) << 4) | (word & 0x0F)) as u8;
        return (Instruction::Adiw { d, k }, 1);
    }

    // SBIW: 1001 0111 KKdd KKKK
    if word & 0xFF00 == 0x9700 {
        let d = ((((word >> 4) & 3) * 2) + 24) as u8;
        let k = ((((word >> 6) & 0x03) << 4) | (word & 0x0F)) as u8;
        return (Instruction::Sbiw { d, k }, 1);
    }

    // CBI: 1001 1000 AAAA Abbb
    if word & 0xFF00 == 0x9800 {
        let a = ((word >> 3) & 0x1F) as u8 + 0x20; // convert to data space
        let b = (word & 7) as u8;
        return (Instruction::Cbi { a, b }, 1);
    }

    // SBIC: 1001 1001 AAAA Abbb
    if word & 0xFF00 == 0x9900 {
        let a = ((word >> 3) & 0x1F) as u8 + 0x20;
        let b = (word & 7) as u8;
        return (Instruction::Sbic { a, b }, 1);
    }

    // SBI: 1001 1010 AAAA Abbb
    if word & 0xFF00 == 0x9A00 {
        let a = ((word >> 3) & 0x1F) as u8 + 0x20;
        let b = (word & 7) as u8;
        return (Instruction::Sbi { a, b }, 1);
    }

    // SBIS: 1001 1011 AAAA Abbb
    if word & 0xFF00 == 0x9B00 {
        let a = ((word >> 3) & 0x1F) as u8 + 0x20;
        let b = (word & 7) as u8;
        return (Instruction::Sbis { a, b }, 1);
    }

    // MUL: 1001 11rd dddd rrrr
    if word & 0xFC00 == 0x9C00 {
        let (d, r) = decode_5_5(word);
        return (Instruction::Mul { d, r }, 1);
    }

    // LD/ST with X/Y/Z pre/post-inc/dec
    // Load variants: 1001 000d dddd xxxx
    if word & 0xFE00 == 0x9000 {
        match word & 0x000F {
            // LDS handled in 32-bit above (0x0000)
            0x1 => return (Instruction::LdZInc { d: d_r }, 1),
            0x2 => return (Instruction::LdZDec { d: d_r }, 1),
            0x4 => return (Instruction::LpmD { d: d_r }, 1),
            0x5 => return (Instruction::LpmDInc { d: d_r }, 1),
            0x6 => return (Instruction::ElpmD { d: d_r }, 1),
            0x7 => return (Instruction::ElpmDInc { d: d_r }, 1),
            0x9 => return (Instruction::LdYInc { d: d_r }, 1),
            0xA => return (Instruction::LdYDec { d: d_r }, 1),
            0xC => return (Instruction::LdX { d: d_r }, 1),
            0xD => return (Instruction::LdXInc { d: d_r }, 1),
            0xE => return (Instruction::LdXDec { d: d_r }, 1),
            0xF => return (Instruction::Pop { d: d_r }, 1),
            _ => {}
        }
    }

    // Store variants: 1001 001r rrrr xxxx
    if word & 0xFE00 == 0x9200 {
        match word & 0x000F {
            // STS handled in 32-bit above (0x0000)
            0x1 => return (Instruction::StZInc { r: d_r }, 1),
            0x2 => return (Instruction::StZDec { r: d_r }, 1),
            0x9 => return (Instruction::StYInc { r: d_r }, 1),
            0xA => return (Instruction::StYDec { r: d_r }, 1),
            0xC => return (Instruction::StX { r: d_r }, 1),
            0xD => return (Instruction::StXInc { r: d_r }, 1),
            0xE => return (Instruction::StXDec { r: d_r }, 1),
            0xF => return (Instruction::Push { r: d_r }, 1),
            _ => {}
        }
    }

    (Instruction::Unknown(word), 1)
}

/// Decode 1111 xxxx instructions (branches, BLD, BST, SBRC, SBRS)
fn decode_1111(word: u16) -> (Instruction, u8) {
    // BRBS: 1111 00kk kkkk ksss
    if word & 0xFC00 == 0xF000 {
        let s = (word & 7) as u8;
        let k = ((word >> 3) & 0x7F) as i8;
        let k = ((k as i16) << 9 >> 9) as i8; // sign extend 7-bit
        return (Instruction::Brbs { s, k }, 1);
    }
    // BRBC: 1111 01kk kkkk ksss
    if word & 0xFC00 == 0xF400 {
        let s = (word & 7) as u8;
        let k = ((word >> 3) & 0x7F) as i8;
        let k = ((k as i16) << 9 >> 9) as i8;
        return (Instruction::Brbc { s, k }, 1);
    }
    // BLD: 1111 100d dddd 0bbb
    if word & 0xFE08 == 0xF800 {
        let d = ((word >> 4) & 0x1F) as u8;
        let b = (word & 7) as u8;
        return (Instruction::Bld { d, b }, 1);
    }
    // BST: 1111 101d dddd 0bbb
    if word & 0xFE08 == 0xFA00 {
        let d = ((word >> 4) & 0x1F) as u8;
        let b = (word & 7) as u8;
        return (Instruction::Bst { d, b }, 1);
    }
    // SBRC: 1111 110r rrrr 0bbb
    if word & 0xFE08 == 0xFC00 {
        let r = ((word >> 4) & 0x1F) as u8;
        let b = (word & 7) as u8;
        return (Instruction::Sbrc { r, b }, 1);
    }
    // SBRS: 1111 111r rrrr 0bbb
    if word & 0xFE08 == 0xFE00 {
        let r = ((word >> 4) & 0x1F) as u8;
        let b = (word & 7) as u8;
        return (Instruction::Sbrs { r, b }, 1);
    }

    (Instruction::Unknown(word), 1)
}

// --- Helper decoders ---

/// Decode 5-bit d, 5-bit r from: xxxx xxrd dddd rrrr
#[inline(always)]
fn decode_5_5(word: u16) -> (u8, u8) {
    let d = ((word >> 4) & 0x1F) as u8;
    let r = ((word & 0x0F) | ((word >> 5) & 0x10)) as u8;
    (d, r)
}

/// Decode 4-bit d, 8-bit K from: xxxx KKKK dddd KKKK
#[inline(always)]
fn decode_4_8(word: u16) -> (u8, u8) {
    let d = ((word >> 4) & 0x0F) as u8;
    let k = (((word >> 4) & 0xF0) | (word & 0x0F)) as u8;
    (d, k)
}

/// Sign-extend 12-bit value to i16
#[inline(always)]
fn sign_extend_12(val: u16) -> i16 {
    if val & 0x800 != 0 {
        (val | 0xF000) as i16
    } else {
        val as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_nop() {
        let (inst, sz) = decode(0x0000, 0);
        assert!(matches!(inst, Instruction::Nop));
        assert_eq!(sz, 1);
    }

    #[test]
    fn test_decode_ldi() {
        // LDI R16, 0xFF => 1110 1111 0000 1111 = 0xEF0F
        let (inst, sz) = decode(0xEF0F, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::Ldi { d, k } => {
                assert_eq!(d, 16);
                assert_eq!(k, 0xFF);
            }
            _ => panic!("Expected Ldi, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_rjmp() {
        let (inst, _) = decode(0xC000, 0);
        match inst {
            Instruction::Rjmp { k } => assert_eq!(k, 0),
            _ => panic!("Expected Rjmp"),
        }
        let (inst, _) = decode(0xCFFF, 0);
        match inst {
            Instruction::Rjmp { k } => assert_eq!(k, -1),
            _ => panic!("Expected Rjmp"),
        }
    }

    #[test]
    fn test_decode_jmp() {
        let (inst, sz) = decode(0x940C, 0x0034);
        assert_eq!(sz, 2);
        match inst {
            Instruction::Jmp { k } => assert_eq!(k, 0x0034),
            _ => panic!("Expected Jmp, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_add() {
        let (inst, _) = decode(0x0C01, 0);
        match inst {
            Instruction::Add { d, r } => {
                assert_eq!(d, 0);
                assert_eq!(r, 1);
            }
            _ => panic!("Expected Add, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_push_pop() {
        let (inst, _) = decode(0x920F, 0);
        match inst {
            Instruction::Push { r } => assert_eq!(r, 0),
            _ => panic!("Expected Push, got {:?}", inst),
        }
        let (inst, _) = decode(0x900F, 0);
        match inst {
            Instruction::Pop { d } => assert_eq!(d, 0),
            _ => panic!("Expected Pop, got {:?}", inst),
        }
    }

    // Critical: STD Y+q and STD Z+q must decode correctly
    // These are the most common store instructions generated by GCC for local variables

    #[test]
    fn test_decode_std_y_q() {
        // STD Y+1, R0 => 10q0 qq1r rrrr 1qqq
        // Y-based (bit3=1), store (bit9=1), q=1, r=0
        // q=1: q5=0, q4:3=00, q2:0=001
        // Encoding: 1000 0010 0000 1001 = 0x8209
        let (inst, sz) = decode(0x8209, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::StYQ { r, q } => {
                assert_eq!(r, 0);
                assert_eq!(q, 1);
            }
            _ => panic!("Expected StYQ, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_ldd_y_q() {
        // LDD R0, Y+1 => 10q0 qq0d dddd 1qqq
        // Y-based (bit3=1), load (bit9=0), q=1, d=0
        // Encoding: 1000 0000 0000 1001 = 0x8009
        let (inst, sz) = decode(0x8009, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::LdYQ { d, q } => {
                assert_eq!(d, 0);
                assert_eq!(q, 1);
            }
            _ => panic!("Expected LdYQ, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_std_z_q() {
        // STD Z+1, R0 => 10q0 qq1r rrrr 0qqq
        // Z-based (bit3=0), store (bit9=1), q=1, r=0
        // Encoding: 1000 0010 0000 0001 = 0x8201
        let (inst, sz) = decode(0x8201, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::StZQ { r, q } => {
                assert_eq!(r, 0);
                assert_eq!(q, 1);
            }
            _ => panic!("Expected StZQ, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_ldd_z_q() {
        // LDD R0, Z+1 => 10q0 qq0d dddd 0qqq
        // Z-based (bit3=0), load (bit9=0), q=1, d=0
        // Encoding: 1000 0000 0000 0001 = 0x8001
        let (inst, sz) = decode(0x8001, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::LdZQ { d, q } => {
                assert_eq!(d, 0);
                assert_eq!(q, 1);
            }
            _ => panic!("Expected LdZQ, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_std_y_q_large_offset() {
        // STD Y+63, R31 => q=63 (111111), r=31
        // q5=1 (bit13), q4:3=11 (bits11:10), q2:0=111 (bits2:0)
        // bit9=1 (store), bit3=1 (Y)
        // 10_1_0_11_1_11111_1_111 = 1010 1111 1111 1111 = 0xAFFF
        let (inst, sz) = decode(0xAFFF, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::StYQ { r, q } => {
                assert_eq!(r, 31);
                assert_eq!(q, 63);
            }
            _ => panic!("Expected StYQ, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_sbiw() {
        // SBIW R28, 1 => 1001 0111 KKdd KKKK
        // K=1 (KK=00, KKKK=0001), dd=10 (R28)
        // 1001 0111 0010 0001 = 0x9721
        let (inst, sz) = decode(0x9721, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::Sbiw { d, k } => {
                assert_eq!(d, 28);
                assert_eq!(k, 1);
            }
            _ => panic!("Expected Sbiw, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_sbiw_large_k() {
        // SBIW R24, 63 => K=63=0b111111, dd=00 (R24)
        // KK=11, KKKK=1111
        // 1001 0111 1100 1111 = 0x97CF
        let (inst, sz) = decode(0x97CF, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::Sbiw { d, k } => {
                assert_eq!(d, 24);
                assert_eq!(k, 63);
            }
            _ => panic!("Expected Sbiw, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_adiw() {
        // ADIW R26, 1 => 1001 0110 KKdd KKKK
        // K=1 (KK=00, KKKK=0001), dd=01 (R26)
        // 1001 0110 0001 0001 = 0x9611
        let (inst, sz) = decode(0x9611, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::Adiw { d, k } => {
                assert_eq!(d, 26);
                assert_eq!(k, 1);
            }
            _ => panic!("Expected Adiw, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_adiw_large_k() {
        // ADIW R30, 48 => K=48=0b110000, dd=11 (R30)
        // KK=11, KKKK=0000
        // 1001 0110 1111 0000 = 0x96F0
        let (inst, sz) = decode(0x96F0, 0);
        assert_eq!(sz, 1);
        match inst {
            Instruction::Adiw { d, k } => {
                assert_eq!(d, 30);
                assert_eq!(k, 48);
            }
            _ => panic!("Expected Adiw, got {:?}", inst),
        }
    }
}
