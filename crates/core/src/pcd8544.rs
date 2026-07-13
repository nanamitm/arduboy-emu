//! PCD8544 84×48 monochrome LCD display controller emulation (Nokia 5110).
//!
//! Used by the Gamebuino Classic for display output. The 84×48 image is
//! centered within the standard 128×64 framebuffer for unified rendering.
//! Supports basic and extended instruction sets, horizontal/vertical
//! addressing, and contrast/bias configuration commands.

use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const FB_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4; // RGBA
const PCD_WIDTH: usize = 84;
const PCD_HEIGHT: usize = 48;
const PCD_PAGES: usize = 6; // 48 / 8

/// PCD8544 84x48 monochrome LCD display controller (Nokia 5110)
pub struct Pcd8544 {
    pub framebuffer: [u8; FB_SIZE],
    /// Internal video RAM (84 * 6 = 504 bytes)
    pub vram: [u8; PCD_WIDTH * PCD_PAGES],
    /// Current X address (column, 0-83)
    x_addr: u8,
    /// Current Y address (page/bank, 0-5)
    y_addr: u8,
    /// Extended instruction set active (H=1)
    extended_mode: bool,
    /// Display control mode
    pub display_mode: u8, // 0=blank, 1=all on, 4=normal, 5=inverse
    /// Power down
    power_down: bool,
    /// Vertical addressing mode
    vertical_addressing: bool,
    /// Whether framebuffer has been updated
    pub dirty: bool,
    /// Debug counters (per-frame, reset each frame)
    pub dbg_cmd_count: u32,
    pub dbg_data_count: u32,
}

impl Pcd8544 {
    pub fn new() -> Self {
        let mut fb = [0u8; FB_SIZE];
        // Initialize alpha channel
        for i in (3..FB_SIZE).step_by(4) {
            fb[i] = 0xFF;
        }
        Pcd8544 {
            framebuffer: fb,
            vram: [0; PCD_WIDTH * PCD_PAGES],
            x_addr: 0,
            y_addr: 0,
            extended_mode: false,
            display_mode: 0,
            power_down: false,
            vertical_addressing: false,
            dirty: false,
            dbg_cmd_count: 0,
            dbg_data_count: 0,
        }
    }

    pub fn receive_command(&mut self, byte: u8) {
        self.dbg_cmd_count += 1;

        if self.extended_mode {
            // Extended instruction set (H=1)
            if byte & 0x80 != 0 {
                // Set Vop (contrast): 0x80 | Vop[6:0]
                // Just ignore contrast setting
            } else if byte & 0x04 != 0 {
                // Temperature control: 0x04 | TC[1:0]
            } else if byte & 0x10 != 0 {
                // LCD bias system: 0x10 | BS[2:0]
            } else if byte & 0x20 != 0 {
                // Function set (also available in extended mode)
                self.power_down = byte & 0x04 != 0;
                self.vertical_addressing = byte & 0x02 != 0;
                self.extended_mode = byte & 0x01 != 0;
            }
        } else {
            // Basic instruction set (H=0)
            if byte & 0x80 != 0 {
                // Set X address: 0x80 | X[6:0]
                self.x_addr = byte & 0x7F;
                if self.x_addr >= PCD_WIDTH as u8 {
                    self.x_addr = 0;
                }
            } else if byte & 0x40 != 0 {
                // Set Y address: 0x40 | Y[2:0]
                self.y_addr = byte & 0x07;
                if self.y_addr >= PCD_PAGES as u8 {
                    self.y_addr = 0;
                }
            } else if byte & 0x20 != 0 {
                // Function set: 0x20 | PD | V | H
                self.power_down = byte & 0x04 != 0;
                self.vertical_addressing = byte & 0x02 != 0;
                self.extended_mode = byte & 0x01 != 0;
            } else if byte & 0x08 != 0 {
                // Display control: 0x08 | D | 0 | E
                let d = (byte >> 2) & 1;
                let e = byte & 1;
                self.display_mode = (d << 2) | e;
                // 0b000=blank, 0b001=all on, 0b100=normal, 0b101=inverse
            }
            // NOP for other commands
        }
    }

    pub fn receive_data(&mut self, byte: u8) {
        self.dbg_data_count += 1;

        let x = self.x_addr as usize;
        let y = self.y_addr as usize;

        if x < PCD_WIDTH && y < PCD_PAGES {
            self.vram[y * PCD_WIDTH + x] = byte;
        }

        // Advance cursor
        if self.vertical_addressing {
            self.y_addr += 1;
            if self.y_addr >= PCD_PAGES as u8 {
                self.y_addr = 0;
                self.x_addr += 1;
                if self.x_addr >= PCD_WIDTH as u8 {
                    self.x_addr = 0;
                }
            }
        } else {
            self.x_addr += 1;
            if self.x_addr >= PCD_WIDTH as u8 {
                self.x_addr = 0;
                self.y_addr += 1;
                if self.y_addr >= PCD_PAGES as u8 {
                    self.y_addr = 0;
                }
            }
        }

        self.dirty = true;
    }

    /// Render VRAM to the 128x64 framebuffer (centered, 1:1 pixel mapping)
    pub fn render_to_framebuffer(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let inverse = self.display_mode == 5;

        // Center 84x48 in 128x64: offset_x = (128-84)/2 = 22, offset_y = (64-48)/2 = 8
        let off_x = (SCREEN_WIDTH - PCD_WIDTH) / 2;
        let off_y = (SCREEN_HEIGHT - PCD_HEIGHT) / 2;

        // Clear entire framebuffer
        for i in (0..FB_SIZE).step_by(4) {
            self.framebuffer[i] = 0;
            self.framebuffer[i + 1] = 0;
            self.framebuffer[i + 2] = 0;
            self.framebuffer[i + 3] = 0xFF;
        }

        // Render PCD8544 VRAM
        for page in 0..PCD_PAGES {
            for col in 0..PCD_WIDTH {
                let vbyte = self.vram[page * PCD_WIDTH + col];
                for bit in 0..8u8 {
                    let pixel_on = ((vbyte >> bit) & 1) != 0;
                    let pixel_on = pixel_on ^ inverse;
                    let sx = off_x + col;
                    let sy = off_y + page * 8 + bit as usize;
                    if sx < SCREEN_WIDTH && sy < SCREEN_HEIGHT {
                        let offset = (sy * SCREEN_WIDTH + sx) * 4;
                        if pixel_on {
                            self.framebuffer[offset] = 0xFF;
                            self.framebuffer[offset + 1] = 0xFF;
                            self.framebuffer[offset + 2] = 0xFF;
                        }
                        // else already 0 from clear
                    }
                }
            }
        }
    }

    pub fn dbg_reset_counters(&mut self) {
        self.dbg_cmd_count = 0;
        self.dbg_data_count = 0;
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::Pcd8544State {
        crate::savestate::Pcd8544State {
            framebuffer: self.framebuffer.to_vec(),
            vram: self.vram.to_vec(),
            x_addr: self.x_addr,
            y_addr: self.y_addr,
            extended_mode: self.extended_mode,
            display_mode: self.display_mode,
            power_down: self.power_down,
            vertical_addressing: self.vertical_addressing,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::Pcd8544State) {
        let fb_len = s.framebuffer.len().min(self.framebuffer.len());
        self.framebuffer[..fb_len].copy_from_slice(&s.framebuffer[..fb_len]);
        let vram_len = s.vram.len().min(self.vram.len());
        self.vram[..vram_len].copy_from_slice(&s.vram[..vram_len]);
        self.x_addr = s.x_addr;
        self.y_addr = s.y_addr;
        self.extended_mode = s.extended_mode;
        self.display_mode = s.display_mode;
        self.power_down = s.power_down;
        self.vertical_addressing = s.vertical_addressing;
        self.dirty = true;
    }
}
