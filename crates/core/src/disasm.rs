//! AVR instruction disassembler.
//!
//! Converts decoded [`Instruction`] values back to human-readable assembly text.
//! Used by the debugger for breakpoint, step, and register-dump views.

use crate::opcodes::Instruction;

/// Format a decoded instruction as an assembly string.
///
/// The output follows AVR assembly conventions (e.g. `ADD R1, R2`).
/// The `pc` parameter (word address) is used to resolve relative branch targets.
pub fn disassemble(inst: Instruction, pc: u16) -> String {
    match inst {
        Instruction::Nop => "NOP".into(),
        // Arithmetic
        Instruction::Add { d, r } => format!("ADD R{}, R{}", d, r),
        Instruction::Adc { d, r } => format!("ADC R{}, R{}", d, r),
        Instruction::Sub { d, r } => format!("SUB R{}, R{}", d, r),
        Instruction::Subi { d, k } => format!("SUBI R{}, 0x{:02X}", d, k),
        Instruction::Sbc { d, r } => format!("SBC R{}, R{}", d, r),
        Instruction::Sbci { d, k } => format!("SBCI R{}, 0x{:02X}", d, k),
        Instruction::And { d, r } => format!("AND R{}, R{}", d, r),
        Instruction::Andi { d, k } => format!("ANDI R{}, 0x{:02X}", d, k),
        Instruction::Or { d, r } => format!("OR R{}, R{}", d, r),
        Instruction::Ori { d, k } => format!("ORI R{}, 0x{:02X}", d, k),
        Instruction::Eor { d, r } => format!("EOR R{}, R{}", d, r),
        Instruction::Com { d } => format!("COM R{}", d),
        Instruction::Neg { d } => format!("NEG R{}", d),
        Instruction::Inc { d } => format!("INC R{}", d),
        Instruction::Dec { d } => format!("DEC R{}", d),
        Instruction::Mul { d, r } => format!("MUL R{}, R{}", d, r),
        Instruction::Muls { d, r } => format!("MULS R{}, R{}", d, r),
        Instruction::Mulsu { d, r } => format!("MULSU R{}, R{}", d, r),
        Instruction::Fmul { d, r } => format!("FMUL R{}, R{}", d, r),
        Instruction::Fmuls { d, r } => format!("FMULS R{}, R{}", d, r),
        Instruction::Fmulsu { d, r } => format!("FMULSU R{}, R{}", d, r),
        Instruction::Adiw { d, k } => format!("ADIW R{}:R{}, {}", d + 1, d, k),
        Instruction::Sbiw { d, k } => format!("SBIW R{}:R{}, {}", d + 1, d, k),
        // Compare
        Instruction::Cp { d, r } => format!("CP R{}, R{}", d, r),
        Instruction::Cpc { d, r } => format!("CPC R{}, R{}", d, r),
        Instruction::Cpi { d, k } => format!("CPI R{}, 0x{:02X}", d, k),
        // Data transfer
        Instruction::Mov { d, r } => format!("MOV R{}, R{}", d, r),
        Instruction::Movw { d, r } => format!("MOVW R{}:R{}, R{}:R{}", d + 1, d, r + 1, r),
        Instruction::Ldi { d, k } => format!("LDI R{}, 0x{:02X}", d, k),
        Instruction::Lds { d, k } => format!("LDS R{}, 0x{:04X}", d, k),
        Instruction::Sts { k, r } => format!("STS 0x{:04X}, R{}", k, r),
        Instruction::LdX { d } => format!("LD R{}, X", d),
        Instruction::LdXInc { d } => format!("LD R{}, X+", d),
        Instruction::LdXDec { d } => format!("LD R{}, -X", d),
        Instruction::LdY { d } => format!("LD R{}, Y", d),
        Instruction::LdYInc { d } => format!("LD R{}, Y+", d),
        Instruction::LdYDec { d } => format!("LD R{}, -Y", d),
        Instruction::LdYQ { d, q } => format!("LDD R{}, Y+{}", d, q),
        Instruction::LdZ { d } => format!("LD R{}, Z", d),
        Instruction::LdZInc { d } => format!("LD R{}, Z+", d),
        Instruction::LdZDec { d } => format!("LD R{}, -Z", d),
        Instruction::LdZQ { d, q } => format!("LDD R{}, Z+{}", d, q),
        Instruction::StX { r } => format!("ST X, R{}", r),
        Instruction::StXInc { r } => format!("ST X+, R{}", r),
        Instruction::StXDec { r } => format!("ST -X, R{}", r),
        Instruction::StY { r } => format!("ST Y, R{}", r),
        Instruction::StYInc { r } => format!("ST Y+, R{}", r),
        Instruction::StYDec { r } => format!("ST -Y, R{}", r),
        Instruction::StYQ { r, q } => format!("STD Y+{}, R{}", q, r),
        Instruction::StZ { r } => format!("ST Z, R{}", r),
        Instruction::StZInc { r } => format!("ST Z+, R{}", r),
        Instruction::StZDec { r } => format!("ST -Z, R{}", r),
        Instruction::StZQ { r, q } => format!("STD Z+{}, R{}", q, r),
        // Stack
        Instruction::Push { r } => format!("PUSH R{}", r),
        Instruction::Pop { d } => format!("POP R{}", d),
        // Shift/Bit
        Instruction::Lsr { d } => format!("LSR R{}", d),
        Instruction::Asr { d } => format!("ASR R{}", d),
        Instruction::Ror { d } => format!("ROR R{}", d),
        Instruction::Swap { d } => format!("SWAP R{}", d),
        Instruction::Bst { d, b } => format!("BST R{}, {}", d, b),
        Instruction::Bld { d, b } => format!("BLD R{}, {}", d, b),
        Instruction::Sbi { a, b } => format!("SBI 0x{:02X}, {}", a, b),
        Instruction::Cbi { a, b } => format!("CBI 0x{:02X}, {}", a, b),
        // Branch
        Instruction::Rjmp { k } => {
            let target = (pc as i32 + 1 + k as i32) as u16;
            format!("RJMP .{:+} ; 0x{:04X}", k, target * 2)
        }
        Instruction::Rcall { k } => {
            let target = (pc as i32 + 1 + k as i32) as u16;
            format!("RCALL .{:+} ; 0x{:04X}", k, target * 2)
        }
        Instruction::Ret => "RET".into(),
        Instruction::Reti => "RETI".into(),
        Instruction::Jmp { k } => format!("JMP 0x{:06X}", k * 2),
        Instruction::Call { k } => format!("CALL 0x{:06X}", k * 2),
        Instruction::Ijmp => "IJMP".into(),
        Instruction::Icall => "ICALL".into(),
        Instruction::Eijmp => "EIJMP".into(),
        Instruction::Eicall => "EICALL".into(),
        Instruction::Cpse { d, r } => format!("CPSE R{}, R{}", d, r),
        Instruction::Sbrc { r, b } => format!("SBRC R{}, {}", r, b),
        Instruction::Sbrs { r, b } => format!("SBRS R{}, {}", r, b),
        Instruction::Sbic { a, b } => format!("SBIC 0x{:02X}, {}", a, b),
        Instruction::Sbis { a, b } => format!("SBIS 0x{:02X}, {}", a, b),
        Instruction::Brbs { s, k } => {
            let target = (pc as i32 + 1 + k as i32) as u16;
            let name = match s {
                0 => "BRCS",
                1 => "BREQ",
                2 => "BRMI",
                3 => "BRVS",
                4 => "BRLT",
                5 => "BRHS",
                6 => "BRTS",
                7 => "BRIE",
                _ => "BRBS",
            };
            format!("{} .{:+} ; 0x{:04X}", name, k, target * 2)
        }
        Instruction::Brbc { s, k } => {
            let target = (pc as i32 + 1 + k as i32) as u16;
            let name = match s {
                0 => "BRCC",
                1 => "BRNE",
                2 => "BRPL",
                3 => "BRVC",
                4 => "BRGE",
                5 => "BRHC",
                6 => "BRTC",
                7 => "BRID",
                _ => "BRBC",
            };
            format!("{} .{:+} ; 0x{:04X}", name, k, target * 2)
        }
        // I/O
        Instruction::In { d, a } => format!("IN R{}, 0x{:02X}", d, a),
        Instruction::Out { a, r } => format!("OUT 0x{:02X}, R{}", a, r),
        // LPM
        Instruction::Lpm0 => "LPM R0, Z".into(),
        Instruction::LpmD { d } => format!("LPM R{}, Z", d),
        Instruction::LpmDInc { d } => format!("LPM R{}, Z+", d),
        // ELPM
        Instruction::Elpm0 => "ELPM R0, Z".into(),
        Instruction::ElpmD { d } => format!("ELPM R{}, Z", d),
        Instruction::ElpmDInc { d } => format!("ELPM R{}, Z+", d),
        // Status register
        Instruction::Sei => "SEI".into(),
        Instruction::Cli => "CLI".into(),
        Instruction::Sec => "SEC".into(),
        Instruction::Clc => "CLC".into(),
        Instruction::Sen => "SEN".into(),
        Instruction::Cln => "CLN".into(),
        Instruction::Sez => "SEZ".into(),
        Instruction::Clz => "CLZ".into(),
        Instruction::Sev => "SEV".into(),
        Instruction::Clv => "CLV".into(),
        Instruction::Ses => "SES".into(),
        Instruction::Cls => "CLS".into(),
        Instruction::Seh => "SEH".into(),
        Instruction::Clh => "CLH".into(),
        Instruction::Set => "SET".into(),
        Instruction::Clt => "CLT".into(),
        // Misc
        Instruction::Sleep => "SLEEP".into(),
        Instruction::Wdr => "WDR".into(),
        Instruction::Break => "BREAK".into(),
        Instruction::Spm => "SPM".into(),
        Instruction::Unknown(w) => format!(".dw 0x{:04X}", w),
    }
}

/// Format the SREG byte as a flag string like "ithsvnzc" (lowercase=clear, UPPER=set).
pub fn format_sreg(sreg: u8) -> String {
    let flags = ['I', 'T', 'H', 'S', 'V', 'N', 'Z', 'C'];
    let mut s = String::with_capacity(8);
    for (i, &f) in flags.iter().enumerate() {
        let bit = 7 - i;
        if sreg & (1 << bit) != 0 {
            s.push(f);
        } else {
            s.push(f.to_ascii_lowercase());
        }
    }
    s
}

/// Disassemble a range of flash memory.
///
/// Returns lines of `"0xAAAA: OPCODE  MNEMONIC"` for the given byte-address range.
pub fn disassemble_range(flash: &[u8], start_byte: usize, end_byte: usize) -> Vec<String> {
    use crate::opcodes;
    let mut lines = Vec::new();
    let mut addr = start_byte & !1; // align to word
    while addr < end_byte && addr + 1 < flash.len() {
        let word = (flash[addr] as u16) | ((flash[addr + 1] as u16) << 8);
        let next = if addr + 3 < flash.len() {
            (flash[addr + 2] as u16) | ((flash[addr + 3] as u16) << 8)
        } else {
            0
        };
        let pc = (addr / 2) as u16;
        let (inst, size) = opcodes::decode(word, next);
        let asm = disassemble(inst, pc);
        if size == 2 {
            lines.push(format!(
                "0x{:04X}: {:04X} {:04X}  {}",
                addr, word, next, asm
            ));
            addr += 4;
        } else {
            lines.push(format!("0x{:04X}: {:04X}       {}", addr, word, asm));
            addr += 2;
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opcodes::Instruction;

    #[test]
    fn test_disasm_basic() {
        assert_eq!(disassemble(Instruction::Nop, 0), "NOP");
        assert_eq!(
            disassemble(Instruction::Add { d: 1, r: 2 }, 0),
            "ADD R1, R2"
        );
        assert_eq!(
            disassemble(Instruction::Ldi { d: 16, k: 0xFF }, 0),
            "LDI R16, 0xFF"
        );
    }

    #[test]
    fn test_disasm_branch() {
        // RJMP +2 at PC=0x10 → target = 0x11+2 = 0x13, byte addr 0x26
        let s = disassemble(Instruction::Rjmp { k: 2 }, 0x10);
        assert!(s.contains("RJMP"));
        assert!(s.contains("0x0026"));
    }

    #[test]
    fn test_format_sreg() {
        assert_eq!(format_sreg(0xFF), "ITHSVNZC");
        assert_eq!(format_sreg(0x00), "ithsvnzc");
        // 0x83 = 1000_0011 → bit7=I, bit1=Z, bit0=C
        assert_eq!(format_sreg(0x83), "IthsvnZC");
    }
}
