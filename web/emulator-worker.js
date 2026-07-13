// Emulator execution thread.  UI, Canvas and AudioWorklet stay on the main
// thread; this worker owns the wasm instance and transfers completed output.
import init, { AbEmu } from './pkg/arduboy.js';

let emu = null;

function bytes(value) {
  return new Uint8Array(value);
}

function copyBytes(value) {
  return new Uint8Array(value).slice();
}

function postFrame(includeAudio = false, sampleRate = 48000, volume = 0) {
  const frame = copyBytes(emu.frame());
  const message = { type: 'frame', frame: frame.buffer, led: Array.from(emu.ledRgb()), tx: emu.ledTx(), rx: emu.ledRx() };
  const transfer = [frame.buffer];
  if (includeAudio && volume > 0) {
    const audio = new Float32Array(emu.renderAudio(sampleRate, volume));
    message.audio = audio.buffer;
    transfer.push(audio.buffer);
  }
  self.postMessage(message, transfer);
}

async function handle(type, payload) {
  switch (type) {
    case 'init':
      await init(new URL('./pkg/arduboy_bg.wasm', import.meta.url));
      emu = new AbEmu();
      postFrame();
      return { width: AbEmu.screenWidth(), height: AbEmu.screenHeight() };
    case 'step':
      emu.runFrame();
      postFrame(true, payload.sampleRate, payload.volume);
      return null;
    case 'setButton': emu.setButton(payload.button, payload.pressed); return null;
    case 'loadFile':
      emu.loadFile(payload.name, bytes(payload.data));
      postFrame();
      return { cpuType: emu.cpuType() };
    case 'loadFx': emu.loadFx(bytes(payload.data)); postFrame(); return null;
    case 'reset': emu.reset(); postFrame(); return null;
    case 'loadEeprom': emu.loadEeprom(bytes(payload.data)); return null;
    case 'eepromDirty': return emu.eepromDirty();
    case 'saveEeprom': return copyBytes(emu.saveEeprom());
    case 'saveState': return copyBytes(emu.saveState());
    case 'loadState': emu.loadState(bytes(payload.data)); postFrame(); return null;
    case 'gifStart': emu.gifStart(); return null;
    case 'gifRecording': return emu.gifRecording();
    case 'gifStop': return copyBytes(emu.gifStop());
    default: throw new Error(`Unknown emulator command: ${type}`);
  }
}

self.onmessage = async ({ data }) => {
  const { id, type, payload = {} } = data;
  try {
    const result = await handle(type, payload);
    const transfer = result instanceof Uint8Array ? [result.buffer] : [];
    self.postMessage({ type: 'response', id, result }, transfer);
  } catch (error) {
    self.postMessage({ type: 'response', id, error: String(error?.message || error) });
  }
};
