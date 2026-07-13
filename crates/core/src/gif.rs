//! Minimal animated GIF encoder (no external dependencies).
//!
//! Produces GIF89a files with LZW-compressed frames. Optimized for
//! the Arduboy's monochrome 128×64 display using a 2-color palette.

/// Builder for animated GIF files.
pub struct GifEncoder {
    width: u16,
    height: u16,
    /// Delay between frames in centiseconds (e.g., 2 = 20ms ≈ 50fps)
    pub delay_cs: u16,
    /// Accumulated GIF data
    data: Vec<u8>,
    frame_count: u32,
    finished: bool,
}

impl GifEncoder {
    /// Create a new GIF encoder with the given dimensions.
    ///
    /// `delay_cs` is the delay between frames in 1/100ths of a second.
    /// For 60fps Arduboy, use 2 (20ms).
    pub fn new(width: u16, height: u16, delay_cs: u16) -> Self {
        let mut data = Vec::with_capacity(65536);

        // GIF89a header
        data.extend_from_slice(b"GIF89a");

        // Logical Screen Descriptor
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.push(0x80); // GCT flag + 1 bit color (2 entries)
        data.push(0x00); // background color index
        data.push(0x00); // pixel aspect ratio

        // Global Color Table (2 entries: black, white)
        data.extend_from_slice(&[0x00, 0x00, 0x00]); // index 0: black
        data.extend_from_slice(&[0xFF, 0xFF, 0xFF]); // index 1: white

        // Netscape Application Extension (infinite loop)
        data.extend_from_slice(&[
            0x21, 0xFF, 0x0B, b'N', b'E', b'T', b'S', b'C', b'A', b'P', b'E', b'2', b'.', b'0',
            0x03, 0x01, 0x00, 0x00, // loop count = 0 (infinite)
            0x00, // block terminator
        ]);

        GifEncoder {
            width,
            height,
            delay_cs,
            data,
            frame_count: 0,
            finished: false,
        }
    }

    /// Add a frame from a flat array of pixel indices (0=black, 1=white).
    ///
    /// `pixels` must have exactly `width * height` elements.
    pub fn add_frame(&mut self, pixels: &[u8]) {
        if self.finished {
            return;
        }

        // Graphic Control Extension
        self.data.push(0x21); // extension introducer
        self.data.push(0xF9); // graphic control label
        self.data.push(0x04); // block size
        self.data.push(0x00); // disposal: none, no transparency
        self.data.extend_from_slice(&self.delay_cs.to_le_bytes());
        self.data.push(0x00); // transparent color index (unused)
        self.data.push(0x00); // block terminator

        // Image Descriptor
        self.data.push(0x2C); // image separator
        self.data.extend_from_slice(&0u16.to_le_bytes()); // left
        self.data.extend_from_slice(&0u16.to_le_bytes()); // top
        self.data.extend_from_slice(&self.width.to_le_bytes());
        self.data.extend_from_slice(&self.height.to_le_bytes());
        self.data.push(0x00); // no local color table, not interlaced

        // LZW compressed image data
        let min_code_size = 2; // minimum for GIF (even though we only need 1 bit)
        self.data.push(min_code_size);
        let compressed = lzw_compress(pixels, min_code_size);
        // Write sub-blocks (max 255 bytes each)
        let mut pos = 0;
        while pos < compressed.len() {
            let block_size = (compressed.len() - pos).min(255);
            self.data.push(block_size as u8);
            self.data
                .extend_from_slice(&compressed[pos..pos + block_size]);
            pos += block_size;
        }
        self.data.push(0x00); // block terminator

        self.frame_count += 1;
    }

    /// Add a frame from a monochrome boolean array.
    pub fn add_frame_mono(&mut self, pixels: &[bool]) {
        let indices: Vec<u8> = pixels.iter().map(|&b| if b { 1 } else { 0 }).collect();
        self.add_frame(&indices);
    }

    /// Finalize the GIF and return the complete file data.
    pub fn finish(mut self) -> Vec<u8> {
        if !self.finished {
            self.data.push(0x3B); // GIF trailer
            self.finished = true;
        }
        self.data
    }

    /// Number of frames added so far.
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }
}

// ─── LZW Compression ────────────────────────────────────────────────────────

fn lzw_compress(data: &[u8], min_code_size: u8) -> Vec<u8> {
    let clear_code = 1u32 << min_code_size;
    let eoi_code = clear_code + 1;

    let mut output = BitWriter::new();
    let mut code_size = min_code_size as u32 + 1;
    let mut next_code = eoi_code + 1;
    let max_table_size = 4096u32;

    // Simple dictionary: Vec of (prefix_code, byte) → code
    // For the Arduboy's monochrome output, the dictionary stays small
    let mut table: Vec<(u32, u8)> = Vec::with_capacity(4096);

    // Emit clear code
    output.write_bits(clear_code, code_size);

    if data.is_empty() {
        output.write_bits(eoi_code, code_size);
        return output.finish();
    }

    let mut prefix = data[0] as u32;

    for &byte in &data[1..] {
        // Search for (prefix, byte) in table
        let entry = table.iter().position(|&(p, b)| p == prefix && b == byte);

        if let Some(idx) = entry {
            prefix = eoi_code + 1 + idx as u32;
        } else {
            // Output prefix code
            output.write_bits(prefix, code_size);

            // Add new entry if table isn't full
            if next_code < max_table_size {
                table.push((prefix, byte));
                next_code += 1;
                // Increase code size when needed
                if next_code > (1 << code_size) && code_size < 12 {
                    code_size += 1;
                }
            } else {
                // Table full: emit clear code and reset
                output.write_bits(clear_code, code_size);
                table.clear();
                code_size = min_code_size as u32 + 1;
                next_code = eoi_code + 1;
            }

            prefix = byte as u32;
        }
    }

    // Output final prefix
    output.write_bits(prefix, code_size);
    // End of information
    output.write_bits(eoi_code, code_size);

    output.finish()
}

struct BitWriter {
    data: Vec<u8>,
    current: u32,
    bits: u32,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            data: Vec::with_capacity(8192),
            current: 0,
            bits: 0,
        }
    }

    fn write_bits(&mut self, value: u32, num_bits: u32) {
        self.current |= value << self.bits;
        self.bits += num_bits;
        while self.bits >= 8 {
            self.data.push(self.current as u8);
            self.current >>= 8;
            self.bits -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.data.push(self.current as u8);
        }
        self.data
    }
}
