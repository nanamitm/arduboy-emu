//! `.arduboy` file parser.
//!
//! An `.arduboy` file is a ZIP archive containing:
//! - `info.json` — metadata (title, author, description)
//! - `*.hex` — Intel HEX game binary
//! - `*-fx.bin` or `*.bin` — optional FX flash data
//!
//! This module provides a minimal ZIP reader (stored + deflate via miniz_oxide)
//! to extract these files.

use std::collections::HashMap;

/// Parsed contents of an .arduboy file.
#[derive(Debug, Default)]
pub struct ArduboyFile {
    /// Game title from info.json (if present).
    pub title: String,
    /// Author from info.json (if present).
    pub author: String,
    /// Intel HEX data as a string.
    pub hex: Option<String>,
    /// FX flash binary data (game data).
    pub fx_data: Option<Vec<u8>>,
    /// FX flash save data.
    pub fx_save: Option<Vec<u8>>,
    /// All files in the archive: name → data.
    pub files: HashMap<String, Vec<u8>>,
}

/// Parse a .arduboy (ZIP) file from raw bytes.
pub fn parse_arduboy(data: &[u8]) -> Result<ArduboyFile, String> {
    let files = read_zip(data)?;
    let mut result = ArduboyFile::default();
    result.files = files.clone();

    // Find hex file
    for (name, content) in &files {
        let lower = name.to_lowercase();
        if lower.ends_with(".hex") {
            result.hex = Some(String::from_utf8_lossy(content).into_owned());
        }
    }

    // Parse info.json (simple key extraction, no full JSON parser)
    if let Some(info_data) = files.get("info.json").or_else(|| files.get("INFO.JSON")) {
        let info_str = String::from_utf8_lossy(info_data);
        result.title = extract_json_string(&info_str, "title")
            .or_else(|| extract_json_string(&info_str, "name"))
            .unwrap_or_default();
        result.author = extract_json_string(&info_str, "author")
            .or_else(|| extract_json_string(&info_str, "developer"))
            .unwrap_or_default();

        // Use info.json field names to find exact files
        if let Some(data_name) = extract_json_string(&info_str, "flashdata") {
            if let Some(content) = files.get(&data_name) {
                result.fx_data = Some(content.clone());
            }
        }
        if let Some(save_name) = extract_json_string(&info_str, "flashsave") {
            if let Some(content) = files.get(&save_name) {
                result.fx_save = Some(content.clone());
            }
        }
    }

    // Find FX data (fallback if not found via info.json): prefer *-fx.bin, then *-data.bin, then *.bin
    if result.fx_data.is_none() {
        for (name, content) in &files {
            let lower = name.to_lowercase();
            if lower.ends_with("-fx.bin") {
                result.fx_data = Some(content.clone());
                break;
            }
        }
    }
    if result.fx_data.is_none() {
        for (name, content) in &files {
            let lower = name.to_lowercase();
            if lower.ends_with("-data.bin") {
                result.fx_data = Some(content.clone());
                break;
            }
        }
    }
    if result.fx_data.is_none() {
        for (name, content) in &files {
            let lower = name.to_lowercase();
            if lower.ends_with(".bin") && !lower.contains("info") && !lower.contains("save") {
                result.fx_data = Some(content.clone());
                break;
            }
        }
    }

    // Find FX save (fallback if not found via info.json)
    if result.fx_save.is_none() {
        for (name, content) in &files {
            let lower = name.to_lowercase();
            if lower.ends_with("-save.bin") {
                result.fx_save = Some(content.clone());
                break;
            }
        }
    }

    if result.hex.is_none() {
        return Err("No .hex file found in .arduboy archive".into());
    }

    Ok(result)
}

/// Simple JSON string value extractor (no full parser).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    // Skip whitespace and colon
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ─── Minimal ZIP Reader ─────────────────────────────────────────────────────

fn read_zip(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut files = HashMap::new();

    // Find End of Central Directory record (scan backwards)
    let eocd_sig: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
    let eocd_pos = data
        .windows(4)
        .rposition(|w| w == eocd_sig)
        .ok_or("No End of Central Directory found — not a valid ZIP")?;

    if eocd_pos + 22 > data.len() {
        return Err("EOCD truncated".into());
    }
    let cd_count = u16_le(data, eocd_pos + 10) as usize;
    let cd_offset = u32_le(data, eocd_pos + 16) as usize;

    // Walk Central Directory entries to get reliable sizes + local header offsets
    let mut cd_pos = cd_offset;
    for _ in 0..cd_count {
        if cd_pos + 46 > data.len() {
            break;
        }
        let sig = u32_le(data, cd_pos);
        if sig != 0x02014b50 {
            break;
        }

        let method = u16_le(data, cd_pos + 10);
        let comp_size = u32_le(data, cd_pos + 20) as usize;
        let _uncomp_size = u32_le(data, cd_pos + 24) as usize;
        let name_len = u16_le(data, cd_pos + 28) as usize;
        let extra_len_cd = u16_le(data, cd_pos + 30) as usize;
        let comment_len = u16_le(data, cd_pos + 32) as usize;
        let local_offset = u32_le(data, cd_pos + 42) as usize;

        let cd_name_start = cd_pos + 46;
        if cd_name_start + name_len > data.len() {
            break;
        }
        let name =
            String::from_utf8_lossy(&data[cd_name_start..cd_name_start + name_len]).into_owned();

        cd_pos = cd_name_start + name_len + extra_len_cd + comment_len;

        // Skip directories
        if name.ends_with('/') || (comp_size == 0 && _uncomp_size == 0) {
            continue;
        }

        // Read data from local file header position
        if local_offset + 30 > data.len() {
            continue;
        }
        let local_sig = u32_le(data, local_offset);
        if local_sig != 0x04034b50 {
            continue;
        }
        let local_name_len = u16_le(data, local_offset + 26) as usize;
        let local_extra_len = u16_le(data, local_offset + 28) as usize;
        let data_start = local_offset + 30 + local_name_len + local_extra_len;
        if data_start + comp_size > data.len() {
            continue;
        }
        let compressed = &data[data_start..data_start + comp_size];

        let file_data = match method {
            0 => compressed.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(compressed)
                .map_err(|e| format!("Inflate error for {}: {:?}", name, e))?,
            _ => continue,
        };

        // Strip directory prefix for simpler lookup
        let simple_name = name.rsplit('/').next().unwrap_or(&name).to_string();
        files.insert(simple_name, file_data.clone());
        if name.contains('/') {
            files.insert(name, file_data);
        }
    }

    if files.is_empty() {
        return Err("No files found in ZIP archive".into());
    }
    Ok(files)
}

fn u16_le(data: &[u8], pos: usize) -> u16 {
    (data[pos] as u16) | ((data[pos + 1] as u16) << 8)
}
fn u32_le(data: &[u8], pos: usize) -> u32 {
    (data[pos] as u32)
        | ((data[pos + 1] as u32) << 8)
        | ((data[pos + 2] as u32) << 16)
        | ((data[pos + 3] as u32) << 24)
}
