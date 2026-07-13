//! Analog-to-digital converter emulation.
//!
//! Returns pseudo-random values via xorshift PRNG to simulate noisy analog
//! readings. The ADSC (start conversion) bit in ADCSRA triggers a conversion;
//! the result is placed in ADCH:ADCL and ADSC is cleared to signal completion.
//! This allows `analogRead()` and `initRandomSeed()` to function correctly.

use super::INT_ADC;

/// ADC register addresses
const ADCL: u16 = 0x78;
const ADCH: u16 = 0x79;
const ADCSRA: u16 = 0x7A;

pub struct Adc {
    pub aden: bool,
    pub adsc: bool,
    pub adie: bool,
    pub adif: bool,
    pub adch: u8,
    pub adcl: u8,
}

impl Adc {
    pub fn new() -> Self {
        Adc {
            aden: false,
            adsc: false,
            adie: false,
            adif: false,
            adch: 0,
            adcl: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Adc::new();
    }

    /// Returns true if addr was handled
    pub fn write(&mut self, addr: u16, value: u8, rng: &mut u32) -> bool {
        if addr == ADCSRA {
            self.aden = value & 0x80 != 0;
            self.adsc = value & 0x40 != 0;
            self.adie = value & 0x08 != 0;
            self.adif = value & 0x10 != 0;
            if self.aden && self.adsc {
                // Instant conversion with random result
                self.adch = xorshift(rng);
                self.adcl = xorshift(rng);
                self.adsc = false;
            }
            return true;
        }
        false
    }

    pub fn read(&self, addr: u16) -> Option<u8> {
        match addr {
            ADCSRA => {
                // Reconstruct ADCSRA register from internal state
                let mut val = 0u8;
                if self.aden {
                    val |= 0x80;
                }
                if self.adsc {
                    val |= 0x40;
                }
                if self.adif {
                    val |= 0x10;
                }
                if self.adie {
                    val |= 0x08;
                }
                Some(val)
            }
            ADCH => Some(self.adch),
            ADCL => Some(self.adcl),
            _ => None,
        }
    }

    pub fn update(&mut self, rng: &mut u32) {
        if self.aden && self.adie {
            self.adif = true;
            self.adsc = false;
            self.adch = xorshift(rng);
            self.adcl = xorshift(rng);
        }
    }

    pub fn check_interrupt(&mut self) -> Option<u16> {
        if self.adif && self.adie {
            self.adif = false;
            return Some(INT_ADC);
        }
        None
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::AdcState {
        crate::savestate::AdcState {
            aden: self.aden,
            adsc: self.adsc,
            adie: self.adie,
            adif: self.adif,
            adch: self.adch,
            adcl: self.adcl,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::AdcState) {
        self.aden = s.aden;
        self.adsc = s.adsc;
        self.adie = s.adie;
        self.adif = s.adif;
        self.adch = s.adch;
        self.adcl = s.adcl;
    }
}

fn xorshift(state: &mut u32) -> u8 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state & 0xFF) as u8
}
