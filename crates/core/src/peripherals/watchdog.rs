//! Watchdog Timer (WDT) emulation.
//!
//! Models the ATmega32u4 / ATmega328P watchdog well enough to run Arduboy and
//! Gamebuino code that uses it, most importantly `Arduboy2::exitToBootloader()`
//! and `wdt_enable()`/`wdt_reset()`/`wdt_disable()` from `<avr/wdt.h>`:
//!
//! - The `WDR` instruction restarts the countdown ([`reset_timer`](Watchdog::reset_timer)).
//! - On timeout in reset mode (WDE) the watchdog requests a system reset.
//! - On timeout in interrupt mode (WDIE) it raises the WDT interrupt; per the
//!   datasheet the hardware then clears WDIE so a following timeout (if WDE is
//!   still set) performs the reset.
//! - The timed change-enable sequence (WDCE) that guards WDE/WDP is honoured.
//!
//! The countdown uses the independent 128 kHz watchdog oscillator: the period is
//! `2048 << WDP` oscillator cycles, converted to CPU clock cycles so it can be
//! measured against the shared [`Cpu::tick`](crate::cpu::Cpu::tick) counter.
//!
//! Design modelled on (but independently written from) simavr's `avr_watchdog`.
//!
//! Note: the watchdog is intentionally not captured in save states — its
//! mid-timeout state is transient and a load leaves it in the safe disabled
//! reset state.

use crate::CLOCK_HZ;

/// Watchdog oscillator frequency (independent 128 kHz RC oscillator).
const WDT_OSC_HZ: u64 = 128_000;

/// WDTCSR bit positions.
const WDIF: u8 = 7;
const WDIE: u8 = 6;
const WDP3: u8 = 5;
const WDCE: u8 = 4;
const WDE: u8 = 3;

/// Action requested by [`Watchdog::update`].
pub enum WatchdogEvent {
    /// Nothing to do this step.
    None,
    /// A reset-mode timeout elapsed; the system should perform a watchdog reset.
    Reset,
}

/// Watchdog timer state machine.
pub struct Watchdog {
    /// WDT interrupt vector (word address); depends on CPU type.
    vector: u16,
    /// WDTCSR data-space address (0x60 on both 32u4 and 328P).
    wdtcsr_addr: u16,
    /// WDE — reset-mode enable.
    wde: bool,
    /// WDIE — interrupt-mode enable.
    wdie: bool,
    /// WDP — 4-bit prescaler select (0..=9 valid; larger values clamp to 9).
    wdp: u8,
    /// True while the timed change-enable window (WDCE) is open for the next write.
    change_enabled: bool,
    /// True while the timer is counting (`wde || wdie`).
    enabled: bool,
    /// Cycle-count baseline; the timeout is measured from here.
    last_tick: u64,
    /// A WDT interrupt has occurred and is waiting for the global interrupt enable.
    irq_pending: bool,
}

impl Watchdog {
    pub fn new(vector: u16, wdtcsr_addr: u16) -> Self {
        Watchdog {
            vector,
            wdtcsr_addr,
            wde: false,
            wdie: false,
            wdp: 0,
            change_enabled: false,
            enabled: false,
            last_tick: 0,
            irq_pending: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Watchdog::new(self.vector, self.wdtcsr_addr);
    }

    /// Timeout period in CPU clock cycles for the current prescaler.
    ///
    /// The WDT counts on the 128 kHz oscillator with a period of
    /// `2048 << WDP` oscillator cycles; convert that to CPU cycles.
    fn timeout_cycles(&self) -> u64 {
        let wdp = self.wdp.min(9) as u32;
        let osc_cycles = 2048u64 << wdp;
        osc_cycles * CLOCK_HZ as u64 / WDT_OSC_HZ
    }

    /// Compose the WDTCSR read-back image. WDCE always reads back as 0.
    fn image(&self) -> u8 {
        let wdp3 = (self.wdp >> 3) & 1;
        let wdp2_0 = self.wdp & 0x07;
        ((self.irq_pending as u8) << WDIF)
            | ((self.wdie as u8) << WDIE)
            | (wdp3 << WDP3)
            | ((self.wde as u8) << WDE)
            | wdp2_0
    }

    /// Restart the countdown. Invoked by the `WDR` instruction.
    pub fn reset_timer(&mut self, tick: u64) {
        self.last_tick = tick;
    }

    /// Handle a write to WDTCSR and mirror the resulting image into `data`.
    pub fn write_wdtcsr(&mut self, value: u8, tick: u64, data: &mut [u8]) {
        // Writing 1 to WDIF clears the pending interrupt flag.
        if value & (1 << WDIF) != 0 {
            self.irq_pending = false;
        }

        let want_wdce = value & (1 << WDCE) != 0;
        let new_wde = value & (1 << WDE) != 0;
        // WDP3 lives at bit 5, WDP2:0 at bits 2:0.
        let new_wdp = ((value >> 2) & 0x08) | (value & 0x07);

        // WDE may be set at any time. Clearing WDE or changing WDP requires the
        // change-enable window — either opened by a previous WDCE write or by
        // this write setting WDCE itself.
        let allow_change = self.change_enabled || want_wdce;
        if allow_change {
            self.wde = new_wde;
            self.wdp = new_wdp;
        } else if new_wde {
            self.wde = true;
        }

        // WDIE is freely writable.
        self.wdie = value & (1 << WDIE) != 0;

        // Setting WDCE arms the window for the next write; any plain write closes it.
        self.change_enabled = want_wdce;

        let was_enabled = self.enabled;
        self.enabled = self.wde || self.wdie;
        // Reconfiguring an idle watchdog (re)starts its countdown.
        if self.enabled && !was_enabled {
            self.last_tick = tick;
        }

        if (self.wdtcsr_addr as usize) < data.len() {
            data[self.wdtcsr_addr as usize] = self.image();
        }
    }

    /// Advance the watchdog to `tick`. Returns [`WatchdogEvent::Reset`] when a
    /// reset-mode timeout elapses.
    pub fn update(&mut self, tick: u64, data: &mut [u8]) -> WatchdogEvent {
        if self.enabled && tick.wrapping_sub(self.last_tick) >= self.timeout_cycles() {
            self.last_tick = tick;
            if self.wdie {
                // Interrupt mode: fire once. Hardware clears WDIE, so a later
                // timeout with WDE still set escalates to a reset.
                self.wdie = false;
                self.irq_pending = true;
                self.enabled = self.wde;
                if (self.wdtcsr_addr as usize) < data.len() {
                    data[self.wdtcsr_addr as usize] = self.image();
                }
            } else if self.wde {
                return WatchdogEvent::Reset;
            } else {
                self.enabled = false;
            }
        }
        WatchdogEvent::None
    }

    /// Take a pending WDT interrupt vector, if one is ready and interrupts are
    /// globally enabled.
    pub fn take_interrupt(&mut self, global_ie: bool) -> Option<u16> {
        if self.irq_pending && global_ie {
            self.irq_pending = false;
            Some(self.vector)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 32u4 WDT vector / WDTCSR address.
    fn wdt() -> Watchdog {
        Watchdog::new(0x0018, 0x60)
    }

    // wdt_enable(WDTO_15MS): WDP=0 → 2048 osc cycles → 2048*125 = 256000 CPU cycles.
    #[test]
    fn timeout_cycles_wdp0() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        // WDCE|WDE arm, then WDE with WDP=0.
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDE, 0, &mut data);
        assert_eq!(w.timeout_cycles(), 256_000);
    }

    #[test]
    fn reset_mode_fires_after_timeout() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDE, 0, &mut data);
        // Just before timeout: no event.
        assert!(matches!(w.update(255_999, &mut data), WatchdogEvent::None));
        // At timeout: reset requested.
        assert!(matches!(w.update(256_000, &mut data), WatchdogEvent::Reset));
    }

    #[test]
    fn wdr_defers_the_timeout() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDE, 0, &mut data);
        // Pet the dog near the deadline; the countdown restarts.
        w.reset_timer(200_000);
        assert!(matches!(w.update(256_000, &mut data), WatchdogEvent::None));
        assert!(matches!(w.update(456_000, &mut data), WatchdogEvent::Reset));
    }

    #[test]
    fn interrupt_mode_then_reset() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        // WDIE + WDE: first timeout interrupts, second resets.
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr((1 << WDIE) | (1 << WDE), 0, &mut data);
        assert!(matches!(w.update(256_000, &mut data), WatchdogEvent::None));
        assert_eq!(w.take_interrupt(true), Some(0x0018));
        // WDIE cleared by hardware; next timeout escalates to reset.
        assert!(matches!(w.update(512_000, &mut data), WatchdogEvent::Reset));
    }

    #[test]
    fn pending_interrupt_waits_for_global_ie() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDIE, 0, &mut data);
        w.update(256_000, &mut data);
        assert_eq!(w.take_interrupt(false), None); // interrupts disabled
        assert_eq!(w.take_interrupt(true), Some(0x0018)); // delivered once enabled
        assert_eq!(w.take_interrupt(true), None); // and only once
    }

    #[test]
    fn disable_via_wdce_sequence_stops_the_timer() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDE, 0, &mut data);
        // wdt_disable(): WDCE|WDE then 0.
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(0, 0, &mut data);
        assert!(matches!(
            w.update(10_000_000, &mut data),
            WatchdogEvent::None
        ));
    }

    #[test]
    fn wde_cannot_be_cleared_without_change_enable() {
        let mut w = wdt();
        let mut data = vec![0u8; 0x100];
        w.write_wdtcsr((1 << WDCE) | (1 << WDE), 0, &mut data);
        w.write_wdtcsr(1 << WDE, 0, &mut data);
        // A stray write of 0 without the WDCE sequence must NOT clear WDE.
        w.write_wdtcsr(0, 0, &mut data);
        assert!(matches!(w.update(256_000, &mut data), WatchdogEvent::Reset));
    }
}
