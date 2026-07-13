//! Intel HEX file parser.
//!
//! Parses Intel HEX format strings (`:LLAAAATT[DD...]CC`) and loads the
//! data into a flash memory buffer. Supports record types 00 (data),
//! 01 (EOF), and 02 (extended segment address) for programs up to 1 MB.

/// Parse Intel HEX format string and load into flash memory.
///
/// Returns the number of bytes loaded (highest address reached).
pub fn parse_hex(hex: &str, flash: &mut [u8]) -> Result<usize, String> {
    let mut max_addr = 0usize;
    let mut base_addr: u32 = 0;

    for line in hex.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(':') {
            continue; // skip non-hex lines
        }

        let bytes = hex_line_to_bytes(&line[1..])?;
        if bytes.len() < 5 {
            return Err("Line too short".into());
        }

        let byte_count = bytes[0] as usize;
        let addr = ((bytes[1] as u16) << 8) | bytes[2] as u16;
        let record_type = bytes[3];

        // Verify checksum
        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if sum != 0 {
            return Err(format!("Checksum error: sum={}", sum));
        }

        match record_type {
            0x00 => {
                // Data record
                let full_addr = base_addr + addr as u32;
                for i in 0..byte_count {
                    let target = (full_addr as usize) + i;
                    if target < flash.len() {
                        flash[target] = bytes[4 + i];
                        if target + 1 > max_addr {
                            max_addr = target + 1;
                        }
                    }
                }
            }
            0x01 => {
                // End of file
                break;
            }
            0x02 => {
                // Extended segment address
                if byte_count >= 2 {
                    base_addr = (((bytes[4] as u32) << 8) | bytes[5] as u32) << 4;
                }
            }
            0x03 => {
                // Start segment address (entry point) - ignore for loading
            }
            0x04 => {
                // Extended linear address
                if byte_count >= 2 {
                    base_addr = ((bytes[4] as u32) << 8 | bytes[5] as u32) << 16;
                }
            }
            0x05 => {
                // Start linear address - ignore
            }
            _ => {
                // Unknown record type, skip
            }
        }
    }

    Ok(max_addr)
}

/// Convert hex character pairs to bytes
fn hex_line_to_bytes(hex_str: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(hex_str.len() / 2);
    let chars: Vec<char> = hex_str.chars().collect();

    if chars.len() % 2 != 0 {
        return Err("Odd number of hex characters".into());
    }

    for chunk in chars.chunks(2) {
        let hi = hex_char(chunk[0])?;
        let lo = hex_char(chunk[1])?;
        bytes.push((hi << 4) | lo);
    }

    Ok(bytes)
}

fn hex_char(c: char) -> Result<u8, String> {
    match c {
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='f' => Ok(c as u8 - b'a' + 10),
        'A'..='F' => Ok(c as u8 - b'A' + 10),
        _ => Err(format!("Invalid hex character: {}", c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_hex() {
        let hex = ":100000000C9434000C944E000C944E000C944E0052\n:00000001FF\n";
        let mut flash = vec![0u8; 32768];
        let size = parse_hex(hex, &mut flash).unwrap();
        assert_eq!(size, 16);
        assert_eq!(flash[0], 0x0C);
        assert_eq!(flash[1], 0x94);
        assert_eq!(flash[2], 0x34);
        assert_eq!(flash[3], 0x00);
        assert_eq!(flash[4], 0x0C);
        assert_eq!(flash[5], 0x94);
    }

    #[test]
    fn test_checksum_error() {
        let hex = ":100000000C9434000C944E000C944E000C944E00FF\n:00000001FF\n";
        let mut flash = vec![0u8; 32768];
        assert!(parse_hex(hex, &mut flash).is_err());
    }

    #[test]
    fn test_empty_hex() {
        let hex = ":00000001FF\n";
        let mut flash = vec![0u8; 32768];
        let size = parse_hex(hex, &mut flash).unwrap();
        assert_eq!(size, 0);
    }
}
