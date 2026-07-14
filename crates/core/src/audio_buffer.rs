//! Sample-accurate audio waveform buffer with post-processing.
//!
//! Records pin-level transitions (edges) with CPU tick timestamps during each
//! frame, then converts them to PCM audio samples at the target sample rate.
//!
//! ## Post-processing pipeline
//!
//! When enabled ([`AudioBuffer::filters_enabled`]), five stages improve quality:
//!
//! 1. **Edge interpolation** — Computes the time-weighted average level within
//!    each sample period instead of snapping to the nearest edge. Eliminates
//!    aliasing artifacts from sub-sample transitions.
//!
//! 2. **Low-pass filter** — 2nd-order Butterworth at 8 kHz simulates the
//!    bandwidth limitation of the Arduboy's piezo speaker, rounding off harsh
//!    upper harmonics from the raw square wave.
//!
//! 3. **DC-blocking high-pass** — 2nd-order Butterworth at 20 Hz removes any
//!    DC offset that accumulates through the LPF.
//!
//! 4. **Click suppression** — Smoothly fades audio in (~2 ms) and out (~5 ms)
//!    when sound starts or stops, preventing audible pops.
//!
//! 5. **Stereo crossfeed** — Blends 20% of each channel into the opposite
//!    channel for natural headphone listening (the Arduboy has a single
//!    mono piezo speaker driven in bridge mode across PC6/PC7).

use std::f32::consts::{PI, SQRT_2};

// ─── Constants ──────────────────────────────────────────────────────────────

/// Low-pass filter cutoff (Hz). Models piezo speaker bandwidth rolloff.
const LPF_CUTOFF: f32 = 8000.0;
/// DC-blocking high-pass cutoff (Hz). Removes sub-audible drift.
const HPF_CUTOFF: f32 = 20.0;
/// Default crossfeed: 20% of opposite channel mixed in.
const DEFAULT_CROSSFEED: f32 = 0.20;
/// Envelope attack time (seconds). Fade-in when audio starts.
const ENV_ATTACK_S: f32 = 0.002;
/// Envelope release time (seconds). Fade-out when audio stops.
const ENV_RELEASE_S: f32 = 0.005;

// ─── Edge recording ─────────────────────────────────────────────────────────

/// A single pin-level transition event.
#[derive(Debug, Clone, Copy)]
pub struct AudioEdge {
    /// CPU tick when the transition occurred.
    pub tick: u64,
    /// Pin level after transition (true = high).
    pub level: bool,
}

/// Per-channel edge buffer with current pin state.
#[derive(Debug)]
pub struct ChannelBuffer {
    /// Recorded edges this frame.
    edges: Vec<AudioEdge>,
    /// Current pin level (carried across frames).
    pub level: bool,
}

impl ChannelBuffer {
    /// Create an empty channel buffer.
    pub fn new() -> Self {
        ChannelBuffer {
            edges: Vec::with_capacity(4096),
            level: false,
        }
    }

    /// Record a pin transition.
    #[inline]
    pub fn push(&mut self, tick: u64, level: bool) {
        if level != self.level {
            self.edges.push(AudioEdge { tick, level });
            self.level = level;
        }
    }

    /// Clear edges for next frame (pin level is preserved).
    pub fn clear(&mut self) {
        self.edges.clear();
    }

    /// Number of edges recorded this frame.
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// Access the raw edge slice.
    pub fn edges(&self) -> &[AudioEdge] {
        &self.edges
    }
}

// ─── 2nd-order biquad IIR filter ────────────────────────────────────────────

/// Biquad IIR filter using Direct Form 2 Transposed.
///
/// Numerically stable for both low and high cutoff frequencies.
/// Used for Butterworth low-pass (speaker sim) and high-pass (DC blocker).
#[derive(Debug, Clone)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// 2nd-order Butterworth low-pass filter.
    ///
    /// Q = 1/√2 gives maximally-flat passband (no resonant peak).
    fn lowpass(cutoff: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * PI * cutoff / sample_rate;
        let (sin_w, cos_w) = (w0.sin(), w0.cos());
        let alpha = sin_w / (2.0 * SQRT_2);
        let a0_inv = 1.0 / (1.0 + alpha);
        Biquad {
            b0: ((1.0 - cos_w) * 0.5) * a0_inv,
            b1: (1.0 - cos_w) * a0_inv,
            b2: ((1.0 - cos_w) * 0.5) * a0_inv,
            a1: (-2.0 * cos_w) * a0_inv,
            a2: (1.0 - alpha) * a0_inv,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// 2nd-order Butterworth high-pass filter.
    fn highpass(cutoff: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * PI * cutoff / sample_rate;
        let (sin_w, cos_w) = (w0.sin(), w0.cos());
        let alpha = sin_w / (2.0 * SQRT_2);
        let a0_inv = 1.0 / (1.0 + alpha);
        Biquad {
            b0: ((1.0 + cos_w) * 0.5) * a0_inv,
            b1: (-(1.0 + cos_w)) * a0_inv,
            b2: ((1.0 + cos_w) * 0.5) * a0_inv,
            a1: (-2.0 * cos_w) * a0_inv,
            a2: (1.0 - alpha) * a0_inv,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Process one sample. State is updated in-place.
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

// ─── Audio buffer with post-processing ──────────────────────────────────────

/// Stereo audio buffer with optional post-processing pipeline.
///
/// Left channel = Speaker 1 (PC6 on 32u4, PD3 on 328P).
/// Right channel = Speaker 2 (PB5).
///
/// Supports two audio modes:
/// - **Edge-based** (GPIO toggle / SBI PIND): records pin-level transitions
/// - **PWM DAC** (Timer2 OCR2B): records 8-bit analog sample values
pub struct AudioBuffer {
    /// Left channel (Speaker 1: PC6 on 32u4, PD3 on 328P).
    pub left: ChannelBuffer,
    /// Right channel (Speaker 2: PB5).
    pub right: ChannelBuffer,
    /// Frame start tick (set at beginning of run_frame).
    pub frame_start: u64,
    /// Frame end tick (set at end of run_frame).
    pub frame_end: u64,

    /// PWM DAC sample buffer: (tick, level) pairs where level is -1.0..+1.0.
    /// Used when Timer2 PWM drives OC2B for analog audio output.
    pub pwm_samples: Vec<(u64, f32)>,
    /// Carried-over PWM level from previous frame (sample-and-hold).
    pwm_level: f32,

    // ── Post-processing state (persists across frames) ──
    lpf_l: Biquad,
    lpf_r: Biquad,
    hpf_l: Biquad,
    hpf_r: Biquad,
    envelope_l: f32,
    envelope_r: f32,
    configured_rate: u32,

    /// Enable/disable audio post-processing pipeline.
    pub filters_enabled: bool,
    /// Stereo crossfeed amount (0.0 = full stereo, 0.5 = mono).
    pub crossfeed: f32,
}

impl AudioBuffer {
    /// Create an empty stereo audio buffer (filters default to 44.1 kHz until
    /// the first render reconfigures them).
    pub fn new() -> Self {
        // Initialize filters with default 44100 Hz; reconfigured on first render
        let sr = 44100.0;
        AudioBuffer {
            left: ChannelBuffer::new(),
            right: ChannelBuffer::new(),
            frame_start: 0,
            frame_end: 0,
            pwm_samples: Vec::with_capacity(4096),
            pwm_level: 0.0,
            lpf_l: Biquad::lowpass(LPF_CUTOFF, sr),
            lpf_r: Biquad::lowpass(LPF_CUTOFF, sr),
            hpf_l: Biquad::highpass(HPF_CUTOFF, sr),
            hpf_r: Biquad::highpass(HPF_CUTOFF, sr),
            envelope_l: 0.0,
            envelope_r: 0.0,
            configured_rate: 0,
            filters_enabled: true,
            crossfeed: DEFAULT_CROSSFEED,
        }
    }

    /// Recalculate filter coefficients for a new sample rate.
    fn configure_filters(&mut self, sample_rate: u32) {
        let sr = sample_rate as f32;
        self.lpf_l = Biquad::lowpass(LPF_CUTOFF, sr);
        self.lpf_r = Biquad::lowpass(LPF_CUTOFF, sr);
        self.hpf_l = Biquad::highpass(HPF_CUTOFF, sr);
        self.hpf_r = Biquad::highpass(HPF_CUTOFF, sr);
        self.configured_rate = sample_rate;
    }

    /// Begin a new frame: store start tick, clear edge buffers.
    pub fn begin_frame(&mut self, tick: u64) {
        self.frame_start = tick;
        self.left.clear();
        self.right.clear();
        self.pwm_samples.clear();
    }

    /// End the current frame: store end tick.
    pub fn end_frame(&mut self, tick: u64) {
        self.frame_end = tick;
    }

    /// Returns true if any audio activity was recorded this frame.
    pub fn has_audio(&self) -> bool {
        self.left.len() > 0 || self.right.len() > 0 || !self.pwm_samples.is_empty()
    }

    /// Returns true if render_samples should still be called.
    ///
    /// True when edges/PWM samples are present (active audio) **or** the envelope
    /// is still fading out from previous activity.
    pub fn needs_render(&self) -> bool {
        self.has_audio() || self.envelope_l > 0.001 || self.envelope_r > 0.001
    }

    /// Push a PWM DAC sample (called when OCR2B changes in PWM mode).
    ///
    /// `value` is the raw 8-bit OCR2B value (0–255). Converted to signed
    /// audio centered around 128 (silence).
    pub fn push_pwm_sample(&mut self, tick: u64, value: u8) {
        // Convert unsigned 8-bit to signed float: 0→-1.0, 128→0.0, 255→+1.0
        let level = (value as f32 - 128.0) / 128.0;
        self.pwm_samples.push((tick, level));
    }

    /// Toggle the post-processing filter pipeline on/off.
    pub fn toggle_filters(&mut self) {
        self.filters_enabled = !self.filters_enabled;
    }

    /// Render edge buffers to interleaved stereo f32 PCM samples.
    ///
    /// `out` receives interleaved \[L, R, L, R, ...\] samples at `sample_rate` Hz.
    /// `volume` scales the base amplitude (0.0–1.0).
    /// `clock_hz` is the CPU clock frequency (16 MHz for Arduboy).
    ///
    /// When [`filters_enabled`](Self::filters_enabled) is true, the output goes
    /// through the full post-processing pipeline. When false, raw
    /// edge-interpolated samples are emitted (still anti-aliased).
    ///
    /// Returns the number of stereo sample pairs written.
    pub fn render_samples(
        &mut self,
        out: &mut Vec<f32>,
        sample_rate: u32,
        clock_hz: u32,
        volume: f32,
    ) -> usize {
        // Reconfigure filters if sample rate changed
        if self.configured_rate != sample_rate {
            self.configure_filters(sample_rate);
        }

        let frame_ticks = self.frame_end.saturating_sub(self.frame_start);
        if frame_ticks == 0 {
            return 0;
        }

        let num_samples =
            ((frame_ticks as f64 * sample_rate as f64) / clock_hz as f64).ceil() as usize;
        out.clear();
        out.reserve(num_samples * 2);

        let tps = clock_hz as f64 / sample_rate as f64; // ticks per sample
        let start = self.frame_start as f64;

        let use_pwm = !self.pwm_samples.is_empty();

        let l_edges = self.left.edges();
        let r_edges = self.right.edges();
        let mut li = 0usize;
        let mut ri = 0usize;
        let mut pwm_i = 0usize;

        // Initial levels: carried-over state from before the first edge
        let mut l_level = if l_edges.is_empty() {
            self.left.level
        } else {
            !l_edges[0].level
        };
        let mut r_level = if r_edges.is_empty() {
            self.right.level
        } else {
            !r_edges[0].level
        };

        let l_active = !l_edges.is_empty() || use_pwm;
        let r_active = !r_edges.is_empty();

        // Envelope ramp rates (per sample)
        let attack_rate = 1.0 / (ENV_ATTACK_S * sample_rate as f32);
        let release_rate = 1.0 / (ENV_RELEASE_S * sample_rate as f32);

        let apply_post = self.filters_enabled;

        for i in 0..num_samples {
            let p_start = start + i as f64 * tps;
            let p_end = p_start + tps;

            // ── Left channel: PWM DAC or edge-based ──
            let l_raw = if use_pwm {
                Self::sample_pwm(
                    &mut pwm_i,
                    &self.pwm_samples,
                    &mut self.pwm_level,
                    p_start,
                    p_end,
                    tps,
                    volume,
                )
            } else {
                Self::sample_channel(&mut li, l_edges, &mut l_level, p_start, p_end, tps, volume)
            };

            // ── Right channel: always edge-based ──
            let r_raw =
                Self::sample_channel(&mut ri, r_edges, &mut r_level, p_start, p_end, tps, volume);

            if apply_post {
                // (1) Click suppression: per-channel envelope
                if l_active {
                    self.envelope_l = (self.envelope_l + attack_rate).min(1.0);
                } else {
                    self.envelope_l = (self.envelope_l - release_rate).max(0.0);
                }
                if r_active {
                    self.envelope_r = (self.envelope_r + attack_rate).min(1.0);
                } else {
                    self.envelope_r = (self.envelope_r - release_rate).max(0.0);
                }

                let l_env = l_raw * self.envelope_l;
                let r_env = r_raw * self.envelope_r;

                // (2) Low-pass filter (speaker bandwidth simulation)
                let l_lp = self.lpf_l.process(l_env);
                let r_lp = self.lpf_r.process(r_env);

                // (3) DC-blocking high-pass
                let l_hp = self.hpf_l.process(l_lp);
                let r_hp = self.hpf_r.process(r_lp);

                // (4) Stereo crossfeed
                let cf = self.crossfeed;
                out.push(l_hp * (1.0 - cf) + r_hp * cf);
                out.push(r_hp * (1.0 - cf) + l_hp * cf);
            } else {
                out.push(l_raw);
                out.push(r_raw);
            }
        }

        num_samples
    }

    /// Compute one edge-interpolated sample for a single channel.
    ///
    /// Integrates the square wave over the sample period `[p_start, p_end)` to
    /// produce the time-weighted average level. This is equivalent to a
    /// box-filter anti-alias: for a sample period containing a transition at
    /// 30% through, outputs 30% old\_level + 70% new\_level.
    #[inline]
    fn sample_channel(
        edge_idx: &mut usize,
        edges: &[AudioEdge],
        level: &mut bool,
        p_start: f64,
        p_end: f64,
        tps: f64,
        volume: f32,
    ) -> f32 {
        let mut time_high = 0.0f64;
        let mut cursor = p_start;

        // Walk all edges that fall within this sample period
        while *edge_idx < edges.len() && (edges[*edge_idx].tick as f64) < p_end {
            let edge_tick = edges[*edge_idx].tick as f64;
            if edge_tick > cursor {
                // Accumulate time at current level before this edge
                if *level {
                    time_high += edge_tick - cursor;
                }
                cursor = edge_tick;
            }
            *level = edges[*edge_idx].level;
            *edge_idx += 1;
        }

        // Remaining time after last edge (or entire period if no edges)
        if *level {
            time_high += p_end - cursor;
        }

        // Map duty cycle [0.0, 1.0] → amplitude [-volume, +volume]
        let duty = (time_high / tps) as f32;
        volume * (2.0 * duty - 1.0)
    }

    /// Compute one sample from PWM DAC data using sample-and-hold interpolation.
    ///
    /// Takes the time-weighted average of PWM levels within the sample period.
    /// Each PWM sample holds its value until the next sample arrives.
    #[inline]
    fn sample_pwm(
        pwm_idx: &mut usize,
        samples: &[(u64, f32)],
        level: &mut f32,
        p_start: f64,
        p_end: f64,
        tps: f64,
        volume: f32,
    ) -> f32 {
        let mut accum = 0.0f64;
        let mut cursor = p_start;

        // Walk all PWM samples that fall within this output sample period
        while *pwm_idx < samples.len() && (samples[*pwm_idx].0 as f64) < p_end {
            let sample_tick = samples[*pwm_idx].0 as f64;
            if sample_tick > cursor {
                // Accumulate time at current level before this sample
                accum += *level as f64 * (sample_tick - cursor);
                cursor = sample_tick;
            }
            *level = samples[*pwm_idx].1;
            *pwm_idx += 1;
        }

        // Remaining time after last sample (or entire period if no samples)
        accum += *level as f64 * (p_end - cursor);

        // Time-weighted average, scaled by volume
        (accum / tps) as f32 * volume
    }
}
