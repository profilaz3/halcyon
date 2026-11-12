// Schema 7 details: what the app was used for since its last report, which
// problems it hit, how far setup got, and a few facts about the system it runs
// on. Fixed keys and fixed values only: no text, names, paths or locale.

export const PAGES = {
  overview: 'Widgets', layout: 'Arrange widgets', pending: 'Recover unfinished work',
  appearance: 'Glass and transparency', widgets: 'Widgets', glance: 'Daily overview', calendar: 'Calendar',
  weather: 'Weather', devbox: 'System monitor', clock: 'Clock', projects: 'Projects', music: 'Music',
  notes: 'Notes', focus: 'Focus timer', devices: 'Devices', general: 'Halcyon', desktop: 'Desktop', 'desktop-dock': 'Dock and panel',
  'desktop-appearance': 'Fonts and icons', 'desktop-wallpaper': 'Wallpaper', 'desktop-icons': 'Icons on the desktop',
  'desktop-profiles': 'Looks', themes: 'Appearance', 'desktop-windows': 'Windows',
  'desktop-workspaces': 'Workspaces', 'desktop-motion': 'Accessibility',
  'desktop-window-rules': 'Apps that don’t tile', 'desktop-shortcuts': 'Shortcuts',
  'desktop-automation': 'Time of day', moments: 'Time of day (widget styles)', adaptive: 'Adaptive looks',
  builder: 'Widget builder', changes: 'Changes',
};

// Update and Islands failures, as named by the app's own Failure kinds.
const FAILURES = {
  missing_packagekit: 'update service missing', offline: 'offline', timeout: 'timed out',
  authorization: 'permission not given', untrusted: 'sources not verified', invalid_response: 'incomplete answer',
  failed: 'system could not finish', installation_changed: 'Halcyon changed meanwhile',
  activation: 'activation failed', islands_mismatch: 'package mismatch', islands_refused: 'refused by helper',
};
export const PROBLEMS = {
  panic: 'Internal error (panic)',
  render_timeout: 'Image render timed out',
  ...Object.fromEntries(Object.entries(FAILURES).map(([k, v]) => [`update.${k}`, `Update check: ${v}`])),
  ...Object.fromEntries(Object.entries(FAILURES).map(([k, v]) => [`islands.${k}`, `Islands: ${v}`])),
};

export const SETUP_STEPS = {look: 'Look', bars: 'Dock and panel', widgets: 'Widgets', arrange: 'Arrange',
  weather: 'Weather (older setup)', ready: 'Review', done: 'Done'};
const SKIPPABLE = ['bars', 'widgets', 'arrange'];
const OUTCOMES = {opened: 'Opened setup', left: 'Skipped from Welcome', kept_for_later: 'Set up later',
  cancelled: 'Cancelled', finished: 'Finished', undone: 'Undid setup'};
export const SETUP = {
  ...OUTCOMES,
  ...Object.fromEntries(Object.entries(SETUP_STEPS).map(([k, v]) => [`reached.${k}`, `Reached ${v}`])),
  ...Object.fromEntries(SKIPPABLE.map(k => [`skipped.${k}`, `Skipped ${SETUP_STEPS[k]}`])),
};

export const GPUS = {amd: 'AMD', intel: 'Intel', nvidia: 'NVIDIA', other: 'Other'};
const MAX_COUNT = 100000;
const DETAIL_KEYS = ['pages', 'problems', 'runtime_hours', 'setup', 'system'];
const SYSTEM_KEYS = ['cosmic', 'displays', 'distro', 'distro_version', 'gpu', 'resolution'];

const plainObject = v => v && typeof v === 'object' && !Array.isArray(v);
const sameKeys = (v, keys) => Object.keys(v).sort().join(',') === [...keys].sort().join(',');

// Positive whole counts under known keys, stored in sorted key order.
function counts(v, vocabulary) {
  if (!plainObject(v)) return null;
  const keys = Object.keys(v).sort();
  if (keys.length > Object.keys(vocabulary).length) return null;
  const out = {};
  for (const key of keys) {
    const n = v[key];
    if (!Object.hasOwn(vocabulary, key) || !Number.isSafeInteger(n) || n < 1 || n > MAX_COUNT) return null;
    out[key] = n;
  }
  return out;
}

const nullable = (v, test) => v === null || test(v);
export const validDistro = v => typeof v === 'string' && /^[a-z0-9][a-z0-9._-]{0,31}$/.test(v);
export const validDistroVersion = v => typeof v === 'string' && /^[a-z0-9][a-z0-9._-]{0,15}$/.test(v);
export const validCosmic = v => typeof v === 'string' && /^\d{1,4}(\.\d{1,6}){0,2}$/.test(v);
const validResolution = v => typeof v === 'string' && /^[1-9]\d{0,4}x[1-9]\d{0,4}$/.test(v)
  && v.split('x').every(n => Number(n) <= 16384);

function system(v) {
  if (!plainObject(v) || !sameKeys(v, SYSTEM_KEYS)) return null;
  const ok = nullable(v.distro, validDistro) && nullable(v.distro_version, validDistroVersion)
    && nullable(v.cosmic, validCosmic) && nullable(v.resolution, validResolution)
    && nullable(v.displays, n => Number.isSafeInteger(n) && n >= 1 && n <= 16)
    && nullable(v.gpu, g => Array.isArray(g) && g.length >= 1 && new Set(g).size === g.length
      && g.every(x => Object.hasOwn(GPUS, x)));
  if (!ok) return null;
  return {
    distro: v.distro, distro_version: v.distro === null ? null : v.distro_version, cosmic: v.cosmic,
    gpu: v.gpu === null ? null : Object.keys(GPUS).filter(g => v.gpu.includes(g)),
    displays: v.displays, resolution: v.resolution,
  };
}

// The canonical stored form of a schema 7 report's details, or null.
export function canonicalDetails(v) {
  if (!plainObject(v) || !sameKeys(v, DETAIL_KEYS)) return null;
  const pages = counts(v.pages, PAGES), problems = counts(v.problems, PROBLEMS), setup = counts(v.setup, SETUP);
  const sys = system(v.system);
  const runtime = v.runtime_hours;
  if (!pages || !problems || !setup || !sys
      || !nullable(runtime, n => Number.isSafeInteger(n) && n >= 0 && n <= 10000)) return null;
  return JSON.stringify({pages, problems, runtime_hours: runtime, setup, system: sys});
}

export const parseDetails = text => {
  if (!text) return null;
  try { return JSON.parse(text); } catch (_) { return null; }
};

const median = list => {
  if (!list.length) return null;
  const s = [...list].sort((a, b) => a - b), m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

// Exact aggregates over the window. Use, problems and setup add up every
// report; the system facts come from each installation's latest report.
export function detailsSummary(rows, from, to) {
  const perInstall = new Map(), latest = new Map(), runtimes = [];
  for (const r of rows) {
    if (r.day < from || r.day > to) continue;
    const d = parseDetails(r.details);
    if (!d) continue;
    if (!perInstall.has(r.n)) perInstall.set(r.n, []);
    perInstall.get(r.n).push(d);
    latest.set(r.n, d.system);
    if (d.runtime_hours !== null) runtimes.push(d.runtime_hours);
  }
  const out = {reported: perInstall.size, reports: [...perInstall.values()].reduce((n, l) => n + l.length, 0)};
  if (!out.reported) return {...out, pages: null, problems: null, setup: null, runtime: null, system: null};

  // For each key: how many installations had it, and the total count.
  const tally = (field, vocabulary) => {
    const installs = new Map(), totals = new Map();
    for (const list of perInstall.values()) {
      const seen = new Set();
      for (const d of list) for (const [key, n] of Object.entries(d[field] || {})) {
        totals.set(key, (totals.get(key) || 0) + n);
        seen.add(key);
      }
      for (const key of seen) installs.set(key, (installs.get(key) || 0) + 1);
    }
    return [...totals.keys()].filter(k => Object.hasOwn(vocabulary, k))
      .map(key => ({key, name: vocabulary[key], installations: installs.get(key), count: totals.get(key)}))
      .sort((a, b) => b.installations - a.installations || b.count - a.count || a.name.localeCompare(b.name));
  };
  out.pages = tally('pages', PAGES);
  const problems = tally('problems', PROBLEMS);
  const affected = [...perInstall.values()].filter(list => list.some(d => Object.keys(d.problems || {}).length)).length;
  out.problems = {items: problems, affected, clean: out.reported - affected};
  const setup = new Map(tally('setup', SETUP).map(i => [i.key, i]));
  const row = (key, name) => ({key, name, installations: setup.get(key)?.installations || 0, count: setup.get(key)?.count || 0});
  out.setup = {
    started: setup.get('opened')?.installations || 0,
    steps: Object.entries(SETUP_STEPS).filter(([k]) => k !== 'weather' || setup.has('reached.weather'))
      .map(([k, name]) => ({...row(`reached.${k}`, name), skipped: setup.get(`skipped.${k}`)?.installations || 0})),
    outcomes: Object.entries(OUTCOMES).map(([k, name]) => row(k, name)),
  };
  out.runtime = runtimes.length ? {reports: runtimes.length, median_hours: median(runtimes),
    buckets: [['Under 1 hour', h => h < 1], ['1 to 3 hours', h => h >= 1 && h <= 3], ['4 to 8 hours', h => h >= 4 && h <= 8],
      ['9 to 16 hours', h => h >= 9 && h <= 16], ['More than 16 hours', h => h > 16]]
      .map(([name, test]) => ({name, count: runtimes.filter(test).length}))} : null;

  const systems = [...latest.values()].filter(Boolean);
  const breakdown = (value, label = k => k) => {
    const m = new Map();
    for (const s of systems) { const k = value(s); m.set(k, (m.get(k) || 0) + 1); }
    return [...m].map(([key, count]) => ({key: key ?? 'unknown', name: key === null ? 'Not reported' : label(key), count}))
      .sort((a, b) => (a.key === 'unknown') - (b.key === 'unknown') || b.count - a.count || a.name.localeCompare(b.name));
  };
  const gpu = new Map();
  for (const s of systems) for (const g of s.gpu || ['unknown']) gpu.set(g, (gpu.get(g) || 0) + 1);
  out.system = {
    reported: systems.length,
    distro: breakdown(s => s.distro === null ? null : [s.distro, s.distro_version].filter(Boolean).join(' '), distroName),
    cosmic: breakdown(s => s.cosmic),
    gpu: [...gpu].map(([key, count]) => ({key, name: key === 'unknown' ? 'Not reported' : GPUS[key], count}))
      .sort((a, b) => (a.key === 'unknown') - (b.key === 'unknown') || b.count - a.count),
    displays: breakdown(s => s.displays === null ? null : String(s.displays), n => n === '1' ? '1 display' : `${n} displays`),
    resolution: breakdown(s => s.resolution, r => r.replace('x', ' × ')),
  };
  return out;
}

const DISTROS = {pop: 'Pop!_OS', ubuntu: 'Ubuntu', fedora: 'Fedora', debian: 'Debian', arch: 'Arch Linux',
  nixos: 'NixOS', opensuse: 'openSUSE', 'opensuse-tumbleweed': 'openSUSE Tumbleweed', manjaro: 'Manjaro',
  endeavouros: 'EndeavourOS', linuxmint: 'Linux Mint', zorin: 'Zorin OS', neon: 'KDE neon', elementary: 'elementary OS',
  serpent: 'Serpent OS', cachyos: 'CachyOS'};
function distroName(value) {
  const [id, version] = value.split(' ');
  return [DISTROS[id] || id, version].filter(Boolean).join(' ');
}
