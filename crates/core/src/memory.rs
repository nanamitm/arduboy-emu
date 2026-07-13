//! AVR memory subsystem.
//!
//! The memory model follows the AVR unified data-space layout:
//!
//! | Address Range | Content             |
//! |---------------|---------------------|
//! | 0x0000–0x001F | General registers R0–R31 |
//! | 0x0020–0x00FF | I/O + extended I/O registers |
//! | 0x0100+       | SRAM (2560 bytes on 32u4, 2048 bytes on 328P) |
//!
//! Flash (32 KB) and EEPROM (1 KB) are separate address spaces.

use crate::{DATA_SIZE, EEPROM_SIZE, FLASH_SIZE};

/// AVR memory model containing data space, flash, and EEPROM.
pub struct Memory {
    /// Unified data space: registers (0x00-0x1F) + I/O (0x20-0xFF) + SRAM (0x100+)
    pub data: Vec<u8>,
    /// Program memory (flash)
    pub flash: Vec<u8>,
    /// EEPROM
    pub eeprom: Vec<u8>,
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            data: vec![0u8; DATA_SIZE],
            flash: vec![0u8; FLASH_SIZE],
            eeprom: vec![0xFFu8; EEPROM_SIZE],
        }
    }

    /// Create memory with a specific data-space size (REG + IO + SRAM).
    pub fn new_with_size(data_size: usize) -> Self {
        Memory {
            data: vec![0u8; data_size],
            flash: vec![0u8; FLASH_SIZE],
            eeprom: vec![0xFFu8; EEPROM_SIZE],
        }
    }

    // --- Register access ---

    #[inline(always)]
    pub fn reg(&self, r: u8) -> u8 {
        self.data[r as usize]
    }

    #[inline(always)]
    pub fn set_reg(&mut self, r: u8, v: u8) {
        self.data[r as usize] = v;
    }

    /// Read 16-bit register pair (little-endian: low reg first)
    /// pair 0=W(R24:R25), 1=X(R26:R27), 2=Y(R28:R29), 3=Z(R30:R31)
    #[inline(always)]
    pub fn reg_pair(&self, pair: u8) -> u16 {
        let base = 24 + (pair as usize) * 2;
        self.data[base] as u16 | ((self.data[base + 1] as u16) << 8)
    }

    /// Write 16-bit register pair
    #[inline(always)]
    pub fn set_reg_pair(&mut self, pair: u8, v: u16) {
        let base = 24 + (pair as usize) * 2;
        self.data[base] = v as u8;
        self.data[base + 1] = (v >> 8) as u8;
    }

    /// Read X register (R26:R27)
    #[inline(always)]
    pub fn x(&self) -> u16 {
        self.data[26] as u16 | ((self.data[27] as u16) << 8)
    }

    /// Read Y register (R28:R29)
    #[inline(always)]
    pub fn y(&self) -> u16 {
        self.data[28] as u16 | ((self.data[29] as u16) << 8)
    }

    /// Read Z register (R30:R31)
    #[inline(always)]
    pub fn z(&self) -> u16 {
        self.data[30] as u16 | ((self.data[31] as u16) << 8)
    }

    /// Write X register
    #[inline(always)]
    pub fn set_x(&mut self, v: u16) {
        self.data[26] = v as u8;
        self.data[27] = (v >> 8) as u8;
    }

    /// Write Y register
    #[inline(always)]
    pub fn set_y(&mut self, v: u16) {
        self.data[28] = v as u8;
        self.data[29] = (v >> 8) as u8;
    }

    /// Write Z register
    #[inline(always)]
    pub fn set_z(&mut self, v: u16) {
        self.data[30] = v as u8;
        self.data[31] = (v >> 8) as u8;
    }

    // --- Program memory ---

    /// Read 16-bit word from flash at word address
    #[inline(always)]
    pub fn read_program_word(&self, word_addr: usize) -> u16 {
        let byte_addr = word_addr * 2;
        if byte_addr + 1 < self.flash.len() {
            self.flash[byte_addr] as u16 | ((self.flash[byte_addr + 1] as u16) << 8)
        } else {
            0
        }
    }

    /// Read single byte from flash at byte address
    #[inline(always)]
    pub fn read_flash_byte(&self, byte_addr: usize) -> u8 {
        if byte_addr < self.flash.len() {
            self.flash[byte_addr]
        } else {
            0
        }
    }

    // --- Data space ---

    #[inline(always)]
    pub fn read_raw(&self, addr: u16) -> u8 {
        let a = addr as usize;
        if a < self.data.len() {
            self.data[a]
        } else {
            0
        }
    }

    #[inline(always)]
    pub fn write_raw(&mut self, addr: u16, v: u8) {
        let a = addr as usize;
        if a < self.data.len() {
            self.data[a] = v;
        }
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_pair() {
        let mut mem = Memory::new();
        mem.set_z(0x1234);
        assert_eq!(mem.z(), 0x1234);
        assert_eq!(mem.data[30], 0x34);
        assert_eq!(mem.data[31], 0x12);
    }

    #[test]
    fn test_program_word() {
        let mut mem = Memory::new();
        mem.flash[0] = 0x0C;
        mem.flash[1] = 0x94;
        assert_eq!(mem.read_program_word(0), 0x940C);
    }
}
