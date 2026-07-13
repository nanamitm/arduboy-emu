//! Advanced debugging facilities.
//!
//! - **RAM Viewer**: Hex + ASCII dump of any data-space region
//! - **I/O Register Viewer**: Named register display for ATmega32u4 / ATmega328P
//! - **Watchpoints**: Trigger on data-space read/write at specified addresses
//!
//! Watchpoints are checked in the emulator's `read_data` / `write_data` paths
//! when enabled.

/// Watchpoint trigger type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WatchKind {
    /// Trigger on write
    Write,
    /// Trigger on read
    Read,
    /// Trigger on read or write
    ReadWrite,
}

/// A data-space watchpoint.
#[derive(Debug, Clone)]
pub struct Watchpoint {
    /// Data-space address to watch
    pub addr: u16,
    /// Trigger condition
    pub kind: WatchKind,
    /// Optional: only trigger when value changes to this
    pub value_match: Option<u8>,
    /// Hit count
    pub hits: u64,
    /// Enabled
    pub enabled: bool,
}

/// Watchpoint trigger event returned from check functions.
#[derive(Debug)]
pub struct WatchHit {
    /// Watchpoint index
    pub index: usize,
    /// Address that triggered
    pub addr: u16,
    /// Old value (for writes) or current value (for reads)
    pub old_val: u8,
    /// New value (for writes, same as old for reads)
    pub new_val: u8,
    /// Access kind that triggered
    pub access: WatchKind,
}

/// Debugger state.
pub struct Debugger {
    /// Active watchpoints
    pub watchpoints: Vec<Watchpoint>,
    /// True if a watchpoint was triggered (emulator should pause)
    pub watch_hit: Option<WatchHit>,
}

impl Debugger {
    pub fn new() -> Self {
        Debugger {
            watchpoints: Vec::new(),
            watch_hit: None,
        }
    }

    /// Add a watchpoint. Returns its index.
    pub fn add_watchpoint(&mut self, addr: u16, kind: WatchKind) -> usize {
        let idx = self.watchpoints.len();
        self.watchpoints.push(Watchpoint {
            addr,
            kind,
            value_match: None,
            hits: 0,
            enabled: true,
        });
        idx
    }

    /// Remove a watchpoint by index.
    pub fn remove_watchpoint(&mut self, idx: usize) -> bool {
        if idx < self.watchpoints.len() {
            self.watchpoints.remove(idx);
            true
        } else {
            false
        }
    }

    /// Check watchpoints for a write access. Call BEFORE writing to data[].
    #[inline]
    pub fn check_write(&mut self, addr: u16, old_val: u8, new_val: u8) {
        for (i, wp) in self.watchpoints.iter_mut().enumerate() {
            if !wp.enabled || wp.addr != addr {
                continue;
            }
            if wp.kind == WatchKind::Read {
                continue;
            }
            if let Some(v) = wp.value_match {
                if new_val != v {
                    continue;
                }
            }
            wp.hits += 1;
            if self.watch_hit.is_none() {
                self.watch_hit = Some(WatchHit {
                    index: i,
                    addr,
                    old_val,
                    new_val,
                    access: WatchKind::Write,
                });
            }
        }
    }

    /// Check watchpoints for a read access.
    #[inline]
    pub fn check_read(&mut self, addr: u16, val: u8) {
        for (i, wp) in self.watchpoints.iter_mut().enumerate() {
            if !wp.enabled || wp.addr != addr {
                continue;
            }
            if wp.kind == WatchKind::Write {
                continue;
            }
            wp.hits += 1;
            if self.watch_hit.is_none() {
                self.watch_hit = Some(WatchHit {
                    index: i,
                    addr,
                    old_val: val,
                    new_val: val,
                    access: WatchKind::Read,
                });
            }
        }
    }

    /// Take pending watchpoint hit (returns and clears it).
    pub fn take_hit(&mut self) -> Option<WatchHit> {
        self.watch_hit.take()
    }

    /// Format watchpoints list.
    pub fn list_watchpoints(&self) -> String {
        if self.watchpoints.is_empty() {
            return "No watchpoints set.\n".into();
        }
        let mut s = String::new();
        for (i, wp) in self.watchpoints.iter().enumerate() {
            let k = match wp.kind {
                WatchKind::Write => "W",
                WatchKind::Read => "R",
                WatchKind::ReadWrite => "RW",
            };
            let en = if wp.enabled { " " } else { "!" };
            let vm = if let Some(v) = wp.value_match {
                format!(" =0x{:02X}", v)
            } else {
                String::new()
            };
            s.push_str(&format!(
                "  [{}]{} 0x{:04X} {}  hits={}{}\n",
                i, en, wp.addr, k, wp.hits, vm
            ));
        }
        s
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

// ─── RAM Viewer ─────────────────────────────────────────────────────────────

/// Format a hex + ASCII dump of data space.
///
/// Outputs 16 bytes per line with address, hex values, and ASCII printable chars.
pub fn dump_ram(data: &[u8], start: u16, length: u16) -> String {
    let mut s = String::new();
    let end = (start as usize + length as usize).min(data.len());
    let start = start as usize;
    let mut addr = start;
    while addr < end {
        let line_end = (addr + 16).min(end);
        s.push_str(&format!("{:04X}: ", addr));
        // Hex bytes
        for i in addr..addr + 16 {
            if i < line_end {
                s.push_str(&format!("{:02X} ", data[i]));
            } else {
                s.push_str("   ");
            }
            if i == addr + 7 {
                s.push(' ');
            }
        }
        s.push(' ');
        // ASCII
        for i in addr..line_end {
            let c = data[i];
            if c >= 0x20 && c < 0x7F {
                s.push(c as char);
            } else {
                s.push('.');
            }
        }
        s.push('\n');
        addr += 16;
    }
    s
}

/// Format a diff view showing only changed bytes between two snapshots.
pub fn dump_ram_diff(old: &[u8], new: &[u8], start: u16, length: u16) -> String {
    let mut s = String::new();
    let end = (start as usize + length as usize).min(old.len().min(new.len()));
    let mut any = false;
    for i in start as usize..end {
        if old[i] != new[i] {
            s.push_str(&format!("  0x{:04X}: {:02X} → {:02X}\n", i, old[i], new[i]));
            any = true;
        }
    }
    if !any {
        s.push_str("  (no changes)\n");
    }
    s
}

// ─── I/O Register Viewer ────────────────────────────────────────────────────

/// Named I/O register definitions for ATmega32u4.
pub fn io_reg_names_32u4() -> Vec<(u16, &'static str)> {
    vec![
        (0x23, "PINB"),
        (0x24, "DDRB"),
        (0x25, "PORTB"),
        (0x26, "PINC"),
        (0x27, "DDRC"),
        (0x28, "PORTC"),
        (0x29, "PIND"),
        (0x2A, "DDRD"),
        (0x2B, "PORTD"),
        (0x2C, "PINE"),
        (0x2D, "DDRE"),
        (0x2E, "PORTE"),
        (0x2F, "PINF"),
        (0x30, "DDRF"),
        (0x31, "PORTF"),
        (0x35, "TIFR0"),
        (0x36, "TIFR1"),
        (0x37, "TIFR3"),
        (0x38, "TIFR4"),
        (0x3B, "PCIFR"),
        (0x3C, "EIFR"),
        (0x3D, "EIMSK"),
        (0x3E, "GPIOR0"),
        (0x3F, "EECR"),
        (0x40, "EEDR"),
        (0x41, "EEARL"),
        (0x42, "EEARH"),
        (0x44, "TCCR0A"),
        (0x45, "TCCR0B"),
        (0x46, "TCNT0"),
        (0x47, "OCR0A"),
        (0x48, "OCR0B"),
        (0x49, "PLLCSR"),
        (0x4C, "SPCR"),
        (0x4D, "SPSR"),
        (0x4E, "SPDR"),
        (0x53, "SMCR"),
        (0x57, "MCUSR"),
        (0x58, "MCUCR"),
        (0x5D, "SPL"),
        (0x5E, "SPH"),
        (0x5F, "SREG"),
        (0x60, "WDTCSR"),
        (0x61, "CLKPR"),
        (0x64, "PRR0"),
        (0x65, "PRR1"),
        (0x6E, "TIMSK0"),
        (0x6F, "TIMSK1"),
        (0x70, "TIMSK3"),
        (0x71, "TIMSK4"),
        (0x78, "ADCL"),
        (0x79, "ADCH"),
        (0x7A, "ADCSRA"),
        (0x7B, "ADCSRB"),
        (0x7C, "ADMUX"),
        (0x80, "TCCR1A"),
        (0x81, "TCCR1B"),
        (0x82, "TCCR1C"),
        (0x84, "TCNT1L"),
        (0x85, "TCNT1H"),
        (0x88, "OCR1AL"),
        (0x89, "OCR1AH"),
        (0x8A, "OCR1BL"),
        (0x8B, "OCR1BH"),
        (0x8C, "OCR1CL"),
        (0x8D, "OCR1CH"),
        (0x90, "TCCR3A"),
        (0x91, "TCCR3B"),
        (0x92, "TCCR3C"),
        (0x94, "TCNT3L"),
        (0x95, "TCNT3H"),
        (0x98, "OCR3AL"),
        (0x99, "OCR3AH"),
        (0xBE, "TCCR4A"),
        (0xBF, "TCCR4B"),
        (0xC0, "TCCR4C"),
        (0xC1, "TCCR4D"),
        (0xCF, "OCR4A"),
        (0xD0, "OCR4B"),
        (0xD2, "OCR4D"),
        (0xD8, "USBCON"),
    ]
}

/// Named I/O register definitions for ATmega328P.
pub fn io_reg_names_328p() -> Vec<(u16, &'static str)> {
    vec![
        (0x23, "PINB"),
        (0x24, "DDRB"),
        (0x25, "PORTB"),
        (0x26, "PINC"),
        (0x27, "DDRC"),
        (0x28, "PORTC"),
        (0x29, "PIND"),
        (0x2A, "DDRD"),
        (0x2B, "PORTD"),
        (0x35, "TIFR0"),
        (0x36, "TIFR1"),
        (0x37, "TIFR2"),
        (0x3B, "PCIFR"),
        (0x3C, "EIFR"),
        (0x3D, "EIMSK"),
        (0x3E, "GPIOR0"),
        (0x3F, "EECR"),
        (0x40, "EEDR"),
        (0x41, "EEARL"),
        (0x42, "EEARH"),
        (0x44, "TCCR0A"),
        (0x45, "TCCR0B"),
        (0x46, "TCNT0"),
        (0x47, "OCR0A"),
        (0x48, "OCR0B"),
        (0x4C, "SPCR"),
        (0x4D, "SPSR"),
        (0x4E, "SPDR"),
        (0x53, "SMCR"),
        (0x57, "MCUSR"),
        (0x58, "MCUCR"),
        (0x5D, "SPL"),
        (0x5E, "SPH"),
        (0x5F, "SREG"),
        (0x60, "WDTCSR"),
        (0x61, "CLKPR"),
        (0x64, "PRR"),
        (0x6E, "TIMSK0"),
        (0x6F, "TIMSK1"),
        (0x70, "TIMSK2"),
        (0x78, "ADCL"),
        (0x79, "ADCH"),
        (0x7A, "ADCSRA"),
        (0x7B, "ADCSRB"),
        (0x7C, "ADMUX"),
        (0x80, "TCCR1A"),
        (0x81, "TCCR1B"),
        (0x82, "TCCR1C"),
        (0x84, "TCNT1L"),
        (0x85, "TCNT1H"),
        (0x88, "OCR1AL"),
        (0x89, "OCR1AH"),
        (0x8A, "OCR1BL"),
        (0x8B, "OCR1BH"),
        (0xB0, "TCCR2A"),
        (0xB1, "TCCR2B"),
        (0xB2, "TCNT2"),
        (0xB3, "OCR2A"),
        (0xB4, "OCR2B"),
        (0xB6, "ASSR"),
    ]
}

/// Format I/O register dump with names and values.
pub fn dump_io_regs(data: &[u8], is_328p: bool) -> String {
    let regs = if is_328p {
        io_reg_names_328p()
    } else {
        io_reg_names_32u4()
    };
    let mut s = String::new();
    for (addr, name) in &regs {
        let a = *addr as usize;
        let val = if a < data.len() { data[a] } else { 0 };
        if val != 0 {
            s.push_str(&format!(
                "  {:>8} (0x{:02X}) = 0x{:02X}  {:08b}\n",
                name, addr, val, val
            ));
        }
    }
    if s.is_empty() {
        s.push_str("  (all zero)\n");
    }
    s
}

/// Format a compact I/O register dump showing all registers.
pub fn dump_io_regs_all(data: &[u8], is_328p: bool) -> String {
    let regs = if is_328p {
        io_reg_names_328p()
    } else {
        io_reg_names_32u4()
    };
    let mut s = String::new();
    let mut col = 0;
    for (addr, name) in &regs {
        let a = *addr as usize;
        let val = if a < data.len() { data[a] } else { 0 };
        s.push_str(&format!("{:>8}={:02X}", name, val));
        col += 1;
        if col % 4 == 0 {
            s.push('\n');
        } else {
            s.push_str("  ");
        }
    }
    if col % 4 != 0 {
        s.push('\n');
    }
    s
}

/// Resolve an I/O address to its name (if known).
pub fn io_name(addr: u16, is_328p: bool) -> Option<&'static str> {
    let regs = if is_328p {
        io_reg_names_328p()
    } else {
        io_reg_names_32u4()
    };
    regs.iter().find(|(a, _)| *a == addr).map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_ram() {
        let mut data = vec![0u8; 512];
        data[0x100] = 0x41; // 'A'
        data[0x101] = 0x42; // 'B'
        data[0x10F] = 0xFF;
        let dump = dump_ram(&data, 0x100, 16);
        assert!(dump.contains("0100:"));
        assert!(dump.contains("41 42"));
        assert!(dump.contains("AB"));
    }

    #[test]
    fn test_watchpoint() {
        let mut dbg = Debugger::new();
        dbg.add_watchpoint(0x100, WatchKind::Write);
        dbg.check_write(0x100, 0x00, 0xFF);
        let hit = dbg.take_hit().unwrap();
        assert_eq!(hit.addr, 0x100);
        assert_eq!(hit.new_val, 0xFF);
    }

    #[test]
    fn test_io_name() {
        assert_eq!(io_name(0x5F, false), Some("SREG"));
        assert_eq!(io_name(0x4E, true), Some("SPDR"));
    }
}
