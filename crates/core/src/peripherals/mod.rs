//! ATmega32u4 / ATmega328P peripheral emulation.
//!
//! Contains hardware peripherals needed to run Arduboy and Gamebuino games:
//!
//! - [`Timer8`] — 8-bit Timer/Counter (Timer0 on both, Timer2 on 328P)
//! - [`Timer16`] — 16-bit Timer/Counter1 and Timer/Counter3 (audio tone generation)
//! - [`Timer4`] — 10-bit high-speed Timer/Counter4 (PWM audio, LED control, 32u4 only)
//! - [`Spi`] — SPI master controller (display and FX flash communication)
//! - [`Adc`] — Analog-to-digital converter (random seed, battery sensing)
//! - [`Pll`] — PLL frequency synthesizer (USB clock, fast PWM)
//! - [`EepromCtrl`] — EEPROM read/write controller (save data)
//! - [`FxFlash`] — W25Q128 16 MB external SPI flash (Arduboy FX game data)

mod adc;
mod eeprom;
pub mod fx_flash;
mod pll;
mod spi;
mod timer16;
mod timer4;
mod timer8;

pub use adc::Adc;
pub use eeprom::EepromCtrl;
pub use fx_flash::FxFlash;
pub use pll::Pll;
pub use spi::Spi;
pub use timer16::{Timer16, Timer16Addrs};
pub use timer4::Timer4;
pub use timer8::{Timer8, Timer8Addrs};

// ─── ATmega32u4 interrupt vector addresses (word addresses) ────────────────

pub const INT_TIMER0_COMPA: u16 = 0x002A;
pub const INT_TIMER0_COMPB: u16 = 0x002C;
pub const INT_TIMER0_OVF: u16 = 0x002E;
pub const INT_TIMER1_COMPA: u16 = 0x0022;
pub const INT_TIMER1_COMPB: u16 = 0x0024;
pub const INT_TIMER1_COMPC: u16 = 0x0026;
pub const INT_TIMER1_OVF: u16 = 0x0028;
pub const INT_TIMER3_COMPA: u16 = 0x0040;
pub const INT_TIMER3_COMPB: u16 = 0x0042;
pub const INT_TIMER3_COMPC: u16 = 0x0044;
pub const INT_TIMER3_OVF: u16 = 0x0046;
pub const INT_SPI: u16 = 0x0030;
pub const INT_ADC: u16 = 0x003A;

// Timer4 (32u4 only)
pub const INT_TIMER4_OVF: u16 = 0x0048;
pub const INT_TIMER4_COMPA: u16 = 0x004A;
pub const INT_TIMER4_COMPB: u16 = 0x004C;
pub const INT_TIMER4_COMPD: u16 = 0x004E;

// ─── ATmega328P interrupt vector addresses (word addresses) ────────────────

pub const INT_328P_TIMER0_COMPA: u16 = 0x001C;
pub const INT_328P_TIMER0_COMPB: u16 = 0x001E;
pub const INT_328P_TIMER0_OVF: u16 = 0x0020;
pub const INT_328P_TIMER1_COMPA: u16 = 0x0016;
pub const INT_328P_TIMER1_COMPB: u16 = 0x0018;
pub const INT_328P_TIMER1_OVF: u16 = 0x001A;
pub const INT_328P_TIMER2_COMPA: u16 = 0x000E;
pub const INT_328P_TIMER2_COMPB: u16 = 0x0010;
pub const INT_328P_TIMER2_OVF: u16 = 0x0012;
pub const INT_328P_SPI: u16 = 0x0022;
pub const INT_328P_USART_RX: u16 = 0x0024;
pub const INT_328P_USART_UDRE: u16 = 0x0026;
pub const INT_328P_USART_TX: u16 = 0x0028;
pub const INT_328P_ADC: u16 = 0x002A;
