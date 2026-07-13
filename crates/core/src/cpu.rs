//! AVR CPU core for ATmega32u4.
//!
//! Implements the CPU state machine and instruction execution for 80+ AVR
//! instructions including arithmetic, logic, branches, load/store, I/O,
//! multiply, and bit manipulation. The execute loop runs on [`Arduboy`]
//! to access the full memory-mapped peripheral bus.
//!
//! Flag computation follows the ATmega32u4 datasheet exactly, including
//! the tricky carry-chain behavior of SBC/SBCI/CPC where the Z flag is
//! only cleared (never set) to support multi-byte comparisons.

use crate::memory::Memory;
use crate::opcodes::Instruction;
use crate::{Arduboy, SPH_ADDR, SPL_ADDR, SREG_ADDR};
use crate::{SREG_C, SREG_H, SREG_I, SREG_N, SREG_S, SREG_T, SREG_V, SREG_Z};

/// CPU state for ATmega32u4.
///
/// Contains the program counter, stack pointer, status register (SREG),
/// a cycle counter (`tick`), and sleep mode flag. Register file R0–R31
/// lives in [`Memory::data`] at offsets 0x00–0x1F.
pub struct Cpu {
    /// Program counter (word address, not byte address)
    pub pc: u16,
    /// Stack pointer (byte address in data space)
    pub sp: u16,
    /// Status register: I T H S V N Z C (bits 7..0)
    pub sreg: u8,
    /// Monotonic cycle counter (incremented by each instruction's cycle cost)
    pub tick: u64,
    /// True when SLEEP instruction has been executed (woken by interrupt)
    pub sleeping: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            pc: 0,
            sp: 0x0AFF,
            sreg: 0,
            tick: 0,
            sleeping: false,
        }
    }

    #[inline(always)]
    pub fn flag(&self, bit: u8) -> bool {
        self.sreg & (1 << bit) != 0
    }

    #[inline(always)]
    pub fn set_flag(&mut self, bit: u8, v: bool) {
        if v {
            self.sreg |= 1 << bit;
        } else {
            self.sreg &= !(1 << bit);
        }
    }
}

// --- Flag helpers ---

/// Write CPU SREG back to the memory-mapped I/O register (0x5F).
#[inline(always)]
pub fn sync_sreg(cpu: &Cpu, mem: &mut Memory) {
    mem.data[SREG_ADDR as usize] = cpu.sreg;
}

/// Compute SREG flags for ADD/ADC result using ATmega32u4 flag formulas.
pub fn flags_add(cpu: &mut Cpu, rd: u8, rr: u8, r: u8) {
    let r7 = (r >> 7) & 1;
    let rd7 = (rd >> 7) & 1;
    let rr7 = (rr >> 7) & 1;
    let r3 = (r >> 3) & 1;
    let rd3 = (rd >> 3) & 1;
    let rr3 = (rr >> 3) & 1;
    let h = (rd3 & rr3) | (rr3 & (r3 ^ 1)) | ((r3 ^ 1) & rd3);
    let v = (rd7 & rr7 & (r7 ^ 1)) | ((rd7 ^ 1) & (rr7 ^ 1) & r7);
    let n = r7;
    let z = if r == 0 { 1u8 } else { 0 };
    let c = (rd7 & rr7) | (rr7 & (r7 ^ 1)) | ((r7 ^ 1) & rd7);
    let s = n ^ v;
    cpu.sreg = (cpu.sreg & 0b1100_0000) | (h << 5) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
}

/// Compute SREG flags for SUB/SBC/CP/CPC result.
///
/// When `set_z` is false (SBC/SBCI/CPC), the Z flag is only cleared, never
/// set — this enables correct multi-byte comparison chains.
pub fn flags_sub(cpu: &mut Cpu, rd: u8, rr: u8, r: u8, set_z: bool) {
    let r7 = (r >> 7) & 1;
    let rd7 = (rd >> 7) & 1;
    let rr7 = (rr >> 7) & 1;
    let r3 = (r >> 3) & 1;
    let rd3 = (rd >> 3) & 1;
    let rr3 = (rr >> 3) & 1;
    let h = ((rd3 ^ 1) & rr3) | (rr3 & r3) | (r3 & (rd3 ^ 1));
    let v = (rd7 & (rr7 ^ 1) & (r7 ^ 1)) | ((rd7 ^ 1) & rr7 & r7);
    let n = r7;
    let c = ((rd7 ^ 1) & rr7) | (rr7 & r7) | (r7 & (rd7 ^ 1));
    let s = n ^ v;
    let old_it = cpu.sreg & 0b1100_0000;
    let z = if set_z {
        if r == 0 {
            1u8
        } else {
            0
        }
    } else {
        if r != 0 {
            0
        } else {
            (cpu.sreg >> 1) & 1
        }
    };
    cpu.sreg = old_it | (h << 5) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
}

/// Compute SREG flags for logic operations (AND, OR, EOR). V is always cleared.
pub fn flags_logic(cpu: &mut Cpu, r: u8) {
    let n = (r >> 7) & 1;
    let z = if r == 0 { 1u8 } else { 0 };
    let s = n; // V=0
    cpu.sreg = (cpu.sreg & 0b1110_0001) | (s << 4) | (n << 2) | (z << 1);
}

/// Skip the next instruction (for CPSE, SBRC, SBRS, SBIC, SBIS).
///
/// Advances PC by 1 or 2 depending on whether the next instruction is 32-bit.
pub fn skip_next(cpu: &mut Cpu, mem: &Memory) {
    let nw = mem.read_program_word(cpu.pc as usize);
    let is_32 = (nw & 0xFE0E == 0x940C)
        || (nw & 0xFE0E == 0x940E)
        || (nw & 0xFE0F == 0x9000)
        || (nw & 0xFE0F == 0x9200);
    cpu.pc = cpu.pc.wrapping_add(if is_32 { 2 } else { 1 });
}

// ---- Instruction execution on Arduboy ----

impl Arduboy {
    /// Execute a single decoded AVR instruction and return the cycle cost.
    ///
    /// The instruction is executed in the context of the full Arduboy system,
    /// allowing memory-mapped I/O writes to reach peripherals (SPI, timers, etc.)
    /// via [`write_data`](Self::write_data) and [`read_data`](Self::read_data).
    pub fn execute_inst(&mut self, inst: Instruction, size: u8) -> u8 {
        self.cpu.pc = self.cpu.pc.wrapping_add(size as u16);

        match inst {
            Instruction::Nop => 1,

            // -- Arithmetic --
            Instruction::Add { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                let res = rd.wrapping_add(rr);
                self.mem.set_reg(d, res);
                flags_add(&mut self.cpu, rd, rr, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Adc { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                let c = (self.cpu.sreg & 1) as u8;
                let res = rd.wrapping_add(rr).wrapping_add(c);
                self.mem.set_reg(d, res);
                flags_add(&mut self.cpu, rd, rr, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sub { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                let res = rd.wrapping_sub(rr);
                self.mem.set_reg(d, res);
                flags_sub(&mut self.cpu, rd, rr, res, true);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Subi { d, k } => {
                let rd = self.mem.reg(d);
                let res = rd.wrapping_sub(k);
                self.mem.set_reg(d, res);
                flags_sub(&mut self.cpu, rd, k, res, true);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sbc { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                let c = (self.cpu.sreg & 1) as u8;
                let res = rd.wrapping_sub(rr).wrapping_sub(c);
                self.mem.set_reg(d, res);
                // AVR flag formulas use original Rr, NOT Rr+C.
                // The result R already incorporates carry.
                flags_sub(&mut self.cpu, rd, rr, res, false);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sbci { d, k } => {
                let rd = self.mem.reg(d);
                let c = (self.cpu.sreg & 1) as u8;
                let res = rd.wrapping_sub(k).wrapping_sub(c);
                self.mem.set_reg(d, res);
                // AVR flag formulas use original K, NOT K+C.
                // The result R already incorporates carry.
                flags_sub(&mut self.cpu, rd, k, res, false);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::And { d, r } => {
                let res = self.mem.reg(d) & self.mem.reg(r);
                self.mem.set_reg(d, res);
                flags_logic(&mut self.cpu, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Andi { d, k } => {
                let res = self.mem.reg(d) & k;
                self.mem.set_reg(d, res);
                flags_logic(&mut self.cpu, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Or { d, r } => {
                let res = self.mem.reg(d) | self.mem.reg(r);
                self.mem.set_reg(d, res);
                flags_logic(&mut self.cpu, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Ori { d, k } => {
                let res = self.mem.reg(d) | k;
                self.mem.set_reg(d, res);
                flags_logic(&mut self.cpu, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Eor { d, r } => {
                let res = self.mem.reg(d) ^ self.mem.reg(r);
                self.mem.set_reg(d, res);
                flags_logic(&mut self.cpu, res);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Com { d } => {
                let res = !self.mem.reg(d);
                self.mem.set_reg(d, res);
                let n = (res >> 7) & 1;
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n;
                self.cpu.sreg = (self.cpu.sreg & 0b1100_0000) | (s << 4) | (n << 2) | (z << 1) | 1;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Neg { d } => {
                let rd = self.mem.reg(d);
                let res = 0u8.wrapping_sub(rd);
                self.mem.set_reg(d, res);
                flags_sub(&mut self.cpu, 0, rd, res, true);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Inc { d } => {
                let rd = self.mem.reg(d);
                let res = rd.wrapping_add(1);
                self.mem.set_reg(d, res);
                let n = (res >> 7) & 1;
                let v = if rd == 0x7F { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0001) | (s << 4) | (v << 3) | (n << 2) | (z << 1);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Dec { d } => {
                let rd = self.mem.reg(d);
                let res = rd.wrapping_sub(1);
                self.mem.set_reg(d, res);
                let n = (res >> 7) & 1;
                let v = if rd == 0x80 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0001) | (s << 4) | (v << 3) | (n << 2) | (z << 1);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Mul { d, r } => {
                let res = (self.mem.reg(d) as u16) * (self.mem.reg(r) as u16);
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Muls { d, r } => {
                let res = ((self.mem.reg(d) as i8 as i16) * (self.mem.reg(r) as i8 as i16)) as u16;
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Mulsu { d, r } => {
                let res = ((self.mem.reg(d) as i8 as i16) * (self.mem.reg(r) as u8 as i16)) as u16;
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Fmul { d, r } => {
                let res = ((self.mem.reg(d) as u16) * (self.mem.reg(r) as u16)) << 1;
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if (res & 0xFFFF) == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Fmuls { d, r } => {
                let res =
                    (((self.mem.reg(d) as i8 as i16) * (self.mem.reg(r) as i8 as i16)) << 1) as u16;
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Fmulsu { d, r } => {
                // Rd signed × Rr unsigned, result << 1
                let res = (((self.mem.reg(d) as i8 as i16) * (self.mem.reg(r) as i16)) << 1) as u16;
                self.mem.set_reg(0, res as u8);
                self.mem.set_reg(1, (res >> 8) as u8);
                let c = if res & 0x8000 != 0 { 1u8 } else { 0 };
                let z = if res == 0 { 1u8 } else { 0 };
                self.cpu.sreg = (self.cpu.sreg & 0b1111_1100) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Adiw { d, k } => {
                let pi = (d - 24) / 2;
                let val = self.mem.reg_pair(pi);
                let res = val.wrapping_add(k as u16);
                self.mem.set_reg_pair(pi, res);
                let rdh7 = (val >> 15) as u8;
                let r15 = (res >> 15) as u8;
                let v = (rdh7 ^ 1) & r15;
                let n = r15;
                let z = if res == 0 { 1u8 } else { 0 };
                let c = (r15 ^ 1) & rdh7;
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0000) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }
            Instruction::Sbiw { d, k } => {
                let pi = (d - 24) / 2;
                let val = self.mem.reg_pair(pi);
                let res = val.wrapping_sub(k as u16);
                self.mem.set_reg_pair(pi, res);
                let rdh7 = (val >> 15) as u8;
                let r15 = (res >> 15) as u8;
                let v = rdh7 & (r15 ^ 1);
                let n = r15;
                let z = if res == 0 { 1u8 } else { 0 };
                let c = r15 & (rdh7 ^ 1);
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0000) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                2
            }

            // -- Compare --
            Instruction::Cp { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                flags_sub(&mut self.cpu, rd, rr, rd.wrapping_sub(rr), true);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Cpc { d, r } => {
                let rd = self.mem.reg(d);
                let rr = self.mem.reg(r);
                let c = (self.cpu.sreg & 1) as u8;
                let res = rd.wrapping_sub(rr).wrapping_sub(c);
                flags_sub(&mut self.cpu, rd, rr, res, false);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Cpi { d, k } => {
                let rd = self.mem.reg(d);
                flags_sub(&mut self.cpu, rd, k, rd.wrapping_sub(k), true);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }

            // -- Data Transfer --
            Instruction::Mov { d, r } => {
                let v = self.mem.reg(r);
                self.mem.set_reg(d, v);
                1
            }
            Instruction::Movw { d, r } => {
                let lo = self.mem.reg(r);
                let hi = self.mem.reg(r + 1);
                self.mem.set_reg(d, lo);
                self.mem.set_reg(d + 1, hi);
                1
            }
            Instruction::Ldi { d, k } => {
                self.mem.set_reg(d, k);
                1
            }
            Instruction::Lds { d, k } => {
                let v = self.read_data(k);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::Sts { k, r } => {
                let v = self.mem.reg(r);
                self.write_data(k, v);
                2
            }

            // LD X
            Instruction::LdX { d } => {
                let a = self.mem.x();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::LdXInc { d } => {
                let a = self.mem.x();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                self.mem.set_x(a.wrapping_add(1));
                2
            }
            Instruction::LdXDec { d } => {
                let a = self.mem.x().wrapping_sub(1);
                self.mem.set_x(a);
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            // LD Y
            Instruction::LdY { d } => {
                let a = self.mem.y();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::LdYInc { d } => {
                let a = self.mem.y();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                self.mem.set_y(a.wrapping_add(1));
                2
            }
            Instruction::LdYDec { d } => {
                let a = self.mem.y().wrapping_sub(1);
                self.mem.set_y(a);
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::LdYQ { d, q } => {
                let a = self.mem.y().wrapping_add(q as u16);
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            // LD Z
            Instruction::LdZ { d } => {
                let a = self.mem.z();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::LdZInc { d } => {
                let a = self.mem.z();
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                self.mem.set_z(a.wrapping_add(1));
                2
            }
            Instruction::LdZDec { d } => {
                let a = self.mem.z().wrapping_sub(1);
                self.mem.set_z(a);
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            Instruction::LdZQ { d, q } => {
                let a = self.mem.z().wrapping_add(q as u16);
                let v = self.read_data(a);
                self.mem.set_reg(d, v);
                2
            }
            // ST X
            Instruction::StX { r } => {
                let a = self.mem.x();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            Instruction::StXInc { r } => {
                let a = self.mem.x();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                self.mem.set_x(a.wrapping_add(1));
                2
            }
            Instruction::StXDec { r } => {
                let a = self.mem.x().wrapping_sub(1);
                self.mem.set_x(a);
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            // ST Y
            Instruction::StY { r } => {
                let a = self.mem.y();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            Instruction::StYInc { r } => {
                let a = self.mem.y();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                self.mem.set_y(a.wrapping_add(1));
                2
            }
            Instruction::StYDec { r } => {
                let a = self.mem.y().wrapping_sub(1);
                self.mem.set_y(a);
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            Instruction::StYQ { r, q } => {
                let a = self.mem.y().wrapping_add(q as u16);
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            // ST Z
            Instruction::StZ { r } => {
                let a = self.mem.z();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            Instruction::StZInc { r } => {
                let a = self.mem.z();
                let v = self.mem.reg(r);
                self.write_data(a, v);
                self.mem.set_z(a.wrapping_add(1));
                2
            }
            Instruction::StZDec { r } => {
                let a = self.mem.z().wrapping_sub(1);
                self.mem.set_z(a);
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }
            Instruction::StZQ { r, q } => {
                let a = self.mem.z().wrapping_add(q as u16);
                let v = self.mem.reg(r);
                self.write_data(a, v);
                2
            }

            // -- Stack --
            Instruction::Push { r } => {
                let v = self.mem.reg(r);
                let sp = self.cpu.sp;
                self.mem.write_raw(sp, v);
                self.cpu.sp = sp.wrapping_sub(1);
                self.mem.data[SPH_ADDR as usize] = (self.cpu.sp >> 8) as u8;
                self.mem.data[SPL_ADDR as usize] = self.cpu.sp as u8;
                2
            }
            Instruction::Pop { d } => {
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
                self.mem.data[SPH_ADDR as usize] = (self.cpu.sp >> 8) as u8;
                self.mem.data[SPL_ADDR as usize] = self.cpu.sp as u8;
                let v = self.mem.read_raw(self.cpu.sp);
                self.mem.set_reg(d, v);
                2
            }

            // -- Shift/Bit --
            Instruction::Lsr { d } => {
                let rd = self.mem.reg(d);
                let res = rd >> 1;
                self.mem.set_reg(d, res);
                let c = rd & 1;
                let n = 0u8;
                let v = c;
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0000) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Asr { d } => {
                let rd = self.mem.reg(d);
                let res = ((rd as i8) >> 1) as u8;
                self.mem.set_reg(d, res);
                let c = rd & 1;
                let n = (res >> 7) & 1;
                let v = n ^ c;
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0000) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Ror { d } => {
                let rd = self.mem.reg(d);
                let old_c = self.cpu.sreg & 1;
                let res = (rd >> 1) | (old_c << 7);
                self.mem.set_reg(d, res);
                let c = rd & 1;
                let n = (res >> 7) & 1;
                let v = n ^ c;
                let z = if res == 0 { 1u8 } else { 0 };
                let s = n ^ v;
                self.cpu.sreg =
                    (self.cpu.sreg & 0b1110_0000) | (s << 4) | (v << 3) | (n << 2) | (z << 1) | c;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Swap { d } => {
                let rd = self.mem.reg(d);
                self.mem.set_reg(d, (rd >> 4) | (rd << 4));
                1
            }
            Instruction::Bst { d, b } => {
                let v = (self.mem.reg(d) >> b) & 1 != 0;
                self.cpu.set_flag(SREG_T, v);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Bld { d, b } => {
                let t = self.cpu.flag(SREG_T);
                let mut rd = self.mem.reg(d);
                if t {
                    rd |= 1 << b;
                } else {
                    rd &= !(1 << b);
                }
                self.mem.set_reg(d, rd);
                1
            }
            Instruction::Sbi { a, b } => {
                self.write_bit(a as u16, b, true);
                2
            }
            Instruction::Cbi { a, b } => {
                self.write_bit(a as u16, b, false);
                2
            }

            // -- Branch --
            Instruction::Rjmp { k } => {
                self.cpu.pc = (self.cpu.pc as i32 + k as i32) as u16;
                2
            }
            Instruction::Rcall { k } => {
                let ret = self.cpu.pc;
                self.push_word(ret);
                self.cpu.pc = (self.cpu.pc as i32 + k as i32) as u16;
                3
            }
            Instruction::Ret => {
                self.cpu.pc = self.pop_word();
                4
            }
            Instruction::Reti => {
                self.cpu.pc = self.pop_word();
                self.cpu.sreg |= 1 << SREG_I;
                sync_sreg(&self.cpu, &mut self.mem);
                4
            }
            Instruction::Jmp { k } => {
                self.cpu.pc = k as u16;
                3
            }
            Instruction::Call { k } => {
                let ret = self.cpu.pc;
                self.push_word(ret);
                self.cpu.pc = k as u16;
                4
            }
            Instruction::Ijmp => {
                self.cpu.pc = self.mem.z();
                2
            }
            Instruction::Icall => {
                let ret = self.cpu.pc;
                self.push_word(ret);
                self.cpu.pc = self.mem.z();
                3
            }
            Instruction::Eijmp => {
                // PC ← EIND:Z — EIND only matters for >128KB flash (not applicable here)
                let z = self.mem.z();
                self.cpu.pc = z;
                2
            }
            Instruction::Eicall => {
                let ret = self.cpu.pc;
                self.push_word(ret);
                let z = self.mem.z();
                self.cpu.pc = z;
                4
            }
            Instruction::Cpse { d, r } => {
                if self.mem.reg(d) == self.mem.reg(r) {
                    skip_next(&mut self.cpu, &self.mem);
                    return 2;
                }
                1
            }
            Instruction::Sbrc { r, b } => {
                if self.mem.reg(r) & (1 << b) == 0 {
                    skip_next(&mut self.cpu, &self.mem);
                    return 2;
                }
                1
            }
            Instruction::Sbrs { r, b } => {
                if self.mem.reg(r) & (1 << b) != 0 {
                    skip_next(&mut self.cpu, &self.mem);
                    return 2;
                }
                1
            }
            Instruction::Sbic { a, b } => {
                let v = self.read_data(a as u16);
                if v & (1 << b) == 0 {
                    skip_next(&mut self.cpu, &self.mem);
                    return 2;
                }
                1
            }
            Instruction::Sbis { a, b } => {
                let v = self.read_data(a as u16);
                if v & (1 << b) != 0 {
                    skip_next(&mut self.cpu, &self.mem);
                    return 2;
                }
                1
            }
            Instruction::Brbs { s, k } => {
                if self.cpu.sreg & (1 << s) != 0 {
                    self.cpu.pc = (self.cpu.pc as i32 + k as i32) as u16;
                    return 2;
                }
                1
            }
            Instruction::Brbc { s, k } => {
                if self.cpu.sreg & (1 << s) == 0 {
                    self.cpu.pc = (self.cpu.pc as i32 + k as i32) as u16;
                    return 2;
                }
                1
            }

            // -- I/O --
            Instruction::In { d, a } => {
                // Note: decoder already converts I/O addr to data space (adds 0x20)
                let v = self.read_data(a as u16);
                self.mem.set_reg(d, v);
                1
            }
            Instruction::Out { a, r } => {
                let v = self.mem.reg(r);
                let addr = a as u16; // decoder already in data space
                self.write_data(addr, v);
                if addr == SREG_ADDR {
                    self.cpu.sreg = v;
                } else if addr == SPH_ADDR {
                    self.cpu.sp = (self.cpu.sp & 0x00FF) | ((v as u16) << 8);
                } else if addr == SPL_ADDR {
                    self.cpu.sp = (self.cpu.sp & 0xFF00) | v as u16;
                }
                1
            }

            // -- LPM --
            Instruction::Lpm0 => {
                let z = self.mem.z();
                let v = self.mem.read_flash_byte(z as usize);
                self.mem.set_reg(0, v);
                3
            }
            Instruction::LpmD { d } => {
                let z = self.mem.z();
                let v = self.mem.read_flash_byte(z as usize);
                self.mem.set_reg(d, v);
                3
            }
            Instruction::LpmDInc { d } => {
                let z = self.mem.z();
                let v = self.mem.read_flash_byte(z as usize);
                self.mem.set_reg(d, v);
                self.mem.set_z(z.wrapping_add(1));
                3
            }

            // -- ELPM (Extended LPM: RAMPZ:Z → flash) --
            Instruction::Elpm0 => {
                let rampz = self.mem.data[0x5B] as u32;
                let z = self.mem.z() as u32;
                let addr = (rampz << 16) | z;
                let v = self.mem.read_flash_byte(addr as usize);
                self.mem.set_reg(0, v);
                3
            }
            Instruction::ElpmD { d } => {
                let rampz = self.mem.data[0x5B] as u32;
                let z = self.mem.z() as u32;
                let addr = (rampz << 16) | z;
                let v = self.mem.read_flash_byte(addr as usize);
                self.mem.set_reg(d, v);
                3
            }
            Instruction::ElpmDInc { d } => {
                let rampz = self.mem.data[0x5B] as u32;
                let z = self.mem.z() as u32;
                let addr = (rampz << 16) | z;
                let v = self.mem.read_flash_byte(addr as usize);
                self.mem.set_reg(d, v);
                let new_z = z.wrapping_add(1);
                self.mem.set_z(new_z as u16);
                // Update RAMPZ on overflow
                if new_z & 0x10000 != 0 {
                    self.mem.data[0x5B] = self.mem.data[0x5B].wrapping_add(1);
                }
                3
            }

            // -- Status flags --
            Instruction::Sei => {
                self.cpu.sreg |= 1 << SREG_I;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Cli => {
                self.cpu.sreg &= !(1 << SREG_I);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sec => {
                self.cpu.sreg |= 1 << SREG_C;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Clc => {
                self.cpu.sreg &= !(1 << SREG_C);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sen => {
                self.cpu.sreg |= 1 << SREG_N;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Cln => {
                self.cpu.sreg &= !(1 << SREG_N);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sez => {
                self.cpu.sreg |= 1 << SREG_Z;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Clz => {
                self.cpu.sreg &= !(1 << SREG_Z);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Sev => {
                self.cpu.sreg |= 1 << SREG_V;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Clv => {
                self.cpu.sreg &= !(1 << SREG_V);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Ses => {
                self.cpu.sreg |= 1 << SREG_S;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Cls => {
                self.cpu.sreg &= !(1 << SREG_S);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Seh => {
                self.cpu.sreg |= 1 << SREG_H;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Clh => {
                self.cpu.sreg &= !(1 << SREG_H);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Set => {
                self.cpu.sreg |= 1 << SREG_T;
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }
            Instruction::Clt => {
                self.cpu.sreg &= !(1 << SREG_T);
                sync_sreg(&self.cpu, &mut self.mem);
                1
            }

            // -- Misc --
            Instruction::Sleep => {
                self.cpu.sleeping = true;
                1
            }
            Instruction::Wdr => 1,
            Instruction::Break => {
                // Debug break — trigger breakpoint_hit
                self.breakpoint_hit = true;
                1
            }
            Instruction::Spm => {
                // Store Program Memory — NOP in emulator (bootloader only)
                1
            }
            Instruction::Unknown(w) => {
                if self.debug {
                    eprintln!(
                        "UNKNOWN OPCODE 0x{:04X} at pc=0x{:04X}",
                        w,
                        self.cpu.pc.wrapping_sub(1)
                    );
                }
                1
            }
        }
    }

    /// Push a 16-bit word onto the stack (high byte at higher addr)
    fn push_word(&mut self, val: u16) {
        self.mem.write_raw(self.cpu.sp, (val >> 8) as u8);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.mem.write_raw(self.cpu.sp, val as u8);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.mem.data[SPH_ADDR as usize] = (self.cpu.sp >> 8) as u8;
        self.mem.data[SPL_ADDR as usize] = self.cpu.sp as u8;
    }

    /// Pop a 16-bit word from the stack
    fn pop_word(&mut self) -> u16 {
        self.cpu.sp = self.cpu.sp.wrapping_add(1);
        let lo = self.mem.read_raw(self.cpu.sp);
        self.cpu.sp = self.cpu.sp.wrapping_add(1);
        let hi = self.mem.read_raw(self.cpu.sp);
        self.mem.data[SPH_ADDR as usize] = (self.cpu.sp >> 8) as u8;
        self.mem.data[SPL_ADDR as usize] = self.cpu.sp as u8;
        (hi as u16) << 8 | lo as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opcodes::Instruction;
    use crate::Arduboy;

    #[test]
    fn test_add() {
        let mut a = Arduboy::new();
        a.mem.set_reg(0, 10);
        a.mem.set_reg(1, 20);
        a.execute_inst(Instruction::Add { d: 0, r: 1 }, 1);
        assert_eq!(a.mem.reg(0), 30);
    }

    #[test]
    fn test_add_overflow() {
        let mut a = Arduboy::new();
        a.mem.set_reg(0, 200);
        a.mem.set_reg(1, 100);
        a.execute_inst(Instruction::Add { d: 0, r: 1 }, 1);
        assert_eq!(a.mem.reg(0), 44);
        assert!(a.cpu.flag(SREG_C));
    }

    #[test]
    fn test_sub() {
        let mut a = Arduboy::new();
        a.mem.set_reg(0, 30);
        a.mem.set_reg(1, 20);
        a.execute_inst(Instruction::Sub { d: 0, r: 1 }, 1);
        assert_eq!(a.mem.reg(0), 10);
        assert!(!a.cpu.flag(SREG_C));
    }

    #[test]
    fn test_push_pop() {
        let mut a = Arduboy::new();
        let sp0 = a.cpu.sp;
        a.mem.set_reg(5, 0x42);
        a.execute_inst(Instruction::Push { r: 5 }, 1);
        assert_eq!(a.cpu.sp, sp0 - 1);
        a.execute_inst(Instruction::Pop { d: 10 }, 1);
        assert_eq!(a.cpu.sp, sp0);
        assert_eq!(a.mem.reg(10), 0x42);
    }

    #[test]
    fn test_rcall_ret() {
        let mut a = Arduboy::new();
        a.cpu.pc = 0x100;
        a.execute_inst(Instruction::Rcall { k: 5 }, 1);
        assert_eq!(a.cpu.pc, 0x106); // 0x100+1+5
        a.execute_inst(Instruction::Ret, 1);
        assert_eq!(a.cpu.pc, 0x101);
    }

    #[test]
    fn test_branch_taken() {
        let mut a = Arduboy::new();
        a.cpu.pc = 0x50;
        a.cpu.sreg |= 1 << SREG_Z;
        let c = a.execute_inst(Instruction::Brbs { s: SREG_Z, k: 3 }, 1);
        assert_eq!(c, 2);
        assert_eq!(a.cpu.pc, 0x54);
    }

    #[test]
    fn test_branch_not_taken() {
        let mut a = Arduboy::new();
        a.cpu.pc = 0x50;
        a.cpu.sreg &= !(1 << SREG_Z);
        let c = a.execute_inst(Instruction::Brbs { s: SREG_Z, k: 3 }, 1);
        assert_eq!(c, 1);
        assert_eq!(a.cpu.pc, 0x51);
    }

    #[test]
    fn test_lpm() {
        let mut a = Arduboy::new();
        a.mem.flash[0x100] = 0x42;
        a.mem.set_z(0x100);
        a.execute_inst(Instruction::LpmD { d: 5 }, 1);
        assert_eq!(a.mem.reg(5), 0x42);
    }

    #[test]
    fn test_mul() {
        let mut a = Arduboy::new();
        a.mem.set_reg(2, 10);
        a.mem.set_reg(3, 20);
        a.execute_inst(Instruction::Mul { d: 2, r: 3 }, 1);
        assert_eq!(a.mem.reg(0), 0xC8); // 200 lo
        assert_eq!(a.mem.reg(1), 0x00); // 200 hi
    }

    #[test]
    fn test_adiw() {
        let mut a = Arduboy::new();
        a.mem.set_z(0x1000);
        a.execute_inst(Instruction::Adiw { d: 30, k: 5 }, 1);
        assert_eq!(a.mem.z(), 0x1005);
    }

    #[test]
    fn test_io_in_out() {
        let mut a = Arduboy::new();
        a.mem.set_reg(16, 0x42);
        // SREG is at data space 0x5F (decoder adds 0x20 to I/O addr 0x3F)
        a.execute_inst(Instruction::Out { a: 0x5F, r: 16 }, 1);
        assert_eq!(a.cpu.sreg, 0x42);
        a.execute_inst(Instruction::In { d: 17, a: 0x5F }, 1);
        assert_eq!(a.mem.reg(17), 0x42);
    }

    #[test]
    fn test_sbci_carry_propagation() {
        // Test: 32-bit increment via SUBI/SBCI chain
        // r24:r25:r26:r27 = 0x000000FF
        // SUBI r24, 0xFF  (subtract -1 = add 1)
        // SBCI r25, 0xFF
        // SBCI r26, 0xFF
        // SBCI r27, 0xFF
        // Expected result: 0x00000100
        let mut a = Arduboy::new();
        a.mem.set_reg(24, 0xFF);
        a.mem.set_reg(25, 0x00);
        a.mem.set_reg(26, 0x00);
        a.mem.set_reg(27, 0x00);
        a.execute_inst(Instruction::Subi { d: 24, k: 0xFF }, 1);
        a.execute_inst(Instruction::Sbci { d: 25, k: 0xFF }, 1);
        a.execute_inst(Instruction::Sbci { d: 26, k: 0xFF }, 1);
        a.execute_inst(Instruction::Sbci { d: 27, k: 0xFF }, 1);
        assert_eq!(a.mem.reg(24), 0x00); // 0xFF - 0xFF = 0, C=0
        assert_eq!(a.mem.reg(25), 0x01); // 0x00 - 0xFF - 0 = 0x01, C=1
        assert_eq!(a.mem.reg(26), 0x00); // 0x00 - 0xFF - 1 = 0x00, C=1
        assert_eq!(a.mem.reg(27), 0x00); // 0x00 - 0xFF - 1 = 0x00, C=1
    }

    #[test]
    fn test_cpc_16bit_compare() {
        // Compare 0x0100 vs 0x00FF (should be greater)
        let mut a = Arduboy::new();
        a.mem.set_reg(20, 0x00); // low byte of 0x0100
        a.mem.set_reg(21, 0x01); // high byte of 0x0100
        a.mem.set_reg(22, 0xFF); // low byte of 0x00FF
        a.mem.set_reg(23, 0x00); // high byte of 0x00FF
        a.execute_inst(Instruction::Cp { d: 20, r: 22 }, 1);
        a.execute_inst(Instruction::Cpc { d: 21, r: 23 }, 1);
        // 0x0100 > 0x00FF, so C should be 0 (no borrow)
        assert_eq!(a.cpu.sreg & 1, 0, "C flag should be clear: 0x0100 > 0x00FF");
    }
}
