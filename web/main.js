// Full-featured web client for arduboy-core.
//
// Drives the wasm emulator: canvas video, AudioWorklet sound, keyboard + touch
// input, ROM drag-and-drop, palette themes, PNG screenshots, GIF recording,
// quick save states, and per-ROM EEPROM/state persistence in IndexedDB.

import init, { AbEmu } from './pkg/arduboy.js';
import { DEFAULT_SKIN, SKINS, getSkin } from './skins.js';

const BASE_VOLUME = 0.25;     // scaled by the volume slider
const STEP_MS = 1000 / 60;    // Arduboy runs at 60 fps

const $ = (id) => document.getElementById(id);
const canvas = $('screen');
const ctx = canvas.getContext('2d', { willReadFrequently: true });

let emu = null;
let imageData = null;
let running = false;
let paused = false;
let romName = '';
let volume = 0.6;
let muted = false;
let palette = 'white';
let skin = DEFAULT_SKIN;

// ── Palette themes (lit / unlit RGB) ─────────────────────────────────────────
const PALETTES = {
  white: null, // pass framebuffer through unchanged
  green: { on: [150, 230, 160], off: [12, 28, 18] },
  amber: { on: [255, 190, 80], off: [28, 18, 6] },
};

// ── Audio ────────────────────────────────────────────────────────────────────
let audioCtx = null;
let audioNode = null;

async function ensureAudio() {
  if (audioCtx) {
    if (audioCtx.state === 'suspended') await audioCtx.resume();
    return;
  }
  audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  try {
    await audioCtx.audioWorklet.addModule('./audio-worklet.js');
    audioNode = new AudioWorkletNode(audioCtx, 'arduboy-audio', { outputChannelCount: [2] });
    audioNode.connect(audioCtx.destination);
  } catch (err) {
    console.warn('AudioWorklet unavailable, running muted:', err);
    audioNode = null;
  }
}

// ── Rendering ────────────────────────────────────────────────────────────────
function draw() {
  const f = emu.frame();
  const d = imageData.data;
  const pal = PALETTES[palette];
  if (!pal) {
    d.set(f);
  } else {
    for (let i = 0, p = 0; i < f.length; i += 4, p += 4) {
      const on = f[i] >= 128;
      const c = on ? pal.on : pal.off;
      d[p] = c[0]; d[p + 1] = c[1]; d[p + 2] = c[2]; d[p + 3] = 255;
    }
  }
  ctx.putImageData(imageData, 0, 0);
}

let lastTime = 0;
let acc = 0;

function loop(now) {
  if (!running || paused) { lastTime = now; return; }
  acc += now - lastTime;
  lastTime = now;
  if (acc > 250) acc = 250;

  let stepped = false;
  const rate = audioCtx ? audioCtx.sampleRate : 48000;
  while (acc >= STEP_MS) {
    emu.runFrame();
    if (audioNode && !muted) {
      const s = emu.renderAudio(rate, BASE_VOLUME * volume);
      audioNode.port.postMessage(s, [s.buffer]);
    }
    acc -= STEP_MS;
    stepped = true;
  }
  if (stepped) { draw(); updateLeds(); }
}

// ── IndexedDB persistence ─────────────────────────────────────────────────────
function idb() {
  return new Promise((res, rej) => {
    const r = indexedDB.open('arduboy', 1);
    r.onupgradeneeded = () => {
      const db = r.result;
      if (!db.objectStoreNames.contains('eeprom')) db.createObjectStore('eeprom');
      if (!db.objectStoreNames.contains('states')) db.createObjectStore('states');
    };
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
}
async function idbPut(store, key, val) {
  const db = await idb();
  return new Promise((res, rej) => {
    const tx = db.transaction(store, 'readwrite');
    tx.objectStore(store).put(val, key);
    tx.oncomplete = () => res();
    tx.onerror = () => rej(tx.error);
  });
}
async function idbGet(store, key) {
  const db = await idb();
  return new Promise((res, rej) => {
    const tx = db.transaction(store, 'readonly');
    const rq = tx.objectStore(store).get(key);
    rq.onsuccess = () => res(rq.result);
    rq.onerror = () => rej(rq.error);
  });
}

async function persistEeprom() {
  if (!running || !romName || !emu.eepromDirty()) return;
  try { await idbPut('eeprom', romName, emu.saveEeprom()); } catch (e) { /* quota */ }
}

// ── ROM loading ────────────────────────────────────────────────────────────
async function loadRom(file) {
  loadRomBytes(file.name, new Uint8Array(await file.arrayBuffer()));
}

async function loadRomBytes(name, bytes) {
  await persistEeprom(); // flush the previous game's EEPROM first
  try {
    emu.loadFile(name, bytes);
  } catch (err) {
    setStatus(`Load failed: ${err}`, true);
    return;
  }
  romName = name;

  // Restore this ROM's saved EEPROM, if any.
  try {
    const saved = await idbGet('eeprom', romName);
    if (saved && saved.length) emu.loadEeprom(new Uint8Array(saved));
  } catch (e) { /* ignore */ }

  await ensureAudio();
  running = true;
  paused = false;
  $('pause').textContent = 'Pause';
  setControlsEnabled(true);
  const cpu = emu.cpuType() === 1 ? 'ATmega328P' : 'ATmega32u4';
  $('cpu').textContent = cpu;
  setStatus(`Loaded ${name}`);
  canvas.focus();
}

async function autoLoadFromQuery() {
  const url = new URLSearchParams(location.search).get('rom');
  if (!url) return;
  try {
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const bytes = new Uint8Array(await resp.arrayBuffer());
    await loadRomBytes(url.split('/').pop() || 'rom.hex', bytes);
  } catch (err) {
    setStatus(`Could not fetch ${url}: ${err}`, true);
  }
}

// ── State / EEPROM actions ────────────────────────────────────────────────
async function saveState() {
  if (!running) return;
  try {
    await idbPut('states', romName, emu.saveState());
    setStatus('State saved');
  } catch (err) {
    setStatus(`Save failed: ${err}`, true);
  }
}
async function loadState() {
  if (!running) return;
  try {
    const bytes = await idbGet('states', romName);
    if (!bytes) { setStatus('No saved state for this ROM'); return; }
    emu.loadState(new Uint8Array(bytes));
    setStatus('State loaded');
  } catch (err) {
    setStatus(`Load failed: ${err}`, true);
  }
}

// ── Screenshot / GIF ──────────────────────────────────────────────────────
function download(blob, filename) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url; a.download = filename; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
const stamp = () => new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
const baseName = () => (romName ? romName.replace(/\.[^.]+$/, '') : 'arduboy');

function screenshot() {
  if (!running) return;
  canvas.toBlob((b) => { if (b) download(b, `${baseName()}-${stamp()}.png`); }, 'image/png');
}
function toggleGif() {
  if (!running) return;
  const btn = $('gif');
  if (emu.gifRecording()) {
    const bytes = emu.gifStop();
    btn.classList.remove('recording');
    btn.textContent = '● Rec GIF';
    if (bytes.length) download(new Blob([bytes], { type: 'image/gif' }), `${baseName()}-${stamp()}.gif`);
    setStatus('GIF saved');
  } else {
    emu.gifStart();
    btn.classList.add('recording');
    btn.textContent = '■ Stop';
    setStatus('Recording GIF…');
  }
}

// ── Toggles / view ────────────────────────────────────────────────────────
function togglePause() {
  if (!running) return;
  paused = !paused;
  $('pause').textContent = paused ? 'Resume' : 'Pause';
  if (!paused) ensureAudio();
}
function reset() {
  if (!running) return;
  emu.reset();
  if (paused) { paused = false; $('pause').textContent = 'Pause'; ensureAudio(); }
}
function toggleMute() {
  muted = !muted;
  $('mute').textContent = muted ? 'Unmute' : 'Mute';
}
function applyScale(v) {
  if (v === 'fit') canvas.style.removeProperty('--screen-w');
  else canvas.style.setProperty('--screen-w', `${128 * Number(v)}px`);
}
function applySkin(requested) {
  skin = getSkin(requested);
  const device = $('device');
  device.className = SKINS[skin].className;
  $('skin').value = skin;
  localStorage.setItem('arduboy.skin', skin);
}
function toggleFullscreen() {
  const stage = $('stage');
  if (document.fullscreenElement) document.exitFullscreen();
  else stage.requestFullscreen?.();
}

// ── Status ────────────────────────────────────────────────────────────────
function setStatus(msg, isError = false) {
  const el = $('msg');
  el.textContent = msg;
  el.style.color = isError ? 'var(--accent-2)' : 'var(--muted)';
}
function setControlsEnabled(on) {
  for (const id of ['pause', 'reset', 'save', 'load', 'shot', 'gif']) $(id).disabled = !on;
}
function updateLeds() {
  if (!running) return;
  const [r, g, b] = emu.ledRgb();
  $('rgb').style.background = `rgb(${r},${g},${b})`;
  $('tx').style.background = emu.ledTx() ? 'var(--good)' : '#333';
  $('rx').style.background = emu.ledRx() ? 'var(--good)' : '#333';
}

// FPS meter (sampled from the render loop).
let fpsFrames = 0, fpsLast = performance.now();
function fpsTick() {
  fpsFrames++;
  const now = performance.now();
  if (now - fpsLast >= 500) {
    $('fps').textContent = `${(fpsFrames * 1000 / (now - fpsLast)).toFixed(0)} fps`;
    fpsFrames = 0; fpsLast = now;
  }
}

// ── Input ─────────────────────────────────────────────────────────────────
const KEY_MAP = { ArrowUp: 0, ArrowDown: 1, ArrowLeft: 2, ArrowRight: 3, KeyZ: 4, KeyX: 5 };

function onKeyDown(e) {
  const btn = KEY_MAP[e.code];
  if (btn !== undefined) {
    e.preventDefault();
    ensureAudio();
    if (!e.repeat) emu.setButton(btn, true);
    return;
  }
  const shortcuts = {
    KeyR: reset, KeyP: togglePause, KeyM: toggleMute, KeyG: toggleGif,
    KeyS: screenshot, F5: saveState, F9: loadState,
  };
  if (shortcuts[e.code]) { e.preventDefault(); shortcuts[e.code](); }
}
function onKeyUp(e) {
  const btn = KEY_MAP[e.code];
  if (btn !== undefined) { e.preventDefault(); emu.setButton(btn, false); }
}

function wireTouch() {
  for (const pad of document.querySelectorAll('.pad')) {
    const btn = Number(pad.dataset.btn);
    const press = (on) => {
      if (!emu) return;
      pad.classList.toggle('active', on);
      emu.setButton(btn, on);
    };
    pad.addEventListener('pointerdown', (e) => {
      e.preventDefault(); pad.setPointerCapture?.(e.pointerId); ensureAudio(); press(true);
    });
    for (const ev of ['pointerup', 'pointercancel', 'pointerleave']) {
      pad.addEventListener(ev, (e) => { e.preventDefault(); press(false); });
    }
    // Guard against a stuck button if the pointer is lost.
    pad.addEventListener('lostpointercapture', () => press(false));
  }
}

// ── Wiring ────────────────────────────────────────────────────────────────
async function main() {
  await init();
  emu = new AbEmu();
  window.__abemu = emu; // debugging / testing
  imageData = ctx.createImageData(AbEmu.screenWidth(), AbEmu.screenHeight());
  draw();
  setStatus('Open a ROM to start (.hex / .arduboy)');

  const skinSelect = $('skin');
  for (const [key, value] of Object.entries(SKINS)) {
    const option = document.createElement('option');
    option.value = key;
    option.textContent = `${value.label} — ${value.description}`;
    skinSelect.append(option);
  }
  const requestedSkin = new URLSearchParams(location.search).get('skin') || localStorage.getItem('arduboy.skin');
  applySkin(requestedSkin);

  const frame = (now) => { loop(now); fpsTick(); requestAnimationFrame(frame); };
  requestAnimationFrame((t) => { lastTime = t; requestAnimationFrame(frame); });

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
  wireTouch();

  $('file').addEventListener('change', (e) => { if (e.target.files[0]) loadRom(e.target.files[0]); });
  $('fx').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    emu.loadFx(new Uint8Array(await f.arrayBuffer()));
    setStatus(`FX loaded: ${f.name}`);
  });
  $('pause').addEventListener('click', togglePause);
  $('reset').addEventListener('click', reset);
  $('save').addEventListener('click', saveState);
  $('load').addEventListener('click', loadState);
  $('shot').addEventListener('click', screenshot);
  $('gif').addEventListener('click', toggleGif);
  $('full').addEventListener('click', toggleFullscreen);
  $('mute').addEventListener('click', toggleMute);
  $('vol').addEventListener('input', (e) => { volume = e.target.value / 100; });
  $('scale').addEventListener('change', (e) => applyScale(e.target.value));
  $('palette').addEventListener('change', (e) => { palette = e.target.value; if (running) draw(); });
  skinSelect.addEventListener('change', (e) => applySkin(e.target.value));

  applyScale($('scale').value);

  // Persist EEPROM periodically and when the page is hidden/closed.
  setInterval(persistEeprom, 4000);
  document.addEventListener('visibilitychange', () => { if (document.hidden) persistEeprom(); });
  window.addEventListener('pagehide', persistEeprom);

  // Drag-and-drop anywhere.
  const dropEl = $('drop');
  const isRom = (name) => /\.(hex|arduboy|elf)$/i.test(name);
  window.addEventListener('dragover', (e) => { e.preventDefault(); dropEl.classList.add('show'); });
  window.addEventListener('dragleave', (e) => { if (e.relatedTarget === null) dropEl.classList.remove('show'); });
  window.addEventListener('drop', (e) => {
    e.preventDefault();
    dropEl.classList.remove('show');
    const file = [...(e.dataTransfer?.files || [])].find((f) => isRom(f.name));
    if (file) loadRom(file);
  });

  await autoLoadFromQuery();
}

main().catch((err) => { console.error(err); setStatus(`Init error: ${err}`, true); });
