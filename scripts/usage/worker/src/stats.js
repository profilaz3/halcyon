// Daily rollups and the public summary. Everything published is an aggregate:
// no installation identifier, hash or per-installation row leaves the database.
// Counts are exact, however small.

import {RETENTION_DAYS, WIDGETS, applyRetention, dayOf, isoDay} from './report.js';

export const FORMATS = ['deb', 'rpm', 'source', 'unknown'];
export const SEGMENT_NAMES = {
  all: 'All installations', deb: 'DEB package', rpm: 'RPM package',
  source: 'Built from source', unknown: 'Not reported',
};
const MAX_HISTORY_DAYS = 365;
// The dashboard counts reports sent straight here, which began with this
// release. Older versions keep reporting to the former Google Sheet, and any
// of their rows imported or retained here are left out of the statistics.
export const DIRECT_SINCE = '0.6.1';
const isDirect = (version, since) => typeof version === 'string' && compareVersions(version, since) >= 0;
const COHORT_WEEKS = 10;

export function compareVersions(a, b) {
  const parse = v => {
    const [core, pre] = String(v).split('-', 2);
    return {nums: core.split('.').map(Number), pre};
  };
  const x = parse(a), y = parse(b);
  for (let i = 0; i < 3; i++) if (x.nums[i] !== y.nums[i]) return x.nums[i] - y.nums[i];
  if (x.pre === y.pre) return 0;
  if (x.pre === undefined) return 1;
  if (y.pre === undefined) return -1;
  return x.pre < y.pre ? -1 : 1;
}

const weekOf = day => Math.floor((day + 3) / 7); // Monday-start UTC weeks
const weekStart = week => week * 7 - 3;

// Retained activity joined with the few installation fields the summary needs.
// rowid stands in for the installation inside one query; it is never published.
export async function loadActivity(db, fromDay) {
  const {results} = await db.prepare(`SELECT i.rowid AS n, a.day AS day, a.version AS version,
      a.package_format AS format, a.update_channel AS channel, a.features AS features,
      i.first_seen_day AS first, i.country AS country
    FROM activity a JOIN installations i ON i.id = a.id
    WHERE a.day >= ? AND i.status = 'enabled' ORDER BY a.day`).bind(fromDay).all();
  return results;
}

export function rollupDay(rows, day) {
  const out = [];
  const segments = new Map([['all', []], ...FORMATS.map(f => [f, []])]);
  for (const r of rows) if (r.day === day) { segments.get('all').push(r); segments.get(r.format)?.push(r); }
  for (const [segment, list] of segments) {
    if (segment !== 'all' && !list.length) continue;
    out.push({day, segment, metric: 'active', key: '', value: list.length});
    out.push({day, segment, metric: 'new', key: '', value: list.filter(r => r.first === day).length});
    const versions = new Map();
    for (const r of list) versions.set(r.version || 'unknown', (versions.get(r.version || 'unknown') || 0) + 1);
    for (const [key, value] of versions) out.push({day, segment, metric: 'version', key, value});
  }
  // Feature snapshots, all install methods: how many reported each feature and
  // how many had each value. Frozen with the other daily totals.
  const known = new Map(), values = new Map();
  const add = (map, key) => map.set(key, (map.get(key) || 0) + 1);
  for (const r of segments.get('all')) {
    const f = parseFeatures(r.features);
    if (!f) continue;
    for (const [key, v] of Object.entries(f)) {
      if (v === null || v === undefined) continue;
      add(known, key);
      if (Array.isArray(v)) v.forEach(w => add(values, `${key}=${w}`));
      else if (v === true) add(values, `${key}=on`);
      else if (typeof v === 'string') add(values, `${key}=${v}`);
    }
  }
  for (const [key, value] of known) out.push({day, segment: 'all', metric: 'feature_known', key, value});
  for (const [key, value] of values) out.push({day, segment: 'all', metric: 'feature', key, value});
  return out;
}

export const parseFeatures = text => {
  if (!text) return null;
  try { return JSON.parse(text); } catch (_) { return null; }
};

export const FEATURE_NAMES = {
  activity_island: 'Activity island', penumbra: 'Penumbra', islands: 'Islands',
  wallpaper_effect: 'Wallpaper effects', visualizer: 'Audio visualizer', wallpaper_depth: 'Wallpaper depth',
  automation: 'Automation',
};
export const WIDGET_NAMES = {
  calendar: 'Calendar', projects: 'Projects', devbox: 'System monitor', weather: 'Weather', clock: 'Clock',
  glance: 'Daily overview', music: 'Music', notes: 'Notes', focus: 'Focus',
};
const VALUE_NAMES = {
  off: 'Off', rain: 'Rain', snow: 'Snow', fog: 'Fog', clouds: 'Cloud shadows', default: 'Default',
};
const valueName = v => VALUE_NAMES[v] || v.charAt(0).toUpperCase() + v.slice(1);

// Latest settings of each installation active in the window.
export function featureSummary(rows, from, to) {
  const latest = new Map();
  for (const r of rows) if (r.day >= from && r.day <= to && r.features) latest.set(r.n, parseFeatures(r.features));
  const snapshots = [...latest.values()].filter(Boolean);
  const out = {reported: snapshots.length, switches: null, widgets: null, wallpaper_effect: null, theme_style: null};
  if (!snapshots.length) return out;
  const share = (key, test) => {
    const known = snapshots.filter(f => f[key] !== null && f[key] !== undefined);
    return {known: known.length, on: known.filter(test).length};
  };
  out.switches = Object.keys(FEATURE_NAMES).map(key => ({key, name: FEATURE_NAMES[key],
    ...share(key, f => key === 'wallpaper_effect' ? f[key] !== 'off' : f[key] === true)}));
  const widgetKnown = snapshots.filter(f => Array.isArray(f.widgets));
  out.widgets = {known: widgetKnown.length, items: WIDGETS.map(w => ({key: w, name: WIDGET_NAMES[w],
    on: widgetKnown.filter(f => f.widgets.includes(w)).length}))};
  const breakdown = key => {
    const counts = new Map();
    for (const f of snapshots) if (typeof f[key] === 'string') counts.set(f[key], (counts.get(f[key]) || 0) + 1);
    return counts.size ? fold(counts, valueName) : null;
  };
  out.wallpaper_effect = breakdown('wallpaper_effect');
  out.theme_style = breakdown('theme_style');
  return out;
}

async function writeDaily(db, days, rows) {
  const statements = [db.prepare(`DELETE FROM daily WHERE day IN (SELECT value FROM json_each(?))`).bind(JSON.stringify(days))];
  for (let i = 0; i < rows.length; i += 2000) {
    const chunk = rows.slice(i, i + 2000).map(r => [r.day, r.segment, r.metric, r.key, r.value]);
    statements.push(db.prepare(`INSERT INTO daily (day, segment, metric, key, value)
      SELECT json_extract(value, '$[0]'), json_extract(value, '$[1]'), json_extract(value, '$[2]'),
        json_extract(value, '$[3]'), json_extract(value, '$[4]') FROM json_each(?)`).bind(JSON.stringify(chunk)));
  }
  await db.batch(statements);
}

async function getMeta(db, key) {
  const row = await db.prepare('SELECT value FROM meta WHERE key = ?').bind(key).first();
  return row ? row.value : null;
}
const setMeta = (db, key, value) => db.prepare(`INSERT INTO meta (key, value) VALUES (?1, ?2)
  ON CONFLICT (key) DO UPDATE SET value = excluded.value`).bind(key, value);

export async function refreshReleases(db, fetchImpl, today) {
  if (await getMeta(db, 'releases_checked') === String(today)) return;
  try {
    const response = await fetchImpl('https://api.github.com/repos/profilaz3/halcyon/releases?per_page=30', {
      headers: {'User-Agent': 'halcyon-reports', 'Accept': 'application/vnd.github+json'},
    });
    if (!response.ok) return;
    const list = await response.json();
    const rows = list.filter(r => !r.draft && !r.prerelease && /^v?\d+\.\d+\.\d+$/.test(r.tag_name) && r.published_at)
      .map(r => [r.tag_name.replace(/^v/, ''), dayOf(new Date(r.published_at))]);
    await db.batch([
      db.prepare(`INSERT INTO releases (version, published_day)
        SELECT json_extract(value, '$[0]'), json_extract(value, '$[1]') FROM json_each(?) WHERE true
        ON CONFLICT (version) DO UPDATE SET published_day = excluded.published_day`).bind(JSON.stringify(rows)),
      setMeta(db, 'releases_checked', String(today)),
    ]);
  } catch (_) {
    // Release markers are decoration; a GitHub outage must not stop rollups.
  }
}

// Hourly: retention, rollups of completed days, then the public summary.
export async function maintain(db, now, {fetchImpl = fetch, since = DIRECT_SINCE} = {}) {
  const today = dayOf(now), oldest = today - RETENTION_DAYS + 1;
  await applyRetention(db, now);
  await refreshReleases(db, fetchImpl, today);
  // Nothing before the first direct release counts, even a pre-release build.
  const released = await db.prepare('SELECT published_day FROM releases WHERE version = ?').bind(since).first('published_day');
  const floor = Math.max(oldest, released ?? oldest);
  const rows = (await loadActivity(db, floor)).filter(r => isDirect(r.version, since));
  // A different scope invalidates every frozen day: rebuild them all from
  // retained activity, and drop totals older than that.
  const rescoped = await getMeta(db, 'scope') !== since;
  if (rescoped) await db.batch([db.prepare('DELETE FROM daily'), db.prepare(`DELETE FROM meta WHERE key = 'rolled_through'`)]);
  const rolled = await getMeta(db, 'rolled_through');
  const from = Math.max(rolled === null ? floor : Number(rolled) + 1, floor);
  if (from <= today - 1) {
    const days = [];
    for (let d = from; d <= today - 1; d++) days.push(d);
    await writeDaily(db, days, days.flatMap(d => rollupDay(rows, d)));
    await setMeta(db, 'rolled_through', String(today - 1)).run();
  }
  if (rescoped) await setMeta(db, 'scope', since).run();
  const [{results: daily}, {results: releases}, {results: statuses}] = await db.batch([
    db.prepare('SELECT day, segment, metric, key, value FROM daily WHERE day >= ? ORDER BY day').bind(today - MAX_HISTORY_DAYS),
    db.prepare('SELECT version, published_day FROM releases'),
    db.prepare('SELECT status, last_version AS version, COUNT(*) AS count FROM installations GROUP BY status, last_version'),
  ]);
  // Choices of installations last seen on an older version stay out too;
  // one that turned sharing off before ever reporting has no version and counts.
  const direct = statuses.filter(s => s.version === null || isDirect(s.version, since));
  const summary = buildSummary({rows, daily, releases, statuses: direct, now, since});
  await setMeta(db, 'summary', JSON.stringify(summary)).run();
  return summary;
}

// Every category with its exact count, largest first; unknown values last.
function fold(counts, labels = k => k) {
  const last = key => key === 'unknown' || key === null || key === undefined;
  return [...counts].map(([key, count]) => ({key: last(key) ? 'unknown' : key, name: labels(key), count}))
    .sort((a, b) => last(a.key) - last(b.key) || b.count - a.count || a.name.localeCompare(b.name));
}

const COUNTRY_NAMES = (() => {
  try { return new Intl.DisplayNames(['en'], {type: 'region'}); } catch (_) { return null; }
})();
const countryName = code => code === 'unknown' ? 'Unknown' : (COUNTRY_NAMES?.of(code) || code);

export function buildSummary({rows, daily, releases, statuses, now, since = DIRECT_SINCE}) {
  const today = dayOf(now), yesterday = today - 1, oldest = today - RETENTION_DAYS + 1;
  const activeDays = [...new Set(daily.filter(r => r.segment === 'all' && r.metric === 'active' && r.value > 0).map(r => r.day))];
  const historyStart = activeDays.length ? Math.min(...activeDays) : null;
  const days = [];
  if (historyStart !== null) for (let d = Math.max(historyStart, today - MAX_HISTORY_DAYS); d <= yesterday; d++) days.push(d);
  const dayIndex = new Map(days.map((d, i) => [d, i]));
  // First day covered by complete raw activity: windows before it are unknown.
  const rawStart = Math.max(oldest, historyStart ?? today);

  const latestFormat = new Map();
  for (const r of rows) latestFormat.set(r.n, r.format);
  const inSegment = (segment, r) => segment === 'all' || r.format === segment;
  const installsIn = (segment, from, to) => {
    const ids = new Set();
    for (const r of rows) if (r.day >= from && r.day <= to && (segment === 'all' || latestFormat.get(r.n) === segment)) ids.add(r.n);
    return ids;
  };

  const releaseList = releases.map(r => ({version: r.version, day: r.published_day}))
    .sort((a, b) => compareVersions(b.version, a.version));
  const latestRelease = releaseList[0] || null;

  // Install method over the last 28 days decides which filters are offered.
  const recent = installsIn('all', today - 27, today);
  const formatCounts = new Map();
  for (const n of recent) formatCounts.set(latestFormat.get(n), (formatCounts.get(latestFormat.get(n)) || 0) + 1);
  const segments = ['all', ...FORMATS.filter(f => formatCounts.get(f))];

  const bySegment = {};
  for (const segment of segments) {
    const series = metric => {
      const values = days.map(() => 0);
      for (const r of daily) if (r.segment === segment && r.metric === metric && dayIndex.has(r.day)) values[dayIndex.get(r.day)] += r.value;
      return values;
    };
    const versionDays = new Map();
    for (const r of daily) {
      if (r.segment !== segment || r.metric !== 'version' || !dayIndex.has(r.day)) continue;
      if (!versionDays.has(r.key)) versionDays.set(r.key, days.map(() => 0));
      versionDays.get(r.key)[dayIndex.get(r.day)] += r.value;
    }
    const versions = new Map(), other = days.map(() => 0);
    for (const [key, values] of versionDays) {
      if (key !== 'unknown') versions.set(key, values);
      else values.forEach((v, i) => { other[i] += v; });
    }
    const names = [...versions.keys()].sort((a, b) => compareVersions(b, a));
    if (other.some(Boolean)) { names.push('Not reported'); versions.set('Not reported', other); }

    const windows = {};
    for (const size of [7, 30, 90]) {
      const from = today - size + 1, prevFrom = today - 2 * size + 1;
      const current = installsIn(segment, from, today);
      const fresh = n => [...n].length;
      const newIn = (a, b) => {
        const ids = new Set();
        for (const r of rows) if (r.first !== null && r.first >= a && r.first <= b && r.day === r.first && (segment === 'all' || latestFormat.get(r.n) === segment)) ids.add(r.n);
        return ids.size;
      };
      const complete = from >= rawStart, prevComplete = prevFrom >= rawStart;
      windows[size] = {
        active: fresh(current), complete,
        previous: prevComplete ? installsIn(segment, prevFrom, from - 1).size : null,
        new: newIn(from, today), previous_new: prevComplete ? newIn(prevFrom, from - 1) : null,
      };
    }

    const window28 = installsIn(segment, today - 27, today);
    const activeDayCount = new Map();
    for (const r of rows) if (r.day >= today - 27 && window28.has(r.n)) activeDayCount.set(r.n, (activeDayCount.get(r.n) || 0) + 1);
    const buckets = [['22 to 28 days', 22, 28], ['15 to 21 days', 15, 21], ['8 to 14 days', 8, 14], ['4 to 7 days', 4, 7], ['1 to 3 days', 1, 3]];
    const frequency = buckets.map(([name, lo, hi]) => ({name, count: [...activeDayCount.values()].filter(c => c >= lo && c <= hi).length}));

    const country = new Map(), method = new Map(), channels = new Map(), lastVersion = new Map();
    for (const r of rows) {
      if (!window28.has(r.n)) continue;
      country.set(r.n, r.country || 'unknown');
      if (r.day >= today - 6) lastVersion.set(r.n, r.version);
      channels.set(r.n, [r.format, r.channel]);
    }
    const tally = map => { const t = new Map(); for (const v of map.values()) t.set(v, (t.get(v) || 0) + 1); return t; };
    for (const n of window28) method.set(n, latestFormat.get(n));
    let updates = null;
    const packaged = [...channels.values()].filter(([f]) => f === 'deb' || f === 'rpm');
    if (packaged.length) {
      const on = packaged.filter(([, c]) => c === 'apt' || c === 'dnf').length;
      updates = {total: packaged.length, on, other: packaged.length - on};
    }
    // Only installations on DIRECT_SINCE or newer are counted, so until a newer
    // release is out every one of them is on the latest by definition.
    let latestShare = null;
    if (latestRelease && compareVersions(latestRelease.version, since) > 0 && lastVersion.size) {
      const known = [...lastVersion.values()].filter(Boolean);
      if (known.length) latestShare = known.filter(v => compareVersions(v, latestRelease.version) >= 0).length / known.length;
    }

    bySegment[segment] = {
      active: series('active'), new: series('new'),
      versions: {names, counts: Object.fromEntries(names.map(n => [n, versions.get(n)]))},
      windows,
      today: [...installsIn(segment, today, today)].length,
      yesterday: days.length ? series('active')[days.length - 1] : null,
      week_earlier: dayIndex.has(yesterday - 7) ? series('active')[dayIndex.get(yesterday - 7)] : null,
      latest_share: latestShare,
      installs_28d: window28.size,
      install_methods: fold(tally(method), k => SEGMENT_NAMES[k] || SEGMENT_NAMES.unknown),
      updates,
      frequency,
      countries: fold(tally(country), countryName),
    };
  }

  // Days until half the installations reporting a version ran that release or newer.
  // Releases before DIRECT_SINCE were never counted here. DIRECT_SINCE itself is
  // the baseline: every counted installation runs it or newer from the start.
  const allVersions = new Map();
  for (const r of daily) if (r.segment === 'all' && r.metric === 'version' && r.key !== 'unknown') {
    if (!allVersions.has(r.day)) allVersions.set(r.day, []);
    allVersions.get(r.day).push(r);
  }
  const releaseStats = releaseList.filter(r => historyStart !== null && r.day >= historyStart && r.day <= today
    && compareVersions(r.version, since) >= 0).slice(0, 4).map(r => {
    const baseline = compareVersions(r.version, since) === 0;
    let reached = null;
    for (let d = r.day; d <= yesterday && reached === null && !baseline; d++) {
      const list = allVersions.get(d) || [];
      const total = list.reduce((n, x) => n + x.value, 0);
      const current = list.filter(x => compareVersions(x.key, r.version) >= 0).reduce((n, x) => n + x.value, 0);
      if (total && current / total >= 0.5) reached = d - r.day;
    }
    return {version: r.version, date: isoDay(r.day), days_to_half: reached, baseline};
  });

  // Weekly cohorts by first report, among installations still sharing data.
  const thisWeek = weekOf(today);
  const cohortMembers = new Map(), cohortActive = new Map();
  for (const r of rows) {
    if (r.first === null || r.first < rawStart || weekStart(weekOf(r.first)) < rawStart) continue;
    const c = weekOf(r.first);
    if (!cohortMembers.has(c)) cohortMembers.set(c, new Set());
    cohortMembers.get(c).add(r.n);
    const key = c + ':' + (weekOf(r.day) - c);
    if (!cohortActive.has(key)) cohortActive.set(key, new Set());
    cohortActive.get(key).add(r.n);
  }
  const cohorts = [...cohortMembers.keys()].filter(c => c < thisWeek).sort((a, b) => a - b).slice(-COHORT_WEEKS)
    .map(c => {
      const size = cohortMembers.get(c).size;
      const rates = [];
      for (let k = 0; c + k < thisWeek; k++) rates.push((cohortActive.get(c + ':' + k)?.size || 0) / size);
      return {start: isoDay(weekStart(c)), size, rates};
    });
  const week4 = cohorts.filter(c => c.rates.length > 4);
  const retention4w = week4.length
    ? week4.reduce((n, c) => n + c.rates[4] * c.size, 0) / week4.reduce((n, c) => n + c.size, 0) : null;

  const statusCount = {};
  for (const s of statuses) statusCount[s.status] = (statusCount[s.status] || 0) + s.count;
  const retained = (statusCount.enabled || 0) + (statusCount.opted_out || 0);
  return {
    schema: 1,
    generated_at: now.toISOString(),
    today: isoDay(today),
    history_start: historyStart === null ? null : isoDay(historyStart),
    since_version: since,
    days: days.map(isoDay),
    segments: segments.map(id => ({id, name: SEGMENT_NAMES[id]})),
    latest_release: latestRelease && {version: latestRelease.version, date: isoDay(latestRelease.day)},
    releases: releaseStats,
    by_segment: bySegment,
    cohorts,
    retention_4w: retention4w,
    opt_out_share: retained ? (statusCount.opted_out || 0) / retained : null,
    sharing: {on: statusCount.enabled || 0, off: statusCount.opted_out || 0},
    features: featureSummary(rows, today - 27, today),
  };
}
