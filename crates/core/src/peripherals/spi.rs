//! SPI master controller emulation.
//!
//! Handles the SPCR, SPSR, and SPDR registers. When the game writes to SPDR,
//! the SPI transfer is considered instant (no clock-cycle delay). The
//! transfer-complete interrupt flag (SPIF) is set immediately so the game's
//! polling loop sees it on the next read.

use super::INT_SPI;

/// SPI addresses
const SPCR: u16 = 0x4C; // SPI Control Register
const SPSR: u16 = 0x4D; // SPI Status Register
const SPDR: u16 = 0x4E; // SPI Data Register

pub struct Spi {
    pub spif: bool,
    pub wcol: bool,
    pub spi2x: bool,
    pub spie: bool,
    pub spe: bool,
}

impl Spi {
    pub fn new() -> Self {
        Spi {
            spif: false,
            wcol: false,
            spi2x: false,
            spie: false,
            spe: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Spi::new();
    }

    /// Returns true if this addr is handled
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        match addr {
            SPCR => {
                self.spie = value & 0x80 != 0;
                self.spe = value & 0x40 != 0;
                true
            }
            SPSR => {
                self.spi2x = value & 1 != 0;
                true
            }
            SPDR => {
                // Data written to SPDR → triggers SPI transfer
                self.spif = true;
                true
            }
            _ => false,
        }
    }

    pub fn read(&self, addr: u16) -> Option<u8> {
        match addr {
            SPSR => Some(((self.spif as u8) << 7) | ((self.wcol as u8) << 6) | (self.spi2x as u8)),
            _ => None,
        }
    }

    pub fn check_interrupt(&mut self) -> Option<u16> {
        if self.spif && self.spie {
            self.spif = false;
            return Some(INT_SPI);
        }
        None
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::SpiState {
        crate::savestate::SpiState {
            spif: self.spif,
            wcol: self.wcol,
            spi2x: self.spi2x,
            spie: self.spie,
            spe: self.spe,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::SpiState) {
        self.spif = s.spif;
        self.wcol = s.wcol;
        self.spi2x = s.spi2x;
        self.spie = s.spie;
        self.spe = s.spe;
    }
}
