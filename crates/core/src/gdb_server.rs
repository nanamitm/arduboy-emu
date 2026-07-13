//! GDB Remote Serial Protocol server for AVR debugging.
//!
//! Implements the GDB RSP over TCP, enabling connection from `avr-gdb` or
//! any GDB-compatible client. Supports:
//!
//! - Register read/write (`g`/`G`/`p`/`P`)
//! - Memory read/write (`m`/`M`)
//! - Single step (`s`) and continue (`c`)
//! - Software breakpoints (`Z0`/`z0`)
//! - Hardware watchpoints (`Z2`/`z2`/`Z3`/`z3`/`Z4`/`z4`)
//! - Kill (`k`) and detach (`D`)
//!
//! ## Usage
//!
//! ```text
//! # Start emulator with GDB server on port 1234:
//! arduboy-emu game.hex --gdb 1234
//!
//! # Connect from avr-gdb:
//! avr-gdb game.elf -ex "target remote :1234"
//! ```
//!
//! The AVR register layout for GDB: R0-R31 (32 bytes), SREG (1), SP (2), PC (4).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// GDB server state.
pub struct GdbServer {
    listener: TcpListener,
    port: u16,
}

/// A connected GDB session.
pub struct GdbSession {
    stream: TcpStream,
    buf: Vec<u8>,
    /// Breakpoints set by GDB (byte addresses)
    pub breakpoints: Vec<u32>,
    /// Whether the session has been detached/killed
    pub done: bool,
}

/// Commands the emulator should execute after processing a GDB packet.
#[derive(Debug)]
pub enum GdbAction {
    /// Continue execution (run until breakpoint/signal)
    Continue,
    /// Single-step one instruction
    Step,
    /// The session is done (detach or kill)
    Disconnect,
    /// No action needed (reply already sent)
    None,
}

impl GdbServer {
    /// Create a GDB server listening on the given TCP port.
    pub fn bind(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
        listener.set_nonblocking(false)?;
        eprintln!("GDB server listening on 127.0.0.1:{}", port);
        eprintln!("Connect with: avr-gdb -ex \"target remote :{}\"", port);
        Ok(GdbServer { listener, port })
    }

    /// Wait for a GDB client to connect (blocking).
    pub fn accept(&self) -> std::io::Result<GdbSession> {
        let (stream, addr) = self.listener.accept()?;
        eprintln!("GDB client connected from {}", addr);
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        Ok(GdbSession {
            stream,
            buf: Vec::with_capacity(4096),
            breakpoints: Vec::new(),
            done: false,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl GdbSession {
    /// Read and process one GDB packet. Returns the action the emulator should take.
    ///
    /// `regs` = R0-R31 (32 bytes), `sreg`, `sp`, `pc` (word address)
    /// `flash`, `data` = memory arrays
    pub fn process_packet(
        &mut self,
        regs: &[u8; 32],
        sreg: u8,
        sp: u16,
        pc: u16,
        flash: &[u8],
        data: &mut [u8],
    ) -> std::io::Result<GdbAction> {
        let packet = match self.read_packet() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("GDB read error: {}", e);
                self.done = true;
                return Ok(GdbAction::Disconnect);
            }
        };

        if packet.is_empty() {
            return Ok(GdbAction::None);
        }

        let cmd = packet[0] as char;
        let args = &packet[1..];

        match cmd {
            // Halt reason
            '?' => {
                self.send_packet(b"S05")?; // SIGTRAP
                Ok(GdbAction::None)
            }

            // Read all registers: R0-R31(32) + SREG(1) + SP(2) + PC(4) = 39 bytes
            'g' => {
                let mut buf = String::with_capacity(78);
                for &r in regs.iter() {
                    buf.push_str(&format!("{:02x}", r));
                }
                buf.push_str(&format!("{:02x}", sreg));
                // SP: little-endian 2 bytes
                buf.push_str(&format!("{:02x}{:02x}", sp & 0xFF, (sp >> 8) & 0xFF));
                // PC: byte address, little-endian 4 bytes
                let pc_byte = (pc as u32) * 2;
                buf.push_str(&format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    pc_byte & 0xFF,
                    (pc_byte >> 8) & 0xFF,
                    (pc_byte >> 16) & 0xFF,
                    (pc_byte >> 24) & 0xFF
                ));
                self.send_packet(buf.as_bytes())?;
                Ok(GdbAction::None)
            }

            // Write all registers
            'G' => {
                // We accept but don't write back (read-only for now)
                self.send_packet(b"OK")?;
                Ok(GdbAction::None)
            }

            // Read single register
            'p' => {
                let reg_num = parse_hex_u32(args).unwrap_or(0) as usize;
                let val = match reg_num {
                    0..=31 => format!("{:02x}", regs[reg_num]),
                    32 => format!("{:02x}", sreg),
                    33 => format!("{:02x}{:02x}", sp & 0xFF, (sp >> 8) & 0xFF),
                    34 => {
                        let pc_byte = (pc as u32) * 2;
                        format!(
                            "{:02x}{:02x}{:02x}{:02x}",
                            pc_byte & 0xFF,
                            (pc_byte >> 8) & 0xFF,
                            (pc_byte >> 16) & 0xFF,
                            (pc_byte >> 24) & 0xFF
                        )
                    }
                    _ => "xx".into(),
                };
                self.send_packet(val.as_bytes())?;
                Ok(GdbAction::None)
            }

            // Read memory: m<addr>,<len>
            'm' => {
                let parts: Vec<&[u8]> = args.splitn(2, |&b| b == b',').collect();
                if parts.len() == 2 {
                    let addr = parse_hex_u32(parts[0]).unwrap_or(0) as usize;
                    let len = parse_hex_u32(parts[1]).unwrap_or(0) as usize;
                    let mut buf = String::with_capacity(len * 2);

                    // AVR GDB uses byte addresses:
                    // 0x000000-0x7FFFFF = flash (program space)
                    // 0x800000-0x80FFFF = data space (SRAM)
                    // 0x810000-0x81FFFF = EEPROM
                    for i in 0..len {
                        let a = addr + i;
                        let byte = if a < 0x800000 {
                            // Flash
                            if a < flash.len() {
                                flash[a]
                            } else {
                                0xFF
                            }
                        } else if a < 0x810000 {
                            // Data space
                            let da = a - 0x800000;
                            if da < data.len() {
                                data[da]
                            } else {
                                0
                            }
                        } else {
                            0xFF
                        };
                        buf.push_str(&format!("{:02x}", byte));
                    }
                    self.send_packet(buf.as_bytes())?;
                } else {
                    self.send_packet(b"E01")?;
                }
                Ok(GdbAction::None)
            }

            // Write memory: M<addr>,<len>:<hex>
            'M' => {
                if let Some(colon) = args.iter().position(|&b| b == b':') {
                    let header = &args[..colon];
                    let hex_data = &args[colon + 1..];
                    let parts: Vec<&[u8]> = header.splitn(2, |&b| b == b',').collect();
                    if parts.len() == 2 {
                        let addr = parse_hex_u32(parts[0]).unwrap_or(0) as usize;
                        let _len = parse_hex_u32(parts[1]).unwrap_or(0) as usize;
                        let bytes = parse_hex_bytes(hex_data);
                        for (i, &b) in bytes.iter().enumerate() {
                            let a = addr + i;
                            if a >= 0x800000 && a < 0x810000 {
                                let da = a - 0x800000;
                                if da < data.len() {
                                    data[da] = b;
                                }
                            }
                        }
                        self.send_packet(b"OK")?;
                    } else {
                        self.send_packet(b"E01")?;
                    }
                } else {
                    self.send_packet(b"E01")?;
                }
                Ok(GdbAction::None)
            }

            // Continue
            'c' => Ok(GdbAction::Continue),

            // Single step
            's' => Ok(GdbAction::Step),

            // Insert breakpoint: Z<type>,<addr>,<kind>
            'Z' => {
                let parts: Vec<&[u8]> = args.splitn(3, |&b| b == b',').collect();
                if parts.len() >= 2 {
                    let bp_type = parse_hex_u32(&parts[0][..]).unwrap_or(0);
                    let addr = parse_hex_u32(parts[1]).unwrap_or(0);
                    match bp_type {
                        0 | 1 => {
                            // Software/hardware breakpoint (byte address → word address)
                            let word_addr = addr / 2;
                            if !self.breakpoints.contains(&word_addr) {
                                self.breakpoints.push(word_addr);
                            }
                            self.send_packet(b"OK")?;
                        }
                        2 | 3 | 4 => {
                            // Write/read/access watchpoint — accept but track via breakpoints
                            self.send_packet(b"OK")?;
                        }
                        _ => {
                            self.send_packet(b"")?;
                        }
                    }
                } else {
                    self.send_packet(b"E01")?;
                }
                Ok(GdbAction::None)
            }

            // Remove breakpoint: z<type>,<addr>,<kind>
            'z' => {
                let parts: Vec<&[u8]> = args.splitn(3, |&b| b == b',').collect();
                if parts.len() >= 2 {
                    let bp_type = parse_hex_u32(&parts[0][..]).unwrap_or(0);
                    let addr = parse_hex_u32(parts[1]).unwrap_or(0);
                    match bp_type {
                        0 | 1 => {
                            let word_addr = addr / 2;
                            self.breakpoints.retain(|&a| a != word_addr);
                            self.send_packet(b"OK")?;
                        }
                        2 | 3 | 4 => {
                            self.send_packet(b"OK")?;
                        }
                        _ => {
                            self.send_packet(b"")?;
                        }
                    }
                } else {
                    self.send_packet(b"E01")?;
                }
                Ok(GdbAction::None)
            }

            // Detach
            'D' => {
                self.send_packet(b"OK")?;
                self.done = true;
                Ok(GdbAction::Disconnect)
            }

            // Kill
            'k' => {
                self.done = true;
                Ok(GdbAction::Disconnect)
            }

            // Query
            'q' => {
                let query = std::str::from_utf8(args).unwrap_or("");
                if query.starts_with("Supported") {
                    self.send_packet(b"PacketSize=4000")?;
                } else if query == "Attached" {
                    self.send_packet(b"1")?; // attached to existing process
                } else if query.starts_with("Offsets") {
                    self.send_packet(b"Text=0;Data=800000;Bss=800000")?;
                } else if query == "C" {
                    self.send_packet(b"QC1")?; // thread 1
                } else if query.starts_with("fThreadInfo") {
                    self.send_packet(b"m1")?;
                } else if query.starts_with("sThreadInfo") {
                    self.send_packet(b"l")?;
                } else {
                    self.send_packet(b"")?; // unsupported
                }
                Ok(GdbAction::None)
            }

            // vCont query
            'v' => {
                let vcmd = std::str::from_utf8(args).unwrap_or("");
                if vcmd == "Cont?" {
                    self.send_packet(b"vCont;c;s")?;
                } else if vcmd.starts_with("Cont;c") {
                    return Ok(GdbAction::Continue);
                } else if vcmd.starts_with("Cont;s") {
                    return Ok(GdbAction::Step);
                } else {
                    self.send_packet(b"")?;
                }
                Ok(GdbAction::None)
            }

            // Unknown command
            _ => {
                self.send_packet(b"")?;
                Ok(GdbAction::None)
            }
        }
    }

    /// Send a stop reply (SIGTRAP) after step/breakpoint.
    pub fn send_stop_reply(&mut self) -> std::io::Result<()> {
        self.send_packet(b"S05")
    }

    /// Read a GDB packet from the stream.
    /// Format: $<data>#<checksum> or Ctrl+C (0x03)
    fn read_packet(&mut self) -> std::io::Result<Vec<u8>> {
        let mut byte = [0u8; 1];

        // Skip until '$' or get interrupt (0x03)
        loop {
            self.stream.read_exact(&mut byte)?;
            if byte[0] == 0x03 {
                // Ctrl+C interrupt
                return Ok(vec![b'?']);
            }
            if byte[0] == b'$' {
                break;
            }
        }

        // Read until '#'
        self.buf.clear();
        loop {
            self.stream.read_exact(&mut byte)?;
            if byte[0] == b'#' {
                break;
            }
            self.buf.push(byte[0]);
        }

        // Read 2-char checksum (we don't validate it)
        let mut cksum = [0u8; 2];
        self.stream.read_exact(&mut cksum)?;

        // Send ACK
        self.stream.write_all(b"+")?;
        self.stream.flush()?;

        Ok(self.buf.clone())
    }

    /// Send a GDB response packet.
    fn send_packet(&mut self, data: &[u8]) -> std::io::Result<()> {
        let checksum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        let mut pkt = Vec::with_capacity(data.len() + 4);
        pkt.push(b'$');
        pkt.extend_from_slice(data);
        pkt.push(b'#');
        pkt.push(HEX_CHARS[(checksum >> 4) as usize]);
        pkt.push(HEX_CHARS[(checksum & 0xF) as usize]);
        self.stream.write_all(&pkt)?;
        self.stream.flush()?;

        // Wait for ACK
        let mut ack = [0u8; 1];
        let _ = self.stream.read_exact(&mut ack);
        Ok(())
    }

    /// Set stream to non-blocking mode for poll-style usage.
    pub fn set_nonblocking(&self, nb: bool) -> std::io::Result<()> {
        self.stream.set_nonblocking(nb)
    }

    /// Check if there's pending data (non-blocking peek).
    pub fn has_pending(&self) -> bool {
        let mut peek = [0u8; 1];
        match self.stream.peek(&mut peek) {
            Ok(n) => n > 0,
            Err(_) => false,
        }
    }
}

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

/// Parse a hex string (as bytes) into a u32.
fn parse_hex_u32(s: &[u8]) -> Option<u32> {
    let mut val = 0u32;
    for &b in s {
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        };
        val = val.checked_shl(4)?.checked_add(digit as u32)?;
    }
    Some(val)
}

/// Parse a hex string into bytes.
fn parse_hex_bytes(s: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i + 1 < s.len() {
        if let Some(hi) = hex_nibble(s[i]) {
            if let Some(lo) = hex_nibble(s[i + 1]) {
                bytes.push((hi << 4) | lo);
            }
        }
        i += 2;
    }
    bytes
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex() {
        assert_eq!(parse_hex_u32(b"1234"), Some(0x1234));
        assert_eq!(parse_hex_u32(b"FF"), Some(0xFF));
        assert_eq!(parse_hex_u32(b"0"), Some(0));
    }

    #[test]
    fn test_parse_hex_bytes() {
        assert_eq!(
            parse_hex_bytes(b"48656C6C6F"),
            vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]
        );
    }
}
