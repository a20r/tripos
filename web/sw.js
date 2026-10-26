// Service worker: the whole game is precached at install, so it opens and plays
// with no network. build.sh stamps the build id; a new build installs into a
// fresh cache and the old one is dropped on activation.
const BUILD = '__BUILD__';
const CACHE = 'tripos-' + BUILD;
const PRECACHE = [
  './',
  'index.html',
  'style.css',
  'pkg/tripos.js',
  'pkg/tripos_bg.wasm',
  'fonts/orbitron.woff2',
  'fonts/share-tech-mono.woff2',
  'favicon.svg',
  'manifest.webmanifest',
  'icons/icon-192.png',
  'icons/icon-512.png',
  'icons/icon-maskable-512.png',
  'icons/apple-touch-icon.png',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE)
      // 'reload' bypasses the HTTP cache so a new build never precaches a stale file.
      .then((cache) => cache.addAll(PRECACHE.map((url) => new Request(url, { cache: 'reload' }))))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET' || new URL(req.url).origin !== self.location.origin) return;
  event.respondWith(
    // Cache first, ignoring the ?v= cache-busters the page uses: the build id already versions the cache.
    caches.match(req, { ignoreSearch: true }).then((hit) => {
      if (hit) return hit;
      return fetch(req).then((res) => {
        if (res.ok) caches.open(CACHE).then((cache) => cache.put(req, res.clone()));
        return res;
      }).catch(() => (req.mode === 'navigate' ? caches.match('index.html') : undefined));
    })
  );
});
