// Offline shell for the installable web client. Upstream ROM downloads remain
// network requests so their licenses and latest versions stay with their host.
const CACHE_NAME = 'arduboy-web-v1';
const APP_SHELL = [
  './',
  './index.html',
  './main.js',
  './skins.js',
  './audio-worklet.js',
  './manifest.webmanifest',
  './icon.png',
  './catalogs/arduboy-collection.json',
  './pkg/arduboy.js',
  './pkg/arduboy_bg.wasm',
];

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(APP_SHELL)));
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(
    keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key)),
  )));
  self.clients.claim();
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin || request.method !== 'GET') return;

  if (request.mode === 'navigate') {
    event.respondWith(fetch(request).catch(() => caches.match('./index.html')));
    return;
  }
  event.respondWith(caches.match(request).then((cached) => cached || fetch(request)));
});
