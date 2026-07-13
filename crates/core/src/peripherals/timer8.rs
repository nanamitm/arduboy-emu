//! 8-bit Timer/Counter emulation.
//!
//! Supports Normal, CTC, and Fast PWM modes with prescalers (1/8/64/256/1024).
//! Handles overflow and compare-match interrupts. Used by the Arduino core
//! library for `millis()`, `micros()`, and `delay()` timing.
//!
//! Reusable for Timer0 (both ATmega32u4 and ATmega328P) and Timer2 (ATmega328P)
//! by providing appropriate register addresses and interrupt vectors.

#[derive(Debug, Clone)]
pub struct Timer8Addrs {
    pub tifr: u16,
    pub tccr_a: u16,
    pub tccr_b: u16,
    pub ocr_a: u16,
    pub ocr_b: u16,
    pub timsk: u16,
    pub tcnt: u16,
    /// Overflow interrupt vector (word address)
    pub int_ovf: u16,
    /// Compare match A interrupt vector (word address)
    pub int_compa: u16,
    /// Compare match B interrupt vector (word address)
    pub int_compb: u16,
    /// True for Timer2 (async timer with different prescaler table)
    pub is_timer2: bool,
}

pub struct Timer8 {
    addrs: Timer8Addrs,
    tick: u64,
    prescale: u32,
    cs: u8,
    mode: u8,
    // Waveform generation mode bits
    wgm00: bool,
    wgm01: bool,
    wgm02: bool,
    // Compare output mode
    com_a: u8,
    com_b: u8,
    ocr0a: u8,
    ocr0b: u8,
    tcnt_shadow: u8,
    // Interrupt flags
    tov0: u32,
    ocf0a: u32,
    ocf0b: u32,
    // Interrupt enable
    toie0: bool,
    ocie0a: bool,
    ocie0b: bool,
    // Debug counters
    pub dbg_ovf_count: u32,
    pub dbg_int_fire_count: u32,
}

impl Timer8 {
    pub fn new(addrs: Timer8Addrs) -> Self {
        Timer8 {
            addrs,
            tick: 0,
            prescale: 0,
            cs: 0,
            mode: 0,
            wgm00: false,
            wgm01: false,
            wgm02: false,
            com_a: 0,
            com_b: 0,
            ocr0a: 0,
            ocr0b: 0,
            tcnt_shadow: 0,
            tov0: 0,
            ocf0a: 0,
            ocf0b: 0,
            toie0: false,
            ocie0a: false,
            ocie0b: false,
            dbg_ovf_count: 0,
            dbg_int_fire_count: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Timer8::new(self.addrs.clone());
    }

    fn update_prescale(&mut self) {
        self.prescale = if self.addrs.is_timer2 {
            // Timer2 (ATmega328P async timer) — different prescaler table
            match self.cs {
                0 => 0,    // stopped
                1 => 1,    // clk/1
                2 => 8,    // clk/8
                3 => 32,   // clk/32
                4 => 64,   // clk/64
                5 => 128,  // clk/128
                6 => 256,  // clk/256
                7 => 1024, // clk/1024
                _ => 0,
            }
        } else {
            // Timer0 (and Timer2 on 32u4) — standard prescaler table
            match self.cs {
                0 => 0,
                1 => 1,
                2 => 8,
                3 => 64,
                4 => 256,
                5 => 1024,
                _ => 0, // external clock (not emulated)
            }
        };
        let wgm = ((self.wgm02 as u8) << 2) | ((self.wgm01 as u8) << 1) | (self.wgm00 as u8);
        self.mode = wgm;
    }

    /// Handle writes to timer registers. Returns true if addr was handled.
    pub fn write(&mut self, addr: u16, value: u8, _old: u8, data: &mut [u8]) -> bool {
        if addr == self.addrs.tifr {
            // Writing 1 to a TIFR bit CLEARS the interrupt flag
            if value & 1 != 0 {
                self.tov0 = 0;
            }
            if value & 2 != 0 {
                self.ocf0a = 0;
            }
            if value & 4 != 0 {
                self.ocf0b = 0;
            }
            return true;
        }
        if addr == self.addrs.tccr_a {
            self.wgm00 = value & 1 != 0;
            self.wgm01 = value & 2 != 0;
            self.com_b = (value >> 4) & 3;
            self.com_a = (value >> 6) & 3;
            self.update_prescale();
            data[addr as usize] = value;
            return true;
        }
        if addr == self.addrs.tccr_b {
            self.wgm02 = value & 8 != 0;
            self.cs = value & 7;
            self.update_prescale();
            data[addr as usize] = value;
            return true;
        }
        if addr == self.addrs.ocr_a {
            self.ocr0a = value;
            data[addr as usize] = value;
            return true;
        }
        if addr == self.addrs.ocr_b {
            self.ocr0b = value;
            data[addr as usize] = value;
            return true;
        }
        if addr == self.addrs.timsk {
            self.toie0 = value & 1 != 0;
            self.ocie0a = value & 2 != 0;
            self.ocie0b = value & 4 != 0;
            data[addr as usize] = value;
            return true;
        }
        if addr == self.addrs.tcnt {
            data[addr as usize] = value;
            self.tcnt_shadow = value;
            return true;
        }
        false
    }

    /// Handle reads from timer registers. Returns Some(value) if handled.
    pub fn read(&mut self, addr: u16, tick: u64, data: &[u8]) -> Option<u8> {
        if addr == self.addrs.tifr {
            return Some(
                ((self.tov0.min(1)) as u8)
                    | (((self.ocf0a.min(1)) as u8) << 1)
                    | (((self.ocf0b.min(1)) as u8) << 2),
            );
        }
        if addr == self.addrs.tcnt {
            self.do_update(tick, data);
            return Some(self.tcnt_shadow);
        }
        None
    }

    fn do_update(&mut self, tick: u64, _data: &[u8]) {
        if self.prescale == 0 {
            return;
        }
        let ticks_since = tick.wrapping_sub(self.tick);
        let interval = (ticks_since / self.prescale as u64) as u32;
        if interval == 0 {
            return;
        }

        let top = if self.mode == 2 || self.mode == 7 {
            if self.ocr0a > 0 {
                self.ocr0a as u32
            } else {
                0xFF
            }
        } else {
            0xFF
        };

        let new_cnt = self.tcnt_shadow as u32 + interval;
        self.tcnt_shadow = if top > 0 {
            (new_cnt % (top + 1)) as u8
        } else {
            new_cnt as u8
        };
    }

    /// Update timer state
    pub fn update(&mut self, tick: u64, data: &mut [u8]) {
        if self.prescale == 0 {
            return;
        }

        let ticks_since = tick.wrapping_sub(self.tick);
        let interval = (ticks_since / self.prescale as u64) as u32;
        if interval == 0 {
            return;
        }

        let old_cnt = self.tcnt_shadow as u32;
        let new_cnt = old_cnt + interval;

        // WGM mode determines TOP value
        let top = if self.mode == 2 || self.mode == 7 {
            // CTC mode (WGM=010) or Fast PWM with OCRA top (WGM=111)
            if self.ocr0a > 0 {
                self.ocr0a as u32
            } else {
                0xFF
            }
        } else {
            0xFF // Normal mode
        };

        // Count overflows/compare matches
        if top > 0 {
            let total_counts = new_cnt;
            let overflows = total_counts / (top + 1);
            let remainder = total_counts % (top + 1);

            if overflows > 0 {
                self.dbg_ovf_count += overflows;
                if self.mode == 2 || self.mode == 7 {
                    // CTC: compare match fires, TOV fires at MAX
                    self.ocf0a = self.ocf0a.saturating_add(overflows);
                }
                // TOV fires at MAX (0xFF overflow) in all modes except CTC-only
                // In modes 0, 3 (Normal, Fast PWM with TOP=MAX), TOV fires on overflow
                if self.mode != 2 {
                    self.tov0 = self.tov0.saturating_add(overflows);
                }
                // Check if we crossed compare match B
                if self.ocr0b > 0 && remainder >= self.ocr0b as u32 && old_cnt < self.ocr0b as u32 {
                    self.ocf0b = self.ocf0b.saturating_add(1);
                }
            } else {
                // No overflow but may cross compare match
                if old_cnt < self.ocr0a as u32 && new_cnt >= self.ocr0a as u32 && self.ocr0a > 0 {
                    self.ocf0a = self.ocf0a.saturating_add(1);
                }
                if old_cnt < self.ocr0b as u32 && new_cnt >= self.ocr0b as u32 && self.ocr0b > 0 {
                    self.ocf0b = self.ocf0b.saturating_add(1);
                }
            }

            self.tcnt_shadow = remainder as u8;
        } else {
            self.tcnt_shadow = new_cnt as u8;
        }

        data[self.addrs.tcnt as usize] = self.tcnt_shadow;
        self.tick += (interval as u64) * (self.prescale as u64);
    }

    /// Check for pending interrupts. Returns vector address if interrupt fires.
    ///
    /// Priority order matches ATmega328P datasheet: COMPA > COMPB > OVF.
    pub fn check_interrupt(&mut self) -> Option<u16> {
        if self.ocf0a > 0 && self.ocie0a {
            self.ocf0a = self.ocf0a.saturating_sub(1);
            self.dbg_int_fire_count += 1;
            return Some(self.addrs.int_compa);
        }
        if self.ocf0b > 0 && self.ocie0b {
            self.ocf0b = self.ocf0b.saturating_sub(1);
            self.dbg_int_fire_count += 1;
            return Some(self.addrs.int_compb);
        }
        if self.tov0 > 0 && self.toie0 {
            self.tov0 = self.tov0.saturating_sub(1);
            self.dbg_int_fire_count += 1;
            return Some(self.addrs.int_ovf);
        }
        None
    }

    pub fn dbg_info(&self) -> String {
        format!(
            "mode={} cs={} ps={} ocra={} ocie_a={} ocf_a={} toie={} tov={} cnt={} int_fires={}",
            self.mode,
            self.cs,
            self.prescale,
            self.ocr0a,
            self.ocie0a,
            self.ocf0a,
            self.toie0,
            self.tov0,
            self.tcnt_shadow,
            self.dbg_int_fire_count
        )
    }

    pub fn dbg_reset_counters(&mut self) {
        self.dbg_ovf_count = 0;
        self.dbg_int_fire_count = 0;
    }

    /// Calculate output frequency for CTC mode with toggle on compare match.
    ///
    /// Returns 0.0 if timer is stopped or not in CTC-toggle configuration.
    /// CTC mode (WGM=010) with COM_A=01 (toggle OC0A on match):
    /// f_out = f_clk / (2 * prescaler * (1 + OCR0A))
    pub fn get_tone_hz(&self, clock: u32) -> f32 {
        if self.prescale == 0 || self.com_a != 1 {
            return 0.0;
        }
        // CTC mode: WGM = 010
        if self.mode != 2 {
            return 0.0;
        }
        if self.ocr0a == 0 {
            return 0.0;
        }
        clock as f32 / (2.0 * self.prescale as f32 * (self.ocr0a as f32 + 1.0))
    }

    /// Check if Timer is outputting PWM on OC_B pin (PWM DAC mode).
    ///
    /// Returns true when COM_B is set (non-zero) in a PWM mode, indicating
    /// hardware PWM output on OC2B (PD3 for Timer2). Used for Gamebuino Classic
    /// sound where Timer1 ISR updates OCR2B to produce audio via PWM DAC.
    pub fn is_pwm_dac_active(&self) -> bool {
        self.com_b != 0
            && self.prescale > 0
            && (self.mode == 1 || self.mode == 3 || self.mode == 5 || self.mode == 7)
    }

    /// Get current OCR_B value (PWM DAC level, 0–255).
    pub fn ocr_b(&self) -> u8 {
        self.ocr0b
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::Timer8State {
        crate::savestate::Timer8State {
            tick: self.tick,
            prescale: self.prescale,
            cs: self.cs,
            mode: self.mode,
            wgm00: self.wgm00,
            wgm01: self.wgm01,
            wgm02: self.wgm02,
            com_a: self.com_a,
            com_b: self.com_b,
            ocr0a: self.ocr0a,
            ocr0b: self.ocr0b,
            tcnt_shadow: self.tcnt_shadow,
            tov0: self.tov0,
            ocf0a: self.ocf0a,
            ocf0b: self.ocf0b,
            toie0: self.toie0,
            ocie0a: self.ocie0a,
            ocie0b: self.ocie0b,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::Timer8State) {
        self.tick = s.tick;
        self.prescale = s.prescale;
        self.cs = s.cs;
        self.mode = s.mode;
        self.wgm00 = s.wgm00;
        self.wgm01 = s.wgm01;
        self.wgm02 = s.wgm02;
        self.com_a = s.com_a;
        self.com_b = s.com_b;
        self.ocr0a = s.ocr0a;
        self.ocr0b = s.ocr0b;
        self.tcnt_shadow = s.tcnt_shadow;
        self.tov0 = s.tov0;
        self.ocf0a = s.ocf0a;
        self.ocf0b = s.ocf0b;
        self.toie0 = s.toie0;
        self.ocie0a = s.ocie0a;
        self.ocie0b = s.ocie0b;
    }
}
