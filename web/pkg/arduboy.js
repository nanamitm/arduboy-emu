/* @ts-self-types="./arduboy.d.ts" */

/**
 * Browser-facing emulator handle.
 */
export class AbEmu {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AbEmuFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_abemu_free(ptr, 0);
    }
    /**
     * CPU type of the loaded ROM: 0 = ATmega32u4, 1 = ATmega328P.
     * @returns {number}
     */
    cpuType() {
        const ret = wasm.abemu_cpuType(this.__wbg_ptr);
        return ret;
    }
    /**
     * Whether EEPROM has unsaved changes.
     * @returns {boolean}
     */
    eepromDirty() {
        const ret = wasm.abemu_eepromDirty(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Current display framebuffer as RGBA8 bytes (128×64×4). Returned as a copy
     * (a `Uint8Array` in JS) suitable for `ImageData`.
     * @returns {Uint8Array}
     */
    frame() {
        const ret = wasm.abemu_frame(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Whether a GIF recording is in progress.
     * @returns {boolean}
     */
    gifRecording() {
        const ret = wasm.abemu_gifRecording(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Begin capturing frames into an animated GIF. Frames are added on each
     * [`AbEmu::run_frame`] until [`AbEmu::gif_stop`].
     */
    gifStart() {
        wasm.abemu_gifStart(this.__wbg_ptr);
    }
    /**
     * Finish the recording and return the encoded GIF bytes (empty if none).
     * @returns {Uint8Array}
     */
    gifStop() {
        const ret = wasm.abemu_gifStop(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * RGB LED state as `[r, g, b]` (0–255).
     * @returns {Uint8Array}
     */
    ledRgb() {
        const ret = wasm.abemu_ledRgb(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * RX LED state (active).
     * @returns {boolean}
     */
    ledRx() {
        const ret = wasm.abemu_ledRx(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * TX LED state (active).
     * @returns {boolean}
     */
    ledTx() {
        const ret = wasm.abemu_ledTx(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Restore EEPROM contents previously saved with [`AbEmu::save_eeprom`].
     * @param {Uint8Array} data
     */
    loadEeprom(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.abemu_loadEeprom(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Load a ROM by file name + bytes. `.arduboy` archives are unpacked (hex +
     * FX); anything else is treated as Intel HEX text. The CPU type is
     * auto-detected from the flash image. Throws on parse failure.
     * @param {string} name
     * @param {Uint8Array} data
     */
    loadFile(name, data) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.abemu_loadFile(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Load an explicit FX flash image (overrides any archive FX).
     * @param {Uint8Array} data
     */
    loadFx(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.abemu_loadFx(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Restore a state blob produced by [`AbEmu::save_state`]. Throws on a bad
     * blob or a CPU-type mismatch with the loaded ROM.
     * @param {Uint8Array} data
     */
    loadState(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.abemu_loadState(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Create a fresh emulator (ATmega32u4, no ROM). Installs a panic hook so
     * Rust panics surface in the browser console instead of an opaque trap.
     */
    constructor() {
        const ret = wasm.abemu_new();
        this.__wbg_ptr = ret >>> 0;
        AbEmuFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Render this frame's audio as interleaved L,R `f32` samples (a
     * `Float32Array` in JS). Call once per frame, after [`AbEmu::run_frame`].
     * @param {number} sample_rate
     * @param {number} volume
     * @returns {Float32Array}
     */
    renderAudio(sample_rate, volume) {
        const ret = wasm.abemu_renderAudio(this.__wbg_ptr, sample_rate, volume);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Reset the CPU and peripherals (flash/FX preserved).
     */
    reset() {
        wasm.abemu_reset(this.__wbg_ptr);
    }
    /**
     * Run one video frame (~216000 cycles at 16 MHz).
     */
    runFrame() {
        wasm.abemu_runFrame(this.__wbg_ptr);
    }
    /**
     * Snapshot EEPROM contents (for browser persistence, e.g. IndexedDB).
     * @returns {Uint8Array}
     */
    saveEeprom() {
        const ret = wasm.abemu_saveEeprom(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Serialize the full emulator state to a compressed byte blob (for a quick
     * slot in IndexedDB or a downloadable `.state` file).
     * @returns {Uint8Array}
     */
    saveState() {
        const ret = wasm.abemu_saveState(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Display height in pixels (64).
     * @returns {number}
     */
    static screenHeight() {
        const ret = wasm.abemu_screenHeight();
        return ret >>> 0;
    }
    /**
     * Display width in pixels (128).
     * @returns {number}
     */
    static screenWidth() {
        const ret = wasm.abemu_screenWidth();
        return ret >>> 0;
    }
    /**
     * Set a button state. `btn`: 0=Up 1=Down 2=Left 3=Right 4=A 5=B.
     * @param {number} btn
     * @param {boolean} pressed
     */
    setButton(btn, pressed) {
        wasm.abemu_setButton(this.__wbg_ptr, btn, pressed);
    }
}
if (Symbol.dispose) AbEmu.prototype[Symbol.dispose] = AbEmu.prototype.free;

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_be289d5034ed271b: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_7534b8e9a36f1ab4: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_new_8a6f238a6ece86ea: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_stack_0ed75d68575b0f3c: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./arduboy_bg.js": import0,
    };
}

const AbEmuFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_abemu_free(ptr >>> 0, 1));

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('arduboy_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
