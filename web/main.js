// Full-featured web client for arduboy-core.
//
// Drives the wasm emulator: canvas video, AudioWorklet sound, keyboard + touch
// input, ROM drag-and-drop, palette themes, PNG screenshots, GIF recording,
// quick save states, and per-ROM EEPROM/state persistence in IndexedDB.

import { DEFAULT_SKIN, SKINS, getSkin } from './skins.js';

const BASE_VOLUME = 0.25;     // scaled by the volume slider
const STEP_MS = 1000 / 60;    // Arduboy runs at 60 fps

const $ = (id) => document.getElementById(id);
const canvas = $('screen');
const ctx = canvas.getContext('2d', { willReadFrequently: true });

let imageData = null;
let running = false;
let paused = false;
let romName = '';
let volume = 0.6;
let muted = false;
let palette = 'white';
let skin = DEFAULT_SKIN;
const inputSources = Array.from({ length: 6 }, () => new Set());
const activeGamepads = new Set();
let catalog = null;
let worker = null;
let workerRequest = 0;
let stepPending = false;
let latestLeds = { rgb: [0, 0, 0], tx: false, rx: false };
let latestFrame = null;
const workerRequests = new Map();

function workerCall(type, payload = {}, transfer = []) {
  const id = ++workerRequest;
  return new Promise((resolve, reject) => {
    workerRequests.set(id, { resolve, reject });
    worker.postMessage({ id, type, payload }, transfer);
  });
}

function workerNotify(type, payload = {}, transfer = []) {
  worker.postMessage({ id: 0, type, payload }, transfer);
}

function initWorker() {
  worker = new Worker('./emulator-worker.js', { type: 'module' });
  worker.addEventListener('message', ({ data }) => {
    if (data.type === 'frame') {
      stepPending = false;
      latestLeds = { rgb: data.led, tx: data.tx, rx: data.rx };
      latestFrame = new Uint8Array(data.frame);
      draw(latestFrame);
      if (data.audio && audioNode && !muted) {
        const samples = new Float32Array(data.audio);
        audioNode.port.postMessage(samples, [samples.buffer]);
      }
      updateLeds();
      return;
    }
    if (data.type !== 'response' || !data.id) return;
    const request = workerRequests.get(data.id);
    if (!request) return;
    workerRequests.delete(data.id);
    if (data.error) request.reject(new Error(data.error));
    else request.resolve(data.result);
  });
  worker.addEventListener('error', (event) => {
    for (const { reject } of workerRequests.values()) reject(event.error || new Error(event.message));
    workerRequests.clear();
    setStatus(`Emulator worker failed: ${event.message}`, true);
  });
  return workerCall('init');
}

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
function draw(f) {
  if (!imageData || !f) return;
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
  pollGamepads();
  if (!running || paused) { lastTime = now; return; }
  acc += now - lastTime;
  lastTime = now;
  if (acc > 250) acc = 250;

  // Never queue frames: if the worker is still busy, retain only the current
  // timing debt and send the next frame when it completes.
  if (acc >= STEP_MS && !stepPending) {
    acc -= STEP_MS;
    stepPending = true;
    workerNotify('step', {
      sampleRate: audioCtx ? audioCtx.sampleRate : 48000,
      volume: audioNode && !muted ? BASE_VOLUME * volume : 0,
    });
  }
}

// ── Input aggregation / gamepads ───────────────────────────────────────────
// A button can be held by the keyboard, touch UI, and one or more gamepads at
// the same time.  Release it only after every source releases it.
function setInput(button, source, pressed) {
  const sources = inputSources[button];
  const wasPressed = sources.size > 0;
  if (pressed) sources.add(source);
  else sources.delete(source);
  const isPressed = sources.size > 0;
  if (wasPressed !== isPressed && worker) workerNotify('setButton', { button, pressed: isPressed });
  const pad = document.querySelector(`.pad[data-btn="${button}"]`);
  if (pad) pad.classList.toggle('active', isPressed);
}

function gamepadButton(gamepad, index) {
  const value = gamepad.buttons[index];
  return !!value && (value.pressed || value.value >= 0.5);
}

function pollGamepads() {
  if (!navigator.getGamepads) return;
  const pads = [...navigator.getGamepads()].filter(Boolean);
  const seen = new Set();
  for (const gamepad of pads) {
    const source = `gamepad-${gamepad.index}`;
    seen.add(source);
    const x = gamepad.axes[0] || 0;
    const y = gamepad.axes[1] || 0;
    const pressed = [
      gamepadButton(gamepad, 12) || y <= -0.5,
      gamepadButton(gamepad, 13) || y >= 0.5,
      gamepadButton(gamepad, 14) || x <= -0.5,
      gamepadButton(gamepad, 15) || x >= 0.5,
      gamepadButton(gamepad, 0) || gamepadButton(gamepad, 2),
      gamepadButton(gamepad, 1) || gamepadButton(gamepad, 3),
    ];
    pressed.forEach((on, button) => setInput(button, source, on));
  }
  for (const source of activeGamepads) {
    if (seen.has(source)) continue;
    for (let button = 0; button < inputSources.length; button++) setInput(button, source, false);
  }
  activeGamepads.clear();
  seen.forEach((source) => activeGamepads.add(source));
  $('gamepad').textContent = pads.length
    ? `Gamepad: ${pads.map((pad) => pad.id || `#${pad.index}`).join(', ')}`
    : 'Gamepad: none';
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
  if (!running || !romName || !await workerCall('eepromDirty')) return;
  try { await idbPut('eeprom', romName, await workerCall('saveEeprom')); } catch (e) { /* quota */ }
}

// ── ROM loading ────────────────────────────────────────────────────────────
async function loadRom(file) {
  loadRomBytes(file.name, new Uint8Array(await file.arrayBuffer()));
}

async function loadRomBytes(name, bytes) {
  await persistEeprom(); // flush the previous game's EEPROM first
  let result;
  try {
    result = await workerCall('loadFile', { name, data: bytes.buffer }, [bytes.buffer]);
  } catch (err) {
    setStatus(`Load failed: ${err}`, true);
    return;
  }
  romName = name;

  // Restore this ROM's saved EEPROM, if any.
  try {
    const saved = await idbGet('eeprom', romName);
    if (saved && saved.length) {
      const data = new Uint8Array(saved);
      await workerCall('loadEeprom', { data: data.buffer }, [data.buffer]);
    }
  } catch (e) { /* ignore */ }

  await ensureAudio();
  running = true;
  paused = false;
  $('pause').textContent = 'Pause';
  setControlsEnabled(true);
  const cpu = result.cpuType === 1 ? 'ATmega328P' : 'ATmega32u4';
  $('cpu').textContent = cpu;
  setStatus(`Loaded ${name}`);
  canvas.focus();
}

async function autoLoadFromQuery() {
  const url = new URLSearchParams(location.search).get('rom');
  if (!url) return;
  try {
    // Plain cross-origin fetch: succeeds only if the host allows CORS.
    // See "ROM loading & CORS policy" in web/README.md.
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const bytes = new Uint8Array(await resp.arrayBuffer());
    await loadRomBytes(url.split('/').pop() || 'rom.hex', bytes);
  } catch (err) {
    // A CORS or mixed-content block surfaces as an opaque "Failed to fetch".
    const hint = /^http:/i.test(url)
      ? ' (http:// is blocked as mixed content — use https://)'
      : ' (the host may not allow cross-origin access; download the ROM and use Open ROM… instead)';
    setStatus(`Could not fetch ${url}: ${err}${hint}`, true);
  }
}

// ── Online ROM catalog ────────────────────────────────────────────────────
// The catalog is generated from eried/ArduboyCollection and committed with the
// site. ROMs and screenshots themselves remain on the upstream GitHub project.
function catalogMatches(game, query, category) {
  if (category && game.category !== category) return false;
  if (!query) return true;
  const text = `${game.title} ${game.author || ''} ${game.description || ''} ${game.category}`.toLowerCase();
  return text.includes(query);
}

function renderCatalog() {
  if (!catalog) return;
  const query = $('catalog-search').value.trim().toLowerCase();
  const category = $('catalog-category').value;
  const matches = catalog.games.filter((game) => catalogMatches(game, query, category));
  const results = $('catalog-results');
  results.replaceChildren();
  for (const game of matches.slice(0, 120)) {
    const button = document.createElement('button');
    button.className = 'catalog-game';
    button.type = 'button';
    button.title = game.description || game.title;
    if (game.imageUrl) {
      const image = document.createElement('img');
      image.src = game.imageUrl;
      image.alt = '';
      image.loading = 'lazy';
      image.addEventListener('error', () => image.remove());
      button.append(image);
    }
    const text = document.createElement('span');
    const title = document.createElement('b');
    title.textContent = game.title;
    const byline = document.createElement('small');
    byline.textContent = [game.author, game.category].filter(Boolean).join(' · ') || 'Unknown author';
    text.append(title, byline);
    button.append(text);
    button.addEventListener('click', () => loadCatalogRom(game));
    results.append(button);
  }
  const limit = matches.length > 120 ? ' — refine your search to see more' : '';
  $('catalog-meta').textContent = `${matches.length} game${matches.length === 1 ? '' : 's'}${limit}`;
}

async function openCatalog() {
  const panel = $('catalog-panel');
  panel.hidden = false;
  if (catalog) { renderCatalog(); return; }
  $('catalog-meta').textContent = 'Loading catalog…';
  try {
    const response = await fetch('./catalogs/arduboy-collection.json', { cache: 'no-cache' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    catalog = await response.json();
    const select = $('catalog-category');
    const categories = [...new Set(catalog.games.map((game) => game.category).filter(Boolean))].sort();
    for (const category of categories) {
      const option = document.createElement('option');
      option.value = category;
      option.textContent = category;
      select.append(option);
    }
    renderCatalog();
  } catch (err) {
    $('catalog-meta').textContent = `Could not load the catalog: ${err}`;
  }
}

async function loadCatalogRom(game) {
  $('catalog-meta').textContent = `Downloading ${game.title}…`;
  try {
    const response = await fetch(game.hexUrl);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const extension = game.hexUrl.toLowerCase().includes('.arduboy') ? '.arduboy' : '.hex';
    await loadRomBytes(`catalog:${game.id}${extension}`, new Uint8Array(await response.arrayBuffer()));
    $('catalog-panel').hidden = true;
  } catch (err) {
    setStatus(`Could not fetch ${game.title}: ${err}`, true);
    $('catalog-meta').textContent = `Download failed: ${err}`;
  }
}

// ── State / EEPROM actions ────────────────────────────────────────────────
async function saveState() {
  if (!running) return;
  try {
    await idbPut('states', romName, await workerCall('saveState'));
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
    const data = new Uint8Array(bytes);
    await workerCall('loadState', { data: data.buffer }, [data.buffer]);
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
async function toggleGif() {
  if (!running) return;
  const btn = $('gif');
  if (await workerCall('gifRecording')) {
    const bytes = await workerCall('gifStop');
    btn.classList.remove('recording');
    btn.textContent = '● Rec GIF';
    if (bytes.length) download(new Blob([bytes], { type: 'image/gif' }), `${baseName()}-${stamp()}.gif`);
    setStatus('GIF saved');
  } else {
    await workerCall('gifStart');
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
async function reset() {
  if (!running) return;
  await workerCall('reset');
  if (paused) { paused = false; $('pause').textContent = 'Pause'; ensureAudio(); }
}
function toggleMute() {
  muted = !muted;
  $('mute').textContent = muted ? 'Unmute' : 'Mute';
}
function applyScale(v) {
  // Device skins own the screen's geometry, so scaling only the canvas would
  // make it overflow its bezel.  Keep the display fitted and scale the whole
  // device instead; 5× preserves the original default visual size.
  const zoom = v === 'fit' ? 1 : Number(v) / 5;
  $('device').style.setProperty('--device-zoom', String(zoom));
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
  const [r, g, b] = latestLeds.rgb;
  $('rgb').style.background = `rgb(${r},${g},${b})`;
  $('tx').style.background = latestLeds.tx ? 'var(--good)' : '#333';
  $('rx').style.background = latestLeds.rx ? 'var(--good)' : '#333';
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
    if (!e.repeat) setInput(btn, `key-${e.code}`, true);
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
  if (btn !== undefined) { e.preventDefault(); setInput(btn, `key-${e.code}`, false); }
}

function wireTouch() {
  for (const pad of document.querySelectorAll('.pad')) {
    const btn = Number(pad.dataset.btn);
    const press = (on) => {
      if (!worker) return;
      setInput(btn, `touch-${btn}`, on);
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
  if ('serviceWorker' in navigator) {
    // Keep the emulator, its WASM module, and the locally generated catalog
    // available when the installed app is launched without a connection.
    navigator.serviceWorker.register('./service-worker.js').catch((err) => {
      console.warn('Service worker registration failed:', err);
    });
  }
  const dimensions = await initWorker();
  // A small debug facade keeps command inspection possible without exposing
  // the worker-owned wasm instance to the UI thread.
  window.__abemu = { call: workerCall };
  imageData = ctx.createImageData(dimensions.width, dimensions.height);
  if (latestFrame) draw(latestFrame);
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
  window.addEventListener('gamepadconnected', pollGamepads);
  window.addEventListener('gamepaddisconnected', pollGamepads);
  wireTouch();

  $('file').addEventListener('change', (e) => { if (e.target.files[0]) loadRom(e.target.files[0]); });
  $('catalog').addEventListener('click', openCatalog);
  $('catalog-close').addEventListener('click', () => { $('catalog-panel').hidden = true; });
  $('catalog-search').addEventListener('input', renderCatalog);
  $('catalog-category').addEventListener('change', renderCatalog);
  $('fx').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    const data = new Uint8Array(await f.arrayBuffer());
    await workerCall('loadFx', { data: data.buffer }, [data.buffer]);
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
  $('palette').addEventListener('change', (e) => { palette = e.target.value; if (latestFrame) draw(latestFrame); });
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
