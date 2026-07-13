//! 10-bit high-speed Timer/Counter4 emulation (ATmega32u4).
//!
//! Timer4 is an enhanced 10-bit timer with asymmetric PWM, dead-time
//! generation, and optional PLL clocking (up to 64 MHz). On the Arduboy,
//! it is occasionally used for PWM audio output or RGB LED control.
//!
//! Supported modes: Normal, CTC (WGM=01 with OCR4C as TOP), Fast PWM.
//! Register addresses: TCCR4A(0xC0)..TCCR4E(0xC4), TCNT4(0xBE),
//! TC4H(0xBF), OCR4A-D, DT4, TIFR4(0x39), TIMSK4(0x72).

/// Timer4 10-bit high-speed timer
pub struct Timer4 {
    /// Internal counter (10-bit, 0..1023)
    tcnt: u16,
    /// High-byte temporary register (TC4H)
    tc4h: u8,
    /// Compare registers (10-bit)
    ocr_a: u16,
    ocr_b: u16,
    ocr_c: u16, // TOP in many PWM modes
    ocr_d: u16,
    /// Control registers
    tccr_a: u8,
    tccr_b: u8,
    tccr_c: u8,
    tccr_d: u8,
    tccr_e: u8,
    /// Dead time register
    dt4: u8,
    /// Interrupt mask (TIMSK4)
    timsk: u8,

    /// Clock select (CS43:CS40)
    cs: u8,
    /// Prescaler value derived from CS bits
    prescale: u32,
    /// Last update tick
    tick: u64,
    /// WGM mode (WGM41:WGM40 from TCCR4D)
    wgm: u8,
    /// Overflow interrupt pending count
    tov: u32,
    /// Compare A match pending count
    ocf_a: u32,
    /// Compare B match pending count
    ocf_b: u32,
    /// Compare D match pending count
    ocf_d: u32,
}

/// Timer4 interrupt vector (word address)
pub const INT_TIMER4_OVF: u16 = 0x0048;
pub const INT_TIMER4_COMPA: u16 = 0x0038;
pub const INT_TIMER4_COMPB: u16 = 0x003C;
pub const INT_TIMER4_COMPD: u16 = 0x003E;

impl Timer4 {
    pub fn new() -> Self {
        Timer4 {
            tcnt: 0,
            tc4h: 0,
            ocr_a: 0,
            ocr_b: 0,
            ocr_c: 0xFF,
            ocr_d: 0,
            tccr_a: 0,
            tccr_b: 0,
            tccr_c: 0,
            tccr_d: 0,
            tccr_e: 0,
            dt4: 0,
            timsk: 0,
            cs: 0,
            prescale: 0,
            tick: 0,
            wgm: 0,
            tov: 0,
            ocf_a: 0,
            ocf_b: 0,
            ocf_d: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Decode prescaler from CS43:CS40 bits.
    /// Timer4 has extended prescaler: /1, /2, /4, /8, ..., /16384
    fn decode_prescale(cs: u8) -> u32 {
        match cs {
            0 => 0, // stopped
            1 => 1,
            2 => 2,
            3 => 4,
            4 => 8,
            5 => 16,
            6 => 32,
            7 => 64,
            8 => 128,
            9 => 256,
            10 => 512,
            11 => 1024,
            12 => 2048,
            13 => 4096,
            14 => 8192,
            15 => 16384,
            _ => 0,
        }
    }

    /// Update timer based on elapsed CPU ticks
    pub fn update(&mut self, current_tick: u64, _data: &mut [u8]) {
        if self.prescale == 0 {
            return;
        }

        let elapsed = current_tick.saturating_sub(self.tick);
        let timer_ticks = elapsed / self.prescale as u64;
        if timer_ticks == 0 {
            return;
        }
        self.tick += timer_ticks * self.prescale as u64;

        let top = self.get_top();

        for _ in 0..timer_ticks.min(2048) {
            self.tcnt = self.tcnt.wrapping_add(1) & 0x3FF; // 10-bit

            // Compare matches
            if self.tcnt == self.ocr_a {
                self.ocf_a += 1;
            }
            if self.tcnt == self.ocr_b {
                self.ocf_b += 1;
            }
            if self.tcnt == self.ocr_d {
                self.ocf_d += 1;
            }

            // TOP / overflow
            if self.tcnt >= top {
                self.tcnt = 0;
                self.tov += 1;
            }
        }
    }

    /// Get TOP value based on WGM mode
    fn get_top(&self) -> u16 {
        match self.wgm & 0x03 {
            0 => 0x3FF,      // Normal: count to 10-bit max
            1 => self.ocr_c, // CTC: OCR4C is TOP
            _ => self.ocr_c, // PWM modes: OCR4C is TOP
        }
    }

    /// Get tone frequency if Timer4 is generating audio (CTC mode with toggle).
    pub fn get_tone_hz(&self, clock_hz: u32) -> f32 {
        if self.prescale == 0 {
            return 0.0;
        }
        // Check CTC mode (WGM=01) with COM4A = toggle (01)
        let com_a = (self.tccr_a >> 6) & 0x03;
        if self.wgm == 1 && com_a == 1 && self.ocr_c > 0 {
            let freq = clock_hz as f32 / (2.0 * self.prescale as f32 * (self.ocr_c as f32 + 1.0));
            if freq >= 20.0 && freq <= 20000.0 {
                return freq;
            }
        }
        // Fast PWM using OCR4A with toggle
        if com_a == 1 && self.ocr_a > 0 {
            let top = self.get_top();
            if top > 0 {
                let freq = clock_hz as f32 / (2.0 * self.prescale as f32 * (top as f32 + 1.0));
                if freq >= 20.0 && freq <= 20000.0 {
                    return freq;
                }
            }
        }
        0.0
    }

    /// Handle register reads
    pub fn read(&self, addr: u16) -> Option<u8> {
        match addr {
            0xBE => Some(self.tcnt as u8),  // TCNT4 low
            0xBF => Some(self.tc4h),        // TC4H
            0xC0 => Some(self.tccr_a),      // TCCR4A
            0xC1 => Some(self.tccr_b),      // TCCR4B
            0xC2 => Some(self.tccr_c),      // TCCR4C
            0xC3 => Some(self.tccr_d),      // TCCR4D
            0xC4 => Some(self.tccr_e),      // TCCR4E
            0xCF => Some(self.ocr_a as u8), // OCR4A
            0xD0 => Some(self.ocr_b as u8), // OCR4B
            0xD1 => Some(self.ocr_c as u8), // OCR4C
            0xD2 => Some(self.ocr_d as u8), // OCR4D
            0xD4 => Some(self.dt4),         // DT4
            0x39 => {
                // TIFR4
                let mut v = 0u8;
                if self.tov > 0 {
                    v |= 1 << 2;
                } // TOV4
                if self.ocf_a > 0 {
                    v |= 1 << 6;
                } // OCF4A
                if self.ocf_b > 0 {
                    v |= 1 << 5;
                } // OCF4B
                if self.ocf_d > 0 {
                    v |= 1 << 7;
                } // OCF4D
                Some(v)
            }
            0x72 => Some(self.timsk), // TIMSK4
            _ => None,
        }
    }

    /// Handle register writes. Returns true if the address was handled.
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        match addr {
            0xBE => {
                // TCNT4 low — combine with TC4H
                self.tcnt = (value as u16) | ((self.tc4h as u16 & 0x03) << 8);
                true
            }
            0xBF => {
                // TC4H — high byte temp register
                self.tc4h = value & 0x03;
                true
            }
            0xC0 => {
                // TCCR4A
                self.tccr_a = value;
                true
            }
            0xC1 => {
                // TCCR4B
                self.tccr_b = value;
                self.cs = value & 0x0F;
                self.prescale = Self::decode_prescale(self.cs);
                if self.prescale > 0 && self.tick == 0 {
                    self.tick = 1;
                }
                true
            }
            0xC2 => {
                // TCCR4C
                self.tccr_c = value;
                true
            }
            0xC3 => {
                // TCCR4D
                self.tccr_d = value;
                self.wgm = value & 0x03;
                true
            }
            0xC4 => {
                // TCCR4E
                self.tccr_e = value;
                true
            }
            0xCF => {
                // OCR4A — combine with TC4H
                self.ocr_a = (value as u16) | ((self.tc4h as u16 & 0x03) << 8);
                true
            }
            0xD0 => {
                // OCR4B
                self.ocr_b = (value as u16) | ((self.tc4h as u16 & 0x03) << 8);
                true
            }
            0xD1 => {
                // OCR4C (TOP in most modes)
                self.ocr_c = (value as u16) | ((self.tc4h as u16 & 0x03) << 8);
                true
            }
            0xD2 => {
                // OCR4D
                self.ocr_d = (value as u16) | ((self.tc4h as u16 & 0x03) << 8);
                true
            }
            0xD4 => {
                // DT4
                self.dt4 = value;
                true
            }
            0x39 => {
                // TIFR4 — write 1 to clear flags
                if value & (1 << 2) != 0 {
                    self.tov = 0;
                }
                if value & (1 << 6) != 0 {
                    self.ocf_a = 0;
                }
                if value & (1 << 5) != 0 {
                    self.ocf_b = 0;
                }
                if value & (1 << 7) != 0 {
                    self.ocf_d = 0;
                }
                true
            }
            0x72 => {
                // TIMSK4
                self.timsk = value;
                true
            }
            // CLKSEL0/1/CLKSTA — clock selection (just store, use CPU clock)
            0xC5 | 0xC6 | 0xC7 => true,
            _ => false,
        }
    }

    /// Check and fire pending interrupts. Returns vector address if firing.
    pub fn check_interrupt(&mut self) -> Option<u16> {
        // Compare A
        if self.ocf_a > 0 && (self.timsk & (1 << 6) != 0) {
            self.ocf_a -= 1;
            return Some(INT_TIMER4_COMPA);
        }
        // Compare B
        if self.ocf_b > 0 && (self.timsk & (1 << 5) != 0) {
            self.ocf_b -= 1;
            return Some(INT_TIMER4_COMPB);
        }
        // Compare D
        if self.ocf_d > 0 && (self.timsk & (1 << 7) != 0) {
            self.ocf_d -= 1;
            return Some(INT_TIMER4_COMPD);
        }
        // Overflow
        if self.tov > 0 && (self.timsk & (1 << 2) != 0) {
            self.tov -= 1;
            return Some(INT_TIMER4_OVF);
        }
        None
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::Timer4State {
        crate::savestate::Timer4State {
            tcnt: self.tcnt,
            tc4h: self.tc4h,
            ocr_a: self.ocr_a,
            ocr_b: self.ocr_b,
            ocr_c: self.ocr_c,
            ocr_d: self.ocr_d,
            tccr_a: self.tccr_a,
            tccr_b: self.tccr_b,
            tccr_c: self.tccr_c,
            tccr_d: self.tccr_d,
            tccr_e: self.tccr_e,
            dt4: self.dt4,
            timsk: self.timsk,
            cs: self.cs,
            prescale: self.prescale,
            tick: self.tick,
            wgm: self.wgm,
            tov: self.tov,
            ocf_a: self.ocf_a,
            ocf_b: self.ocf_b,
            ocf_d: self.ocf_d,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::Timer4State) {
        self.tcnt = s.tcnt;
        self.tc4h = s.tc4h;
        self.ocr_a = s.ocr_a;
        self.ocr_b = s.ocr_b;
        self.ocr_c = s.ocr_c;
        self.ocr_d = s.ocr_d;
        self.tccr_a = s.tccr_a;
        self.tccr_b = s.tccr_b;
        self.tccr_c = s.tccr_c;
        self.tccr_d = s.tccr_d;
        self.tccr_e = s.tccr_e;
        self.dt4 = s.dt4;
        self.timsk = s.timsk;
        self.cs = s.cs;
        self.prescale = s.prescale;
        self.tick = s.tick;
        self.wgm = s.wgm;
        self.tov = s.tov;
        self.ocf_a = s.ocf_a;
        self.ocf_b = s.ocf_b;
        self.ocf_d = s.ocf_d;
    }
}
