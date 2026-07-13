//! Single-ROM diagnostic for investigating smoke-test outliers.
//!
//! Usage: cargo run --release --example rom_diag -- <file.hex> [frames]

use arduboy_core::{Arduboy, Button};
use std::fs;

fn snapshot(ard: &Arduboy, blank: &[u32], label: &str) {
    let fb = ard.framebuffer_u32();
    let changed = fb != blank;
    let on = fb.iter().filter(|&&p| p != blank[0]).count();
    println!(
        "  [{label}] pc=0x{:04X} unknown_ops={} spdr_writes={} fb_changed={} lit_pixels={}",
        ard.cpu.pc, ard.unknown_ops, ard.dbg_spdr_writes, changed, on
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("usage: rom_diag <file.hex> [frames]");
    let frames: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(300);

    let hex = fs::read_to_string(&file).expect("read hex");
    let blank = Arduboy::new().framebuffer_u32();

    // --catch-reset: single-step and report each time the PC re-enters the
    // interrupt-vector region (a reset/restart), with the instruction that got
    // us there. Reveals restart loops.
    if std::env::args().any(|a| a == "--catch-reset") {
        let mut ard = Arduboy::new();
        ard.load_hex(&hex).expect("load hex");
        println!("Catching resets in {file}");
        let mut hits = 0;
        let mut ring: std::collections::VecDeque<(u16, String)> = std::collections::VecDeque::new();
        for _ in 0..2_000_000u64 {
            let prev_pc = ard.cpu.pc;
            let dis = ard.step_one();
            ring.push_back((prev_pc, dis.clone()));
            if ring.len() > 30 {
                ring.pop_front();
            }
            let pc = ard.cpu.pc;
            if pc == 0x0000 && prev_pc > 0x0100 {
                println!("  reset via JMP 0 — last 30 instructions:");
                for (p, d) in &ring {
                    println!("    0x{p:04X}: {d}");
                }
                hits += 1;
                if hits >= 1 {
                    break;
                }
            }
        }
        if hits == 0 {
            println!("  no resets seen in 2,000,000 steps");
        }
        return;
    }

    let profile = std::env::args().any(|a| a == "--profile");

    let mut ard = Arduboy::new();
    let loaded = ard.load_hex(&hex).expect("load hex");
    println!("Loaded {file} ({loaded} bytes), running {frames} frames");

    // Optional static disassembly: --dis <byte_addr> <count>
    let argv: Vec<String> = std::env::args().collect();
    if let Some(i) = argv.iter().position(|a| a == "--dis") {
        let start = argv
            .get(i + 1)
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);
        let count: usize = argv.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(24);
        let mut byte = start as usize;
        for _ in 0..count {
            let word = ard.mem.flash[byte] as u16 | ((ard.mem.flash[byte + 1] as u16) << 8);
            let next = ard.mem.flash[byte + 2] as u16 | ((ard.mem.flash[byte + 3] as u16) << 8);
            let (inst, sz) = arduboy_core::opcodes::decode(word, next);
            let asm = arduboy_core::disasm::disassemble(inst, (byte / 2) as u16);
            println!("  0x{byte:04X}: {asm}");
            byte += sz as usize * 2;
        }
        return;
    }

    if profile {
        ard.profiler.start(ard.cpu.tick);
    }
    for _ in 0..frames {
        ard.run_frame();
    }
    if profile {
        ard.profiler.stop(ard.cpu.tick);
        println!("{}", ard.profiler_report());
    }
    snapshot(&ard, &blank, "no input");

    // Tap through the common "press to start" buttons and keep running.
    for btn in [Button::A, Button::B, Button::Up, Button::Down] {
        ard.set_button(btn, true);
        for _ in 0..30 {
            ard.run_frame();
        }
        ard.set_button(btn, false);
        for _ in 0..30 {
            ard.run_frame();
        }
    }
    snapshot(&ard, &blank, "after input");

    // Dump the working registers, then trace the loop the CPU is sitting in.
    print!("  regs:");
    for r in [1u8, 18, 19, 26, 27, 30, 31] {
        print!(" R{r}=0x{:02X}", ard.mem.reg(r));
    }
    println!(
        " X=0x{:04X}",
        (ard.mem.reg(27) as u16) << 8 | ard.mem.reg(26) as u16
    );
    println!("  instruction trace from pc=0x{:04X}:", ard.cpu.pc);
    for _ in 0..16 {
        let pc = ard.cpu.pc;
        let dis = ard.step_one();
        println!("    0x{pc:04X}: {dis}");
    }
}
