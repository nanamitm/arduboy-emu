//! Minimal PNG encoder (no external dependencies).
//!
//! Generates valid PNG files using uncompressed (stored) deflate blocks.
//! This produces larger files than optimal but is simple and dependency-free.
//! Suitable for 128×64 Arduboy screenshots where file size is trivial.

/// Encode an RGBA pixel buffer as a PNG file.
///
/// `width` and `height` are in pixels. `rgba` contains `width * height * 4` bytes
/// in row-major RGBA order.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::with_capacity(rgba.len() + 1024);

    // PNG signature
    png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type: RGB (no alpha needed for Arduboy)
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut png, b"IHDR", &ihdr);

    // Build raw filtered data: filter byte (0=None) + RGB pixels per row
    let row_bytes = width as usize * 3 + 1; // filter byte + RGB
    let mut raw = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        raw.push(0); // filter: None
        for x in 0..width as usize {
            let offset = (y * width as usize + x) * 4;
            raw.push(rgba[offset]); // R
            raw.push(rgba[offset + 1]); // G
            raw.push(rgba[offset + 2]); // B
        }
    }

    // Wrap in zlib (stored blocks) and write IDAT
    let zlib_data = zlib_stored(&raw);
    write_chunk(&mut png, b"IDAT", &zlib_data);

    // IEND
    write_chunk(&mut png, b"IEND", &[]);

    png
}

/// Encode a monochrome (1-bit) image as a grayscale PNG.
///
/// `pixels` is a flat array of booleans (true = white, false = black).
pub fn encode_png_mono(width: u32, height: u32, pixels: &[bool]) -> Vec<u8> {
    let mut png = Vec::with_capacity(width as usize * height as usize + 1024);

    // PNG signature
    png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR: 8-bit grayscale
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(0); // color type: grayscale
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut png, b"IHDR", &ihdr);

    // Build filtered data
    let row_bytes = width as usize + 1;
    let mut raw = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        raw.push(0); // filter: None
        for x in 0..width as usize {
            raw.push(if pixels[y * width as usize + x] {
                255
            } else {
                0
            });
        }
    }

    let zlib_data = zlib_stored(&raw);
    write_chunk(&mut png, b"IDAT", &zlib_data);
    write_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    // CRC over type + data
    let crc = crc32(&chunk_type[..], data);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Wrap raw data in zlib format using stored (uncompressed) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 64);
    // zlib header: CMF=0x78 (deflate, window 32K), FLG=0x01 (check bits)
    out.push(0x78);
    out.push(0x01);

    // Emit stored deflate blocks (max 65535 bytes each)
    let mut pos = 0;
    while pos < data.len() {
        let remaining = data.len() - pos;
        let block_size = remaining.min(65535);
        let is_final = pos + block_size >= data.len();
        out.push(if is_final { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00
        let len = block_size as u16;
        out.push(len as u8);
        out.push((len >> 8) as u8);
        let nlen = !len;
        out.push(nlen as u8);
        out.push((nlen >> 8) as u8);
        out.extend_from_slice(&data[pos..pos + block_size]);
        pos += block_size;
    }

    // Adler-32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// CRC-32 (PNG/zlib)
fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}
