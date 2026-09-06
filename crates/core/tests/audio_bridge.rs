use arduboy_core::{Arduboy, CpuType};

fn render(pattern: impl Fn(u64) -> u8, filters: bool) -> Vec<f32> {
    let mut ard = Arduboy::new();
    ard.audio_buf.filters_enabled = filters;
    ard.audio_buf.begin_frame(0);
    for i in 0..100 {
        ard.cpu.tick = i * 8000;
        ard.write_data(0x28, pattern(i));
    }
    ard.audio_buf.end_frame(800000);
    let mut out = Vec::new();
    ard.audio_buf.render_samples(&mut out, 48000, 16000000, 1.0);
    out
}

#[test]
fn bridge_audio_survives_mono_downmix() {
    for filters in [false, true] {
        let out = render(|i| if i % 2 == 0 { 0x40 } else { 0x80 }, filters);
        assert!(out.chunks_exact(2).all(|s| s[0] == s[1]));
        let peak = out
            .chunks_exact(2)
            .map(|s| ((s[0] + s[1]) * 0.5).abs())
            .fold(0.0f32, f32::max);
        assert!(peak > 0.5, "bridge output was cancelled: {peak}");
    }
}

#[test]
fn equal_pin_voltages_are_silent() {
    for filters in [false, true] {
        assert!(render(|i| if i % 2 == 0 { 0xc0 } else { 0 }, filters)
            .iter()
            .all(|s| s.abs() < 0.00001));
    }
}

#[test]
fn single_pin_still_produces_audio() {
    let out = render(|i| if i % 2 == 0 { 0x40 } else { 0 }, true);
    assert!(out.iter().any(|s| s.abs() > 0.2));
}

#[test]
fn gamebuino_keeps_independent_audio() {
    let ard = Arduboy::new_with_cpu(CpuType::Atmega328p);
    assert!(!ard.audio_buf.bridge_mode);
}
