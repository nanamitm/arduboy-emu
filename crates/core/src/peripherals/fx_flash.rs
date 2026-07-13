/// W25Q128 SPI Flash emulation for Arduboy FX
/// 16MB flash connected via SPI with CS on PD1 (Arduino D2)
///
/// Supported commands:
/// - 0x03: Read Data (addr24, then continuous read)
/// - 0x0B: Fast Read (addr24 + dummy, then continuous read)
/// - 0x9F: JEDEC ID → EF 40 18 (W25Q128)
/// - 0xAB: Release Power Down / Device ID → returns device ID 0x17
/// - 0x05: Read Status Register 1 → 0x00 (not busy)
/// - 0xB9: Power Down
/// - 0x06: Write Enable
/// - 0x04: Write Disable
/// - 0x02: Page Program (addr24 + data)
/// - 0x20: Sector Erase (4KB)

const FLASH_SIZE: usize = 16 * 1024 * 1024; // 16MB

// JEDEC ID for W25Q128JV
const JEDEC_MFR: u8 = 0xEF; // Winbond
const JEDEC_TYPE: u8 = 0x40; // SPI
const JEDEC_CAP: u8 = 0x18; // 128Mbit = 16MB

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FxState {
    Idle,
    // Read commands
    ReadAddr { cmd: u8, addr_bytes: u8, addr: u32 },
    ReadDummy { addr: u32 },
    Reading { addr: u32 },
    // JEDEC ID response
    JedecId { byte_idx: u8 },
    // Release Power Down / Device ID
    ReleasePD { byte_idx: u8 },
    // Status register
    ReadStatus,
    // Page Program
    ProgAddr { addr_bytes: u8, addr: u32 },
    Programming { addr: u32 },
    // Sector Erase
    EraseAddr { addr_bytes: u8, addr: u32 },
}

pub struct FxFlash {
    pub data: Vec<u8>,
    pub state: FxState,
    pub loaded: bool,
    write_enabled: bool,
    powered_down: bool,
}

impl FxFlash {
    pub fn new() -> Self {
        FxFlash {
            data: Vec::new(), // Lazy: only allocate when data is loaded
            state: FxState::Idle,
            loaded: false,
            write_enabled: false,
            powered_down: false,
        }
    }

    fn ensure_data(&mut self) {
        if self.data.is_empty() {
            self.data = vec![0xFF; FLASH_SIZE];
        }
    }

    /// Load flash data from binary data. Data is loaded at start of flash by default.
    pub fn load_data(&mut self, bin: &[u8]) {
        self.ensure_data();
        if bin.len() <= FLASH_SIZE {
            self.data[..bin.len()].copy_from_slice(bin);
        } else {
            self.data.copy_from_slice(&bin[..FLASH_SIZE]);
        }
        self.loaded = true;
    }

    /// Load flash data at a specific offset
    pub fn load_data_at(&mut self, bin: &[u8], offset: usize) {
        self.ensure_data();
        let end = (offset + bin.len()).min(FLASH_SIZE);
        let len = end - offset;
        self.data[offset..end].copy_from_slice(&bin[..len]);
        self.loaded = true;
    }

    /// Called when CS goes HIGH - deselect, reset state machine
    pub fn deselect(&mut self) {
        self.state = FxState::Idle;
    }

    /// Process one SPI byte exchange. Returns the response byte (MISO).
    /// `mosi` is the byte sent by the master (written to SPDR).
    pub fn transfer(&mut self, mosi: u8) -> u8 {
        match self.state {
            FxState::Idle => {
                // First byte after CS low = command
                match mosi {
                    0x03 => {
                        // Read Data: 3 address bytes then continuous read
                        self.state = FxState::ReadAddr {
                            cmd: 0x03,
                            addr_bytes: 0,
                            addr: 0,
                        };
                        0xFF
                    }
                    0x0B => {
                        // Fast Read: 3 address bytes + 1 dummy then continuous read
                        self.state = FxState::ReadAddr {
                            cmd: 0x0B,
                            addr_bytes: 0,
                            addr: 0,
                        };
                        0xFF
                    }
                    0x9F => {
                        // JEDEC ID
                        self.state = FxState::JedecId { byte_idx: 0 };
                        0xFF
                    }
                    0xAB => {
                        // Release Power Down / Device ID
                        self.powered_down = false;
                        self.state = FxState::ReleasePD { byte_idx: 0 };
                        0xFF
                    }
                    0xB9 => {
                        // Power Down
                        self.powered_down = true;
                        0xFF
                    }
                    0x05 => {
                        // Read Status Register 1
                        self.state = FxState::ReadStatus;
                        0xFF
                    }
                    0x06 => {
                        // Write Enable
                        self.write_enabled = true;
                        0xFF
                    }
                    0x04 => {
                        // Write Disable
                        self.write_enabled = false;
                        0xFF
                    }
                    0x02 => {
                        // Page Program
                        self.state = FxState::ProgAddr {
                            addr_bytes: 0,
                            addr: 0,
                        };
                        0xFF
                    }
                    0x20 => {
                        // Sector Erase (4KB)
                        self.state = FxState::EraseAddr {
                            addr_bytes: 0,
                            addr: 0,
                        };
                        0xFF
                    }
                    _ => {
                        // Unknown command, ignore
                        0xFF
                    }
                }
            }

            FxState::ReadAddr {
                cmd,
                addr_bytes,
                addr,
            } => {
                let new_addr = (addr << 8) | mosi as u32;
                let new_count = addr_bytes + 1;
                if new_count >= 3 {
                    let masked = (new_addr as usize) % FLASH_SIZE;
                    if cmd == 0x0B {
                        // Fast Read needs 1 dummy byte
                        self.state = FxState::ReadDummy {
                            addr: masked as u32,
                        };
                    } else {
                        // Standard Read - start immediately
                        self.state = FxState::Reading {
                            addr: masked as u32,
                        };
                    }
                } else {
                    self.state = FxState::ReadAddr {
                        cmd,
                        addr_bytes: new_count,
                        addr: new_addr,
                    };
                }
                0xFF
            }

            FxState::ReadDummy { addr } => {
                // One dummy byte for Fast Read
                self.state = FxState::Reading { addr };
                0xFF
            }

            FxState::Reading { addr } => {
                let val = if self.data.is_empty() {
                    0xFF
                } else {
                    let idx = (addr as usize) % self.data.len();
                    self.data[idx]
                };
                self.state = FxState::Reading {
                    addr: addr.wrapping_add(1) & (FLASH_SIZE as u32 - 1),
                };
                val
            }

            FxState::JedecId { byte_idx } => {
                let val = match byte_idx {
                    0 => JEDEC_MFR,
                    1 => JEDEC_TYPE,
                    2 => JEDEC_CAP,
                    _ => 0x00,
                };
                self.state = FxState::JedecId {
                    byte_idx: byte_idx + 1,
                };
                val
            }

            FxState::ReleasePD { byte_idx } => {
                // 3 dummy bytes then device ID
                let val = if byte_idx >= 3 { 0x17 } else { 0xFF };
                self.state = FxState::ReleasePD {
                    byte_idx: byte_idx + 1,
                };
                val
            }

            FxState::ReadStatus => {
                // Bit 0 = BUSY (always 0, instant operations)
                // Bit 1 = WEL (write enable latch)
                (self.write_enabled as u8) << 1
            }

            FxState::ProgAddr { addr_bytes, addr } => {
                let new_addr = (addr << 8) | mosi as u32;
                let new_count = addr_bytes + 1;
                if new_count >= 3 {
                    let masked = (new_addr as usize) % FLASH_SIZE;
                    self.state = FxState::Programming {
                        addr: masked as u32,
                    };
                } else {
                    self.state = FxState::ProgAddr {
                        addr_bytes: new_count,
                        addr: new_addr,
                    };
                }
                0xFF
            }

            FxState::Programming { addr } => {
                if self.write_enabled && !self.data.is_empty() {
                    let idx = (addr as usize) % self.data.len();
                    // Flash programming can only clear bits (AND operation)
                    self.data[idx] &= mosi;
                    // Stay within same 256-byte page
                    let page_base = addr & !0xFF;
                    let next = page_base | ((addr + 1) & 0xFF);
                    self.state = FxState::Programming { addr: next };
                }
                0xFF
            }

            FxState::EraseAddr { addr_bytes, addr } => {
                let new_addr = (addr << 8) | mosi as u32;
                let new_count = addr_bytes + 1;
                if new_count >= 3 {
                    if self.write_enabled && !self.data.is_empty() {
                        // Erase 4KB sector
                        let sector_start = (new_addr as usize) & !(4096 - 1);
                        let sector_end = (sector_start + 4096).min(self.data.len());
                        for b in &mut self.data[sector_start..sector_end] {
                            *b = 0xFF;
                        }
                    }
                    self.write_enabled = false;
                    self.state = FxState::Idle;
                } else {
                    self.state = FxState::EraseAddr {
                        addr_bytes: new_count,
                        addr: new_addr,
                    };
                }
                0xFF
            }
        }
    }

    /// Capture state for save state. FX command state is reset to Idle.
    pub fn save_state(&self) -> crate::savestate::FxFlashState {
        crate::savestate::FxFlashState {
            data: self.data.clone(),
            loaded: self.loaded,
            write_enabled: self.write_enabled,
            powered_down: self.powered_down,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: crate::savestate::FxFlashState) {
        self.data = s.data;
        self.loaded = s.loaded;
        self.write_enabled = s.write_enabled;
        self.powered_down = s.powered_down;
        self.state = FxState::Idle; // Reset transient SPI state
    }
}
