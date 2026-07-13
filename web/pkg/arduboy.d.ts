/* tslint:disable */
/* eslint-disable */

/**
 * Browser-facing emulator handle.
 */
export class AbEmu {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * CPU type of the loaded ROM: 0 = ATmega32u4, 1 = ATmega328P.
     */
    cpuType(): number;
    /**
     * Whether EEPROM has unsaved changes.
     */
    eepromDirty(): boolean;
    /**
     * Current display framebuffer as RGBA8 bytes (128×64×4). Returned as a copy
     * (a `Uint8Array` in JS) suitable for `ImageData`.
     */
    frame(): Uint8Array;
    /**
     * Whether a GIF recording is in progress.
     */
    gifRecording(): boolean;
    /**
     * Begin capturing frames into an animated GIF. Frames are added on each
     * [`AbEmu::run_frame`] until [`AbEmu::gif_stop`].
     */
    gifStart(): void;
    /**
     * Finish the recording and return the encoded GIF bytes (empty if none).
     */
    gifStop(): Uint8Array;
    /**
     * RGB LED state as `[r, g, b]` (0–255).
     */
    ledRgb(): Uint8Array;
    /**
     * RX LED state (active).
     */
    ledRx(): boolean;
    /**
     * TX LED state (active).
     */
    ledTx(): boolean;
    /**
     * Restore EEPROM contents previously saved with [`AbEmu::save_eeprom`].
     */
    loadEeprom(data: Uint8Array): void;
    /**
     * Load a ROM by file name + bytes. `.arduboy` archives are unpacked (hex +
     * FX); anything else is treated as Intel HEX text. The CPU type is
     * auto-detected from the flash image. Throws on parse failure.
     */
    loadFile(name: string, data: Uint8Array): void;
    /**
     * Load an explicit FX flash image (overrides any archive FX).
     */
    loadFx(data: Uint8Array): void;
    /**
     * Restore a state blob produced by [`AbEmu::save_state`]. Throws on a bad
     * blob or a CPU-type mismatch with the loaded ROM.
     */
    loadState(data: Uint8Array): void;
    /**
     * Create a fresh emulator (ATmega32u4, no ROM). Installs a panic hook so
     * Rust panics surface in the browser console instead of an opaque trap.
     */
    constructor();
    /**
     * Render this frame's audio as interleaved L,R `f32` samples (a
     * `Float32Array` in JS). Call once per frame, after [`AbEmu::run_frame`].
     */
    renderAudio(sample_rate: number, volume: number): Float32Array;
    /**
     * Reset the CPU and peripherals (flash/FX preserved).
     */
    reset(): void;
    /**
     * Run one video frame (~216000 cycles at 16 MHz).
     */
    runFrame(): void;
    /**
     * Snapshot EEPROM contents (for browser persistence, e.g. IndexedDB).
     */
    saveEeprom(): Uint8Array;
    /**
     * Serialize the full emulator state to a compressed byte blob (for a quick
     * slot in IndexedDB or a downloadable `.state` file).
     */
    saveState(): Uint8Array;
    /**
     * Display height in pixels (64).
     */
    static screenHeight(): number;
    /**
     * Display width in pixels (128).
     */
    static screenWidth(): number;
    /**
     * Set a button state. `btn`: 0=Up 1=Down 2=Left 3=Right 4=A 5=B.
     */
    setButton(btn: number, pressed: boolean): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_abemu_free: (a: number, b: number) => void;
    readonly abemu_cpuType: (a: number) => number;
    readonly abemu_eepromDirty: (a: number) => number;
    readonly abemu_frame: (a: number) => [number, number];
    readonly abemu_gifRecording: (a: number) => number;
    readonly abemu_gifStart: (a: number) => void;
    readonly abemu_gifStop: (a: number) => [number, number];
    readonly abemu_ledRgb: (a: number) => [number, number];
    readonly abemu_ledRx: (a: number) => number;
    readonly abemu_ledTx: (a: number) => number;
    readonly abemu_loadEeprom: (a: number, b: number, c: number) => void;
    readonly abemu_loadFile: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly abemu_loadFx: (a: number, b: number, c: number) => void;
    readonly abemu_loadState: (a: number, b: number, c: number) => [number, number];
    readonly abemu_new: () => number;
    readonly abemu_renderAudio: (a: number, b: number, c: number) => [number, number];
    readonly abemu_reset: (a: number) => void;
    readonly abemu_runFrame: (a: number) => void;
    readonly abemu_saveEeprom: (a: number) => [number, number];
    readonly abemu_saveState: (a: number) => [number, number, number, number];
    readonly abemu_screenHeight: () => number;
    readonly abemu_screenWidth: () => number;
    readonly abemu_setButton: (a: number, b: number, c: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
