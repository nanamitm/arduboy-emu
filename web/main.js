// Web client for arduboy-core: drives the wasm emulator, renders to a canvas,
// plays audio via an AudioWorklet, and loads ROMs from a file picker or drop.

import init, { AbEmu } from './pkg/arduboy.js';

const VOLUME = 0.15;          // matches the desktop frontends
const STEP_MS = 1000 / 60;    // Arduboy runs at 60 fps

const $ = (id) => document.getElementById(id);
const statusEl = $('status');
const canvas = $('screen');
const ctx = canvas.getContext('2d');

let emu = null;
let imageData = null;
let running = false;          // a ROM is loaded and not paused
let paused = false;

// ── Audio ────────────────────────────────────────────────────────────────────
let audioCtx = null;
let audioNode = null;
let muted = false;

async function ensureAudio() {
  if (audioCtx) {
    if (audioCtx.state === 'suspended') await audioCtx.resume();
    return;
  }
  audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  try {
    await audioCtx.audioWorklet.addModule('./audio-worklet.js');
    audioNode = new AudioWorkletNode(audioCtx, 'arduboy-audio', {
      outputChannelCount: [2],
    });
    audioNode.connect(audioCtx.destination);
  } catch (err) {
    console.warn('AudioWorklet unavailable, running muted:', err);
    audioNode = null;
  }
}

// ── Rendering / emulation loop ───────────────────────────────────────────────
function draw() {
  imageData.data.set(emu.frame());
  ctx.putImageData(imageData, 0, 0);
}

let lastTime = 0;
let acc = 0;

function loop(now) {
  requestAnimationFrame(loop);
  if (!running || paused) { lastTime = now; return; }

  acc += now - lastTime;
  lastTime = now;
  if (acc > 250) acc = 250; // don't spiral after a tab was backgrounded

  let stepped = false;
  const rate = audioCtx ? audioCtx.sampleRate : 48000;
  while (acc >= STEP_MS) {
    emu.runFrame();
    if (audioNode && !muted) {
      const samples = emu.renderAudio(rate, VOLUME);
      // Transfer the buffer to avoid a copy.
      audioNode.port.postMessage(samples, [samples.buffer]);
    }
    acc -= STEP_MS;
    stepped = true;
  }
  if (stepped) draw();
}

// ── ROM loading ──────────────────────────────────────────────────────────────
async function loadRom(file) {
  loadRomBytes(file.name, new Uint8Array(await file.arrayBuffer()));
}

async function loadRomBytes(name, bytes) {
  try {
    emu.loadFile(name, bytes);
  } catch (err) {
    setStatus(`Load failed: ${err}`, true);
    return;
  }
  await ensureAudio();
  running = true;
  paused = false;
  $('pause').textContent = 'Pause';
  const cpu = emu.cpuType() === 1 ? 'ATmega328P' : 'ATmega32u4';
  setStatus(`Loaded ${name} · ${cpu}`);
  canvas.focus();
}

// Optionally auto-load a ROM referenced by `?rom=<url>` (same-origin), handy for
// deep links to a hosted game and for automated testing.
async function autoLoadFromQuery() {
  const url = new URLSearchParams(location.search).get('rom');
  if (!url) return;
  try {
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const bytes = new Uint8Array(await resp.arrayBuffer());
    const name = url.split('/').pop() || 'rom.hex';
    await loadRomBytes(name, bytes);
  } catch (err) {
    setStatus(`Could not fetch ${url}: ${err}`, true);
  }
}

function setStatus(msg, isError = false) {
  statusEl.textContent = msg;
  statusEl.style.color = isError ? 'var(--accent-2)' : 'var(--muted)';
}

// ── Input ────────────────────────────────────────────────────────────────────
const KEY_MAP = {
  ArrowUp: 0, ArrowDown: 1, ArrowLeft: 2, ArrowRight: 3,
  KeyZ: 4, KeyX: 5,
};

function onKey(down) {
  return (e) => {
    const btn = KEY_MAP[e.code];
    if (btn === undefined || !emu) return;
    e.preventDefault();
    if (e.repeat) return;
    emu.setButton(btn, down);
  };
}

// ── Wiring ───────────────────────────────────────────────────────────────────
async function main() {
  await init();
  emu = new AbEmu();
  imageData = ctx.createImageData(AbEmu.screenWidth(), AbEmu.screenHeight());
  draw();
  setStatus('Open a ROM to start (.hex / .arduboy)');
  window.__abemu = emu; // expose for debugging / testing

  requestAnimationFrame((t) => { lastTime = t; requestAnimationFrame(loop); });

  window.addEventListener('keydown', onKey(true));
  window.addEventListener('keyup', onKey(false));

  $('file').addEventListener('change', (e) => {
    if (e.target.files[0]) loadRom(e.target.files[0]);
  });

  $('pause').addEventListener('click', async () => {
    if (!running) return;
    paused = !paused;
    $('pause').textContent = paused ? 'Resume' : 'Pause';
    if (!paused) await ensureAudio();
  });

  $('reset').addEventListener('click', () => { if (running) emu.reset(); });

  $('mute').addEventListener('click', () => {
    muted = !muted;
    $('mute').textContent = muted ? 'Unmute' : 'Mute';
  });

  // Drag-and-drop anywhere on the page.
  const dropEl = $('drop');
  const isRom = (name) => /\.(hex|arduboy|elf)$/i.test(name);
  window.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropEl.classList.add('show');
  });
  window.addEventListener('dragleave', (e) => {
    if (e.relatedTarget === null) dropEl.classList.remove('show');
  });
  window.addEventListener('drop', (e) => {
    e.preventDefault();
    dropEl.classList.remove('show');
    const file = [...(e.dataTransfer?.files || [])].find((f) => isRom(f.name));
    if (file) loadRom(file);
  });

  await autoLoadFromQuery();
}

main().catch((err) => {
  console.error(err);
  setStatus(`Init error: ${err}`, true);
});
