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

    let mut ard = Arduboy::new();
    let loaded = ard.load_hex(&hex).expect("load hex");
    println!("Loaded {file} ({loaded} bytes), running {frames} frames");

    for _ in 0..frames {
        ard.run_frame();
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
