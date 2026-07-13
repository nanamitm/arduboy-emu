//! PLL frequency synthesizer emulation.
//!
//! The PLL (PLLCSR at 0x49) is used by the ATmega32u4 for USB and high-speed
//! timer clocking. When the game enables the PLL (PLLE=1), this emulation
//! immediately reports lock (PLOCK=1) since there is no real oscillator to wait for.

/// PLL Control register at 0x49
pub struct Pll {
    pub pindiv: bool,
    pub plle: bool,
    pub plock: bool,
}

impl Pll {
    pub fn new() -> Self {
        Pll {
            pindiv: false,
            plle: false,
            plock: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Pll::new();
    }

    pub fn read(&self) -> u8 {
        ((self.pindiv as u8) << 4) | ((self.plle as u8) << 1) | (self.plock as u8)
    }

    pub fn write(&mut self, value: u8) {
        self.pindiv = value & 0x10 != 0;
        self.plle = value & 0x02 != 0;
        self.plock = true; // PLL locks instantly in emulation
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::PllState {
        crate::savestate::PllState {
            pindiv: self.pindiv,
            plle: self.plle,
            plock: self.plock,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::PllState) {
        self.pindiv = s.pindiv;
        self.plle = s.plle;
        self.plock = s.plock;
    }
}
