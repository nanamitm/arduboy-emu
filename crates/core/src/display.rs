//! SSD1306 128×64 monochrome OLED display controller emulation.
//!
//! Processes command and data bytes received over SPI to maintain an internal
//! VRAM that is rendered to an RGBA framebuffer. Supports horizontal and
//! vertical addressing modes, column/page address windowing, and the
//! display-on/off command set used by the Arduboy2 library.

use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const FB_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4; // RGBA

/// SSD1306 128x64 monochrome OLED display controller
pub struct Ssd1306 {
    /// Rendered 128×64 image as RGBA bytes (row-major, 4 bytes per pixel).
    pub framebuffer: [u8; FB_SIZE],
    /// Current column pointer
    col: u8,
    /// Current page pointer (each page = 8 rows)
    page: u8,
    /// Column address range
    col_start: u8,
    col_end: u8,
    /// Page address range
    page_start: u8,
    page_end: u8,
    /// Display inversion
    inverted: bool,
    /// Display on/off
    display_on: bool,
    /// Contrast level (0x00–0xFF, default 0x7F)
    pub contrast: u8,
    /// Whether framebuffer has been updated
    pub dirty: bool,
    /// Debug: command bytes received this frame
    pub dbg_cmd_count: u32,
    /// Debug: data bytes received this frame
    pub dbg_data_count: u32,
    /// Multi-byte command state
    cmd_state: CmdState,
    /// Number of remaining parameter bytes to ignore
    cmd_skip: u8,
}

#[derive(Debug, Clone, Copy)]
enum CmdState {
    Ready,
    SetColStart,
    SetColEnd,
    SetPageStart,
    SetPageEnd,
    SetContrast,
}

impl Ssd1306 {
    /// Create a display controller with a cleared framebuffer and default state.
    pub fn new() -> Self {
        Ssd1306 {
            framebuffer: [0; FB_SIZE],
            col: 0,
            page: 0,
            col_start: 0,
            col_end: 127,
            page_start: 0,
            page_end: 7,
            inverted: false,
            display_on: false,
            contrast: 0xCF, // SSD1306 default
            dirty: false,
            cmd_state: CmdState::Ready,
            cmd_skip: 0,
            dbg_cmd_count: 0,
            dbg_data_count: 0,
        }
    }

    /// Receive a command byte (DC pin low)
    pub fn receive_command(&mut self, byte: u8) {
        self.dbg_cmd_count += 1;
        // If we're skipping parameter bytes from a previous command
        if self.cmd_skip > 0 {
            self.cmd_skip -= 1;
            return;
        }

        match self.cmd_state {
            CmdState::SetColStart => {
                self.col_start = byte.min(127);
                self.col = self.col_start;
                self.cmd_state = CmdState::SetColEnd;
                return;
            }
            CmdState::SetColEnd => {
                self.col_end = byte.min(127);
                self.cmd_state = CmdState::Ready;
                return;
            }
            CmdState::SetPageStart => {
                self.page_start = byte.min(7);
                self.page = self.page_start;
                self.cmd_state = CmdState::SetPageEnd;
                return;
            }
            CmdState::SetPageEnd => {
                self.page_end = byte.min(7);
                self.cmd_state = CmdState::Ready;
                return;
            }
            CmdState::SetContrast => {
                self.contrast = byte;
                self.cmd_state = CmdState::Ready;
                return;
            }
            CmdState::Ready => {}
        }

        match byte {
            0x21 => {
                // Set column address (2 more bytes follow)
                self.cmd_state = CmdState::SetColStart;
            }
            0x22 => {
                // Set page address (2 more bytes follow)
                self.cmd_state = CmdState::SetPageStart;
            }
            0xAE => {
                self.display_on = false;
            }
            0xAF => {
                self.display_on = true;
            }
            0xA6 => {
                self.inverted = false;
                self.dirty = true;
            }
            0xA7 => {
                self.inverted = true;
                self.dirty = true;
            }
            // Set contrast (next byte is contrast value)
            0x81 => {
                self.cmd_state = CmdState::SetContrast;
            }
            // Commands that take 1 parameter byte (skip next byte)
            0x20 | // Set memory addressing mode
            0xA8 | // Set multiplex ratio
            0xD3 | // Set display offset
            0xD5 | // Set display clock divide
            0xD9 | // Set pre-charge period
            0xDA | // Set COM pins hardware config
            0xDB | // Set VCOMH deselect level
            0x8D   // Charge pump setting
            => {
                self.cmd_skip = 1;
            }
            // Commands with no extra bytes (or lower nibble commands)
            0x00..=0x0F => {} // Set lower column start address (page addressing)
            0x10..=0x1F => {} // Set higher column start address
            0x40..=0x7F => {} // Set display start line
            0xA0 | 0xA1 => {} // Segment re-map
            0xA4 | 0xA5 => {} // Display on/resume from GDDRAM
            0xC0 | 0xC8 => {} // COM output scan direction
            0xE3 => {}        // NOP
            _ => {
                // Unknown command, ignore
            }
        }
    }

    /// Receive a data byte (DC pin high)
    pub fn receive_data(&mut self, byte: u8) {
        self.dbg_data_count += 1;
        let x = self.col as usize;
        let page = self.page as usize;

        if x < SCREEN_WIDTH && page < 8 {
            // Pixel brightness scaled by contrast (0x00=black, 0xFF=full)
            let bright = self.contrast;
            // Each byte represents 8 vertical pixels in the current column
            for bit in 0..8u8 {
                let pixel_on = ((byte >> bit) & 1) != 0;
                let pixel_on = pixel_on ^ self.inverted;
                let y = page * 8 + bit as usize;
                if y < SCREEN_HEIGHT {
                    let offset = (y * SCREEN_WIDTH + x) * 4;
                    if pixel_on {
                        self.framebuffer[offset] = bright; // R
                        self.framebuffer[offset + 1] = bright; // G
                        self.framebuffer[offset + 2] = bright; // B
                        self.framebuffer[offset + 3] = 0xFF; // A
                    } else {
                        self.framebuffer[offset] = 0;
                        self.framebuffer[offset + 1] = 0;
                        self.framebuffer[offset + 2] = 0;
                        self.framebuffer[offset + 3] = 0xFF; // A always opaque
                    }
                }
            }
            self.dirty = true;
        }

        // Advance cursor
        self.col += 1;
        if self.col > self.col_end {
            self.col = self.col_start;
            self.page += 1;
            if self.page > self.page_end {
                self.page = self.page_start;
            }
        }
    }

    /// Reset per-frame debug counters
    pub fn dbg_reset_counters(&mut self) {
        self.dbg_cmd_count = 0;
        self.dbg_data_count = 0;
    }

    /// Convert framebuffer to u32 pixel array (0xRRGGBB format for minifb)
    pub fn as_pixel_buffer(&self) -> Vec<u32> {
        let mut pixels = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
        for i in 0..pixels.len() {
            let r = self.framebuffer[i * 4] as u32;
            let g = self.framebuffer[i * 4 + 1] as u32;
            let b = self.framebuffer[i * 4 + 2] as u32;
            pixels[i] = (r << 16) | (g << 8) | b;
        }
        pixels
    }

    /// Capture state for save state.
    pub fn save_state(&self) -> crate::savestate::Ssd1306State {
        crate::savestate::Ssd1306State {
            framebuffer: self.framebuffer.to_vec(),
            col: self.col,
            page: self.page,
            col_start: self.col_start,
            col_end: self.col_end,
            page_start: self.page_start,
            page_end: self.page_end,
            inverted: self.inverted,
            display_on: self.display_on,
            contrast: self.contrast,
        }
    }

    /// Restore state from save state.
    pub fn load_state(&mut self, s: &crate::savestate::Ssd1306State) {
        let len = s.framebuffer.len().min(self.framebuffer.len());
        self.framebuffer[..len].copy_from_slice(&s.framebuffer[..len]);
        self.col = s.col;
        self.page = s.page;
        self.col_start = s.col_start;
        self.col_end = s.col_end;
        self.page_start = s.page_start;
        self.page_end = s.page_end;
        self.inverted = s.inverted;
        self.display_on = s.display_on;
        self.contrast = s.contrast;
        self.cmd_state = CmdState::Ready;
        self.cmd_skip = 0;
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_creation() {
        let display = Ssd1306::new();
        assert_eq!(display.col_start, 0);
        assert_eq!(display.col_end, 127);
        assert_eq!(display.page_end, 7);
    }

    #[test]
    fn test_set_column_address() {
        let mut display = Ssd1306::new();
        display.receive_command(0x21); // Set column address
        display.receive_command(10); // Start column
        display.receive_command(50); // End column
        assert_eq!(display.col_start, 10);
        assert_eq!(display.col_end, 50);
        assert_eq!(display.col, 10);
    }

    #[test]
    fn test_write_pixel_data() {
        let mut display = Ssd1306::new();
        // Set cursor to start
        display.receive_command(0x21);
        display.receive_command(0);
        display.receive_command(127);
        display.receive_command(0x22);
        display.receive_command(0);
        display.receive_command(7);

        // Write 0xFF to first column - all 8 pixels on
        display.receive_data(0xFF);
        assert!(display.dirty);

        // Check first 8 pixels in column 0
        for bit in 0..8 {
            let offset = (bit * SCREEN_WIDTH) * 4;
            assert_eq!(
                display.framebuffer[offset], display.contrast,
                "pixel ({}, {}) should be on",
                0, bit
            );
        }
    }
}
