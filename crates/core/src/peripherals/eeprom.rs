//! EEPROM controller emulation.
//!
//! The ATmega32u4 has 1 KB of EEPROM accessible through registers:
//! EECR (0x3F), EEDR (0x40), EEARL (0x41), EEARH (0x42).
//! Actual read/write operations are handled in [`Arduboy::read_data`] and
//! [`Arduboy::write_data`] by intercepting EECR writes.

/// EEPROM control registers
/// EECR = 0x3F, EEDR = 0x40, EEARL = 0x41, EEARH = 0x42
/// Actual EEPROM data is in Memory::eeprom
/// Read/write hooks are handled in Arduboy::read_data/write_data

pub struct EepromCtrl;

impl EepromCtrl {
    pub fn new() -> Self {
        EepromCtrl
    }
    pub fn reset(&mut self) {}
}
