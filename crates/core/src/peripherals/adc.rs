//! Analog-to-digital converter emulation.
//!
//! Returns pseudo-random values via xorshift PRNG to simulate noisy analog
//! readings. Setting ADSC in ADCSRA while the ADC is enabled (ADEN) starts a
//! conversion; [`update`](Adc::update) then completes it — placing the result in
//! ADCH:ADCL, clearing ADSC (so `while (ADCSRA & _BV(ADSC))` polling exits) and
//! setting ADIF for interrupt-driven use. Writing ADSC while ADEN is clear has
//! no effect (ADSC reads back 0), matching hardware. This lets `analogRead()`
//! and `initRandomSeed()` work in both polling and interrupt modes.

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
    pub fn write(&mut self, addr: u16, value: u8, _rng: &mut u32) -> bool {
        if addr == ADCSRA {
            self.aden = value & 0x80 != 0;
            self.adie = value & 0x08 != 0;
            // ADIF is cleared by writing a logical 1 to it.
            if value & 0x10 != 0 {
                self.adif = false;
            }
            // A conversion only starts when the ADC is enabled. Writing ADSC
            // with ADEN clear has no effect and ADSC reads back as 0. The
            // conversion itself completes in `update`.
            if self.aden {
                if value & 0x40 != 0 {
                    self.adsc = true;
                }
            } else {
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
        // Complete an in-progress conversion. Works for both polling (software
        // watches ADSC) and interrupt-driven (ADIF/ADIE) use.
        if self.aden && self.adsc {
            self.adch = xorshift(rng);
            self.adcl = xorshift(rng);
            self.adsc = false;
            self.adif = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn adsc_set(adc: &Adc) -> bool {
        adc.read(ADCSRA).unwrap() & 0x40 != 0
    }

    #[test]
    fn polling_conversion_completes_via_update() {
        let mut adc = Adc::new();
        let mut rng = 0x1234_5678;
        adc.write(ADCSRA, 0x80, &mut rng); // enable ADC
        adc.write(ADCSRA, 0xC0, &mut rng); // ADEN + ADSC: start conversion
        assert!(adsc_set(&adc), "ADSC should read 1 while converting");
        adc.update(&mut rng);
        assert!(!adsc_set(&adc), "ADSC should clear on completion");
        assert!(adc.adif, "ADIF should be set on completion");
    }

    #[test]
    fn adsc_without_aden_has_no_effect() {
        // Regression: a ROM writing ADCSRA = 0x40 (ADSC set, ADEN clear) must not
        // latch ADSC, else `while (ADCSRA & _BV(ADSC))` hangs forever.
        let mut adc = Adc::new();
        let mut rng = 0x1234_5678;
        adc.write(ADCSRA, 0x40, &mut rng);
        assert!(!adsc_set(&adc), "ADSC must read 0 when ADEN is clear");
        adc.update(&mut rng);
        assert!(!adsc_set(&adc));
    }

    #[test]
    fn interrupt_conversion_sets_adif_and_fires() {
        let mut adc = Adc::new();
        let mut rng = 0x1234_5678;
        adc.write(ADCSRA, 0x88, &mut rng); // ADEN + ADIE
        adc.write(ADCSRA, 0xC8, &mut rng); // + ADSC
        adc.update(&mut rng);
        assert_eq!(adc.check_interrupt(), Some(INT_ADC));
        assert_eq!(adc.check_interrupt(), None); // fires once
    }
}
