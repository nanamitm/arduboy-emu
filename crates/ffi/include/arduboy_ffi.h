/*
 * arduboy_ffi.h — C ABI for the arduboy-core emulator.
 *
 * Matches the `extern "C"` surface in crates/ffi/src/lib.rs. Link against the
 * `arduboy_ffi` cdylib (arduboy_ffi.dll / libarduboy_ffi.so) or staticlib.
 *
 * Ownership & threading:
 *   - abemu_new() returns a handle you must release with abemu_free().
 *   - A handle is NOT thread-safe: use it from one thread only.
 *   - Pointers returned by abemu_framebuffer()/abemu_last_error() are owned by
 *     the handle and valid only until the next mutating call on it.
 */
#ifndef ARDUBOY_FFI_H
#define ARDUBOY_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque emulator handle. */
typedef struct Emu Emu;

/* Button identifiers for abemu_set_button(). */
enum AbButton {
    AB_BTN_UP    = 0,
    AB_BTN_DOWN  = 1,
    AB_BTN_LEFT  = 2,
    AB_BTN_RIGHT = 3,
    AB_BTN_A     = 4,
    AB_BTN_B     = 5
};

/* ── Lifecycle ─────────────────────────────────────────────────────────── */
Emu        *abemu_new(void);
void        abemu_free(Emu *h);
const char *abemu_last_error(Emu *h);

/* ── Loading ───────────────────────────────────────────────────────────── */
/* Auto-detects .arduboy / .elf / .hex; auto-loads companion FX .bin and .eep.
 * Returns 0 on success, non-zero on failure (see abemu_last_error). */
int abemu_load_file(Emu *h, const char *path);
int abemu_load_fx_file(Emu *h, const char *path);

/* ── Info ──────────────────────────────────────────────────────────────── */
int abemu_screen_width(void);   /* 128 */
int abemu_screen_height(void);  /* 64  */
int abemu_cpu_type(Emu *h);     /* 0 = ATmega32u4, 1 = ATmega328P */
int abemu_title(Emu *h, char *out, int max);

/* ── Control ───────────────────────────────────────────────────────────── */
void abemu_reset(Emu *h);
void abemu_run_frame(Emu *h);
void abemu_set_button(Emu *h, int btn, int pressed);

/* ── Display ───────────────────────────────────────────────────────────── */
/* RGBA8, width*height*4 bytes (32768). Valid until the next mutating call. */
const unsigned char *abemu_framebuffer(Emu *h);

/* ── Audio ─────────────────────────────────────────────────────────────── */
/* Fills `out` with interleaved L,R f32 samples. `max_pairs` is out capacity in
 * stereo pairs (out must hold max_pairs*2 floats). Returns pairs written. */
int abemu_render_audio(Emu *h, float *out, int max_pairs,
                       unsigned int sample_rate, float volume);

/* ── LEDs & serial ─────────────────────────────────────────────────────── */
void abemu_led_rgb(Emu *h, unsigned char *r, unsigned char *g, unsigned char *b);
int  abemu_led_tx(Emu *h);
int  abemu_led_rx(Emu *h);
int  abemu_take_serial(Emu *h, unsigned char *out, int max);

/* ── EEPROM ────────────────────────────────────────────────────────────── */
int abemu_eeprom_dirty(Emu *h);
int abemu_save_eeprom(Emu *h);

/* ── Save state (quick save / load) ────────────────────────────────────── */
int abemu_save_state(Emu *h);
int abemu_load_state(Emu *h);

/* ── Screenshot & GIF ──────────────────────────────────────────────────── */
int abemu_screenshot_png(Emu *h, const char *path);
int abemu_gif_start(Emu *h, const char *path);
int abemu_gif_recording(Emu *h);
int abemu_gif_stop(Emu *h);

/* ── Debug views ───────────────────────────────────────────────────────── */
int abemu_dump_regs(Emu *h, char *out, int max);
int abemu_dump_ram(Emu *h, unsigned int start, unsigned int length,
                   char *out, int max);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* ARDUBOY_FFI_H */
