//! ROM smoke / regression harness.
//!
//! Loads every `.hex` under a directory, runs each for a number of frames, and
//! reports ROMs that fail to load, panic, execute unknown opcodes, or never draw
//! anything. This is a conformance signal for the emulator core, not a committed
//! test — the ROMs live outside the repository.
//!
//! Usage:
//!   cargo run --release --example rom_smoke -- <dir> [frames]
//!
//! `dir` is scanned recursively for `.hex` files. `frames` defaults to 120 (~2s).

use arduboy_core::{detect_cpu_type, Arduboy, CpuType};
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

fn collect_hex(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_hex(&path, out);
        } else if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("hex"))
        {
            out.push(path);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: rom_smoke <dir> [frames]");
        std::process::exit(2);
    };
    let frames: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(120);

    let base = Path::new(&dir);
    let mut roms = Vec::new();
    collect_hex(base, &mut roms);
    roms.sort();
    println!("Found {} hex ROMs under {dir}", roms.len());
    println!("Running {frames} frames each...\n");

    // A freshly reset device's framebuffer — anything drawn differs from this.
    // Kept per CPU type since 328P defaults to the PCD8544 LCD (Gamebuino).
    let blank_32u4 = Arduboy::new_with_cpu(CpuType::Atmega32u4).framebuffer_u32();
    let blank_328p = Arduboy::new_with_cpu(CpuType::Atmega328p).framebuffer_u32();
    let mut cpu_counts = [0usize; 2]; // [32u4, 328p]

    let mut pass = 0usize;
    let mut load_fail: Vec<(String, String)> = Vec::new();
    let mut panicked: Vec<String> = Vec::new();
    let mut unknown: Vec<(String, u64)> = Vec::new();
    let mut blank: Vec<String> = Vec::new();

    for rom in &roms {
        let name = rom
            .strip_prefix(base)
            .unwrap_or(rom)
            .to_string_lossy()
            .replace('\\', "/");
        let hex = match fs::read_to_string(rom) {
            Ok(s) => s,
            Err(e) => {
                load_fail.push((name, format!("read error: {e}")));
                continue;
            }
        };

        let result = catch_unwind(AssertUnwindSafe(|| {
            // Auto-detect the target from the vector table (Arduboy=32u4,
            // Gamebuino Classic=328P) so both compilations run correctly.
            let mut probe = Arduboy::new();
            probe.load_hex(&hex)?;
            let cpu = detect_cpu_type(&probe.mem.flash);
            let blank = if cpu == CpuType::Atmega328p {
                &blank_328p
            } else {
                &blank_32u4
            };
            let mut ard = Arduboy::new_with_cpu(cpu);
            ard.load_hex(&hex)?;
            for _ in 0..frames {
                ard.run_frame();
            }
            let fb = ard.framebuffer_u32();
            // "Rendered" means the display differs from a freshly reset device,
            // so a uniform full-screen fill still counts as drawn.
            let rendered = fb != *blank;
            Ok::<(u64, bool, CpuType), String>((ard.unknown_ops, rendered, cpu))
        }));

        match result {
            Err(_) => panicked.push(name),
            Ok(Err(e)) => load_fail.push((name, e)),
            Ok(Ok((unk, rendered, cpu))) => {
                cpu_counts[(cpu == CpuType::Atmega328p) as usize] += 1;
                let mut ok = true;
                if unk > 0 {
                    unknown.push((name.clone(), unk));
                    ok = false;
                }
                if !rendered {
                    blank.push(name);
                    ok = false;
                }
                if ok {
                    pass += 1;
                }
            }
        }
    }

    let report = |title: &str, items: &[String]| {
        if !items.is_empty() {
            println!("\n{} ({}):", title, items.len());
            for it in items {
                println!("  - {it}");
            }
        }
    };

    report("PANICKED", &panicked);
    if !load_fail.is_empty() {
        println!("\nLOAD FAILED ({}):", load_fail.len());
        for (n, e) in &load_fail {
            println!("  - {n}: {e}");
        }
    }
    if !unknown.is_empty() {
        println!("\nUNKNOWN OPCODES ({}):", unknown.len());
        for (n, c) in &unknown {
            println!("  - {n}: {c} unknown ops");
        }
    }
    report("BLANK / NO RENDER", &blank);

    println!(
        "\n=== Summary: {}/{} clean (panic={}, load_fail={}, unknown={}, blank={}) ===",
        pass,
        roms.len(),
        panicked.len(),
        load_fail.len(),
        unknown.len(),
        blank.len(),
    );
    println!(
        "Detected CPU: {} × ATmega32u4, {} × ATmega328P",
        cpu_counts[0], cpu_counts[1]
    );
}
