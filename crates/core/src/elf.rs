//! Minimal ELF and DWARF parser for AVR debug info.
//!
//! Reads ELF files (.elf) to extract:
//! - Flash contents (PT_LOAD segments)
//! - Symbol table (.symtab) for function name lookup
//! - DWARF .debug_line for source file + line number ↔ PC mapping
//!
//! ## Usage
//!
//! ```text
//! arduboy-emu game.elf --step
//! ```
//!
//! The parser handles only little-endian 32-bit ELF (EM_AVR = 83) as
//! produced by avr-gcc. DWARF versions 2–4 line programs are supported.

use std::collections::BTreeMap;

/// Parsed ELF file contents.
pub struct ElfFile {
    /// Flash image (from PT_LOAD segments below 0x800000)
    pub flash: Vec<u8>,
    /// Symbol table: byte_address → function_name
    pub symbols: BTreeMap<u32, String>,
    /// Sorted symbol addresses for reverse lookup
    sym_addrs: Vec<u32>,
    /// Source line map: byte_address → (file, line)
    pub line_map: BTreeMap<u32, (String, u32)>,
    /// Sorted line addresses for reverse lookup
    line_addrs: Vec<u32>,
    /// Entry point (byte address)
    pub entry: u32,
}

// ELF constants
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const EM_AVR: u16 = 83;
const PT_LOAD: u32 = 1;
const SHT_SYMTAB: u32 = 2;

fn u16le(d: &[u8], o: usize) -> u16 {
    (d[o] as u16) | ((d[o + 1] as u16) << 8)
}
fn u32le(d: &[u8], o: usize) -> u32 {
    (d[o] as u32) | ((d[o + 1] as u32) << 8) | ((d[o + 2] as u32) << 16) | ((d[o + 3] as u32) << 24)
}
fn read_str(d: &[u8], o: usize) -> String {
    if o >= d.len() {
        return String::new();
    }
    let end = d[o..].iter().position(|&b| b == 0).unwrap_or(0);
    String::from_utf8_lossy(&d[o..o + end]).into_owned()
}
fn read_uleb128(d: &[u8], p: &mut usize) -> u32 {
    let mut r = 0u32;
    let mut s = 0;
    loop {
        if *p >= d.len() {
            break;
        }
        let b = d[*p];
        *p += 1;
        r |= ((b & 0x7F) as u32) << s;
        if b & 0x80 == 0 {
            break;
        }
        s += 7;
        if s >= 35 {
            break;
        }
    }
    r
}
fn read_sleb128(d: &[u8], p: &mut usize) -> i32 {
    let mut r = 0i32;
    let mut s = 0u32;
    let mut b;
    loop {
        if *p >= d.len() {
            return r;
        }
        b = d[*p];
        *p += 1;
        r |= ((b & 0x7F) as i32) << s;
        s += 7;
        if b & 0x80 == 0 {
            break;
        }
        if s >= 35 {
            break;
        }
    }
    if s < 32 && (b & 0x40) != 0 {
        r |= !0i32 << s;
    }
    r
}

/// Parse an ELF file from raw bytes.
pub fn parse_elf(data: &[u8]) -> Result<ElfFile, String> {
    if data.len() < 52 {
        return Err("File too small for ELF header".into());
    }
    if data[0..4] != ELF_MAGIC {
        return Err("Not an ELF file".into());
    }
    if data[4] != 1 {
        return Err("Only 32-bit ELF supported".into());
    }
    if data[5] != 1 {
        return Err("Only little-endian ELF supported".into());
    }
    let e_machine = u16le(data, 18);
    if e_machine != EM_AVR {
        return Err(format!("Not AVR ELF (machine={})", e_machine));
    }

    let entry = u32le(data, 24);
    let e_phoff = u32le(data, 28) as usize;
    let e_shoff = u32le(data, 32) as usize;
    let e_phentsize = u16le(data, 42) as usize;
    let e_phnum = u16le(data, 44) as usize;
    let e_shentsize = u16le(data, 46) as usize;
    let e_shnum = u16le(data, 48) as usize;
    let e_shstrndx = u16le(data, 50) as usize;

    // ── Load program segments ──────────────────────────────────────────
    let mut flash = vec![0u8; 32768]; // 32KB default
    for i in 0..e_phnum {
        let off = e_phoff + i * e_phentsize;
        if off + e_phentsize > data.len() {
            break;
        }
        let p_type = u32le(data, off);
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u32le(data, off + 4) as usize;
        let p_vaddr = u32le(data, off + 8) as usize;
        let p_filesz = u32le(data, off + 16) as usize;
        if p_vaddr < 0x800000 && p_offset + p_filesz <= data.len() {
            let end = p_vaddr + p_filesz;
            if end > flash.len() {
                flash.resize(end, 0xFF);
            }
            flash[p_vaddr..end].copy_from_slice(&data[p_offset..p_offset + p_filesz]);
        }
    }

    // ── Section headers ────────────────────────────────────────────────
    let shstrtab_off = if e_shstrndx < e_shnum {
        let sh = e_shoff + e_shstrndx * e_shentsize;
        if sh + e_shentsize <= data.len() {
            u32le(data, sh + 16) as usize
        } else {
            0
        }
    } else {
        0
    };

    let mut symtab_off = 0usize;
    let mut symtab_size = 0usize;
    let mut symtab_entsize = 16usize;
    let mut symtab_link = 0usize;
    let mut debug_line_off = 0usize;
    let mut debug_line_size = 0usize;

    for i in 0..e_shnum {
        let sh = e_shoff + i * e_shentsize;
        if sh + e_shentsize > data.len() {
            break;
        }
        let sh_name = u32le(data, sh) as usize;
        let sh_type = u32le(data, sh + 4);
        let sh_offset = u32le(data, sh + 16) as usize;
        let sh_size = u32le(data, sh + 20) as usize;
        let sh_link = u32le(data, sh + 24) as usize;
        let sh_entsize = u32le(data, sh + 36) as usize;

        if sh_type == SHT_SYMTAB {
            symtab_off = sh_offset;
            symtab_size = sh_size;
            symtab_entsize = if sh_entsize > 0 { sh_entsize } else { 16 };
            symtab_link = sh_link;
        }
        let name = read_str(data, shstrtab_off + sh_name);
        if name == ".debug_line" {
            debug_line_off = sh_offset;
            debug_line_size = sh_size;
        }
    }

    // ── Symbol table ───────────────────────────────────────────────────
    let mut symbols = BTreeMap::new();
    if symtab_off > 0 {
        let strtab_off = if symtab_link < e_shnum {
            let sh = e_shoff + symtab_link * e_shentsize;
            if sh + e_shentsize <= data.len() {
                u32le(data, sh + 16) as usize
            } else {
                0
            }
        } else {
            0
        };

        let count = symtab_size / symtab_entsize;
        for i in 0..count {
            let off = symtab_off + i * symtab_entsize;
            if off + symtab_entsize > data.len() {
                break;
            }
            let st_name = u32le(data, off) as usize;
            let st_value = u32le(data, off + 4);
            let st_info = data[off + 12];
            let st_type = st_info & 0xF;
            // STT_FUNC=2, STT_OBJECT=1
            if (st_type == 2 || st_type == 1) && st_name > 0 {
                let name = read_str(data, strtab_off + st_name);
                if !name.is_empty() {
                    symbols.insert(st_value, name);
                }
            }
        }
    }

    // ── DWARF .debug_line ──────────────────────────────────────────────
    let line_map = if debug_line_off > 0
        && debug_line_size > 0
        && debug_line_off + debug_line_size <= data.len()
    {
        parse_debug_line(&data[debug_line_off..debug_line_off + debug_line_size])
    } else {
        BTreeMap::new()
    };

    let sym_addrs: Vec<u32> = symbols.keys().copied().collect();
    let line_addrs: Vec<u32> = line_map.keys().copied().collect();

    Ok(ElfFile {
        flash,
        symbols,
        sym_addrs,
        line_map,
        line_addrs,
        entry,
    })
}

impl ElfFile {
    /// Find function name containing byte address (nearest symbol at or below).
    pub fn find_function(&self, byte_addr: u32) -> Option<(&str, u32)> {
        let idx = self.sym_addrs.partition_point(|&a| a <= byte_addr);
        if idx == 0 {
            return None;
        }
        let sym_addr = self.sym_addrs[idx - 1];
        let name = self.symbols.get(&sym_addr)?;
        Some((name.as_str(), byte_addr - sym_addr))
    }

    /// Find source file:line for byte address (nearest entry at or below).
    pub fn find_line(&self, byte_addr: u32) -> Option<(&str, u32)> {
        let idx = self.line_addrs.partition_point(|&a| a <= byte_addr);
        if idx == 0 {
            return None;
        }
        let line_addr = self.line_addrs[idx - 1];
        let (file, line) = self.line_map.get(&line_addr)?;
        Some((file.as_str(), *line))
    }

    /// Format symbol + source for a given PC word address.
    pub fn describe_pc(&self, pc_word: u16) -> String {
        let addr = (pc_word as u32) * 2;
        let mut parts = Vec::new();
        if let Some((name, offset)) = self.find_function(addr) {
            if offset == 0 {
                parts.push(format!("<{}>", name));
            } else {
                parts.push(format!("<{}+{}>", name, offset));
            }
        }
        if let Some((file, line)) = self.find_line(addr) {
            let short = file
                .rsplit('/')
                .next()
                .unwrap_or(file)
                .rsplit('\\')
                .next()
                .unwrap_or(file);
            parts.push(format!("{}:{}", short, line));
        }
        if parts.is_empty() {
            return String::new();
        }
        parts.join(" ")
    }
}

/// Parse DWARF .debug_line section (version 2–4).
fn parse_debug_line(section: &[u8]) -> BTreeMap<u32, (String, u32)> {
    let mut result = BTreeMap::new();
    let mut pos = 0;

    while pos + 10 < section.len() {
        let unit_length = u32le(section, pos) as usize;
        if unit_length == 0 {
            break;
        }
        let unit_end = (pos + 4 + unit_length).min(section.len());
        pos += 4;

        let version = u16le(section, pos);
        pos += 2;
        if version < 2 || version > 4 {
            pos = unit_end;
            continue;
        }

        let header_length = u32le(section, pos) as usize;
        pos += 4;
        let prog_start = pos + header_length;
        if prog_start > unit_end {
            pos = unit_end;
            continue;
        }

        let min_inst_len = section[pos] as u32;
        pos += 1;
        if version >= 4 {
            pos += 1;
        } // max_ops_per_instruction
        let _default_is_stmt = section[pos];
        pos += 1;
        let line_base = section[pos] as i8;
        pos += 1;
        let line_range = section[pos] as u32;
        pos += 1;
        let opcode_base = section[pos];
        pos += 1;

        // Standard opcode lengths
        let mut std_lens = vec![0u8; opcode_base as usize];
        for j in 1..(opcode_base as usize) {
            if pos < section.len() {
                std_lens[j] = section[pos];
                pos += 1;
            }
        }

        // Directory table (null-terminated strings, ended by empty string)
        let mut dirs: Vec<String> = vec!["".into()]; // dir index 0 = compilation dir
        loop {
            if pos >= section.len() || section[pos] == 0 {
                pos += 1;
                break;
            }
            let s = read_str(section, pos);
            pos += s.len() + 1;
            dirs.push(s);
        }

        // File name table
        let mut files: Vec<String> = vec!["<unknown>".into()]; // file index 0
        loop {
            if pos >= section.len() || section[pos] == 0 {
                break;
            }
            let name = read_str(section, pos);
            pos += name.len() + 1;
            let dir_idx = read_uleb128(section, &mut pos) as usize;
            let _mtime = read_uleb128(section, &mut pos);
            let _fsize = read_uleb128(section, &mut pos);
            // Build full path
            let full = if dir_idx > 0 && dir_idx < dirs.len() && !dirs[dir_idx].is_empty() {
                format!("{}/{}", dirs[dir_idx], name)
            } else {
                name
            };
            files.push(full);
        }

        // Execute line number program
        pos = prog_start.min(section.len());
        let mut address = 0u32;
        let mut file = 1u32;
        let mut line = 1u32;
        let mut end_sequence = false;

        while pos < unit_end {
            let op = section[pos];
            pos += 1;

            if op == 0 {
                // Extended opcode
                let ext_len = read_uleb128(section, &mut pos) as usize;
                let ext_end = pos + ext_len;
                if pos >= unit_end {
                    break;
                }
                let ext_op = section[pos];
                pos += 1;

                match ext_op {
                    1 => {
                        // end_sequence
                        end_sequence = true;
                        address = 0;
                        file = 1;
                        line = 1;
                    }
                    2 => {
                        // set_address
                        if pos + 3 < section.len() {
                            address = u32le(section, pos);
                        }
                        pos = ext_end;
                    }
                    4 => {
                        // define_file
                        let n = read_str(section, pos);
                        files.push(n);
                        pos = ext_end;
                    }
                    _ => {
                        pos = ext_end;
                    }
                }
                if end_sequence {
                    end_sequence = false;
                    continue;
                }
            } else if op < opcode_base {
                match op {
                    1 => {
                        // copy
                        let f = files.get(file as usize).cloned().unwrap_or_default();
                        result.insert(address, (f, line));
                    }
                    2 => {
                        address += read_uleb128(section, &mut pos) * min_inst_len;
                    }
                    3 => {
                        line = (line as i32 + read_sleb128(section, &mut pos)) as u32;
                    }
                    4 => {
                        file = read_uleb128(section, &mut pos);
                    }
                    5 => {
                        let _ = read_uleb128(section, &mut pos);
                    } // set_column
                    6 | 7 => {} // negate_stmt, set_basic_block
                    8 => {
                        // const_add_pc
                        let adj = ((255 - opcode_base) as u32 / line_range.max(1)) * min_inst_len;
                        address += adj;
                    }
                    9 => {
                        // fixed_advance_pc
                        if pos + 1 < section.len() {
                            address += u16le(section, pos) as u32;
                        }
                        pos += 2;
                    }
                    10 => {} // set_prologue_end
                    11 => {} // set_epilogue_begin
                    12 => {
                        let _ = read_uleb128(section, &mut pos);
                    } // set_isa
                    _ => {
                        // Skip unknown standard opcodes using length table
                        let n = if (op as usize) < std_lens.len() {
                            std_lens[op as usize]
                        } else {
                            0
                        };
                        for _ in 0..n {
                            let _ = read_uleb128(section, &mut pos);
                        }
                    }
                }
            } else {
                // Special opcode
                let adjusted = (op - opcode_base) as u32;
                let addr_inc = (adjusted / line_range.max(1)) * min_inst_len;
                let line_inc = line_base as i32 + (adjusted % line_range.max(1)) as i32;
                address += addr_inc;
                line = (line as i32 + line_inc) as u32;
                let f = files.get(file as usize).cloned().unwrap_or_default();
                result.insert(address, (f, line));
            }
        }
        pos = unit_end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uleb128() {
        let d = [0x80, 0x01];
        let mut p = 0;
        assert_eq!(read_uleb128(&d, &mut p), 128);
    }

    #[test]
    fn test_sleb128() {
        let d = [0x7F];
        let mut p = 0;
        assert_eq!(read_sleb128(&d, &mut p), -1);
    }

    #[test]
    fn test_bad_magic() {
        assert!(parse_elf(&[0u8; 64]).is_err());
    }

    #[test]
    fn test_too_short() {
        assert!(parse_elf(&[0x7F, b'E', b'L', b'F']).is_err());
    }

    #[test]
    fn test_find_function() {
        let mut elf = ElfFile {
            flash: vec![],
            symbols: BTreeMap::new(),
            sym_addrs: vec![],
            line_map: BTreeMap::new(),
            line_addrs: vec![],
            entry: 0,
        };
        elf.symbols.insert(0x100, "main".into());
        elf.symbols.insert(0x200, "loop".into());
        elf.sym_addrs = elf.symbols.keys().copied().collect();
        assert_eq!(elf.find_function(0x100), Some(("main", 0)));
        assert_eq!(elf.find_function(0x110), Some(("main", 16)));
        assert_eq!(elf.find_function(0x200), Some(("loop", 0)));
        assert_eq!(elf.find_function(0x050), None);
    }
}
