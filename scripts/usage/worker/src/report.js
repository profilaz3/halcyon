// Report validation and ingestion. Semantics match the former Apps Script
// collector (scripts/usage/collector.gs) so 0.2.0 and later clients keep
// working unchanged: same schemas, same acknowledgements, same ordering rules.

export const MAX_INSTALLATIONS = 100000;
export const RETENTION_DAYS = 90;
export const MAX_BODY = 1024;

export const validCountry = v => typeof v === 'string' && /^[A-Z]{2}$/.test(v);
export const validVersion = v => typeof v === 'string' && v.length <= 48
  && /^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/.test(v);
const validTime = t => Number.isSafeInteger(t) && t >= 0 && t <= 253402300799;

export function validInstallation(format, channel, installed) {
  if (!['deb', 'rpm', 'source', 'unknown'].includes(format)
      || !['apt', 'dnf', 'manual', 'disabled', 'source', 'unknown'].includes(channel)
      || !(installed === null || validVersion(installed))) return false;
  if (format === 'source') return channel === 'source' && installed === null;
  if (format === 'unknown') return channel === 'unknown' && installed === null;
  return ['manual', 'disabled', 'unknown', format === 'deb' ? 'apt' : 'dnf'].includes(channel);
}

// Schema 6 feature snapshot: fixed keys, fixed values, null when unknown.
export const WIDGETS = ['calendar', 'projects', 'devbox', 'weather', 'clock', 'glance', 'music', 'notes', 'focus'];
export const FEATURE_VALUES = {
  wallpaper_effect: ['off', 'rain', 'snow', 'fog', 'clouds'],
  activity_island: 'bool', penumbra: 'bool', islands: 'bool',
  widgets: 'widgets',
  visualizer: 'bool', wallpaper_depth: 'bool',
  theme_style: ['default', 'soft', 'outline', 'paper', 'bare', 'glass', 'frosted', 'midnight', 'terminal', 'neon',
    'editorial', 'retro', 'pastel', 'bauhaus'],
  automation: 'bool',
};
const FEATURE_KEYS = Object.keys(FEATURE_VALUES);

// The canonical stored form, or null when the snapshot is malformed.
export function canonicalFeatures(f) {
  if (!f || typeof f !== 'object' || Array.isArray(f)) return null;
  if (Object.keys(f).sort().join(',') !== [...FEATURE_KEYS].sort().join(',')) return null;
  const out = {};
  for (const key of FEATURE_KEYS) {
    const kind = FEATURE_VALUES[key], v = f[key];
    if (v === null) { out[key] = null; continue; }
    if (kind === 'bool') { if (typeof v !== 'boolean') return null; out[key] = v; continue; }
    if (kind === 'widgets') {
      if (!Array.isArray(v) || new Set(v).size !== v.length || !v.every(w => WIDGETS.includes(w))) return null;
      out[key] = WIDGETS.filter(w => v.includes(w));
      continue;
    }
    if (!kind.includes(v)) return null;
    out[key] = v;
  }
  return JSON.stringify(out);
}

export function validateReport(body) {
  if (typeof body !== 'string' || body.length > MAX_BODY) throw Error('Invalid report');
  const v = JSON.parse(body);
  if (!v || typeof v !== 'object' || Array.isArray(v) || typeof v.installation !== 'string'
      || !/^[a-f0-9]{64}$/.test(v.installation)) throw Error('Invalid report');
  const keys = Object.keys(v).sort().join(',');
  const country = v.country === null || validCountry(v.country);
  const revision = Number.isSafeInteger(v.revision) && v.revision >= 0;
  const legacy = v.schema === 1 && keys === 'installation,schema,version' && validVersion(v.version)
    || v.schema === 2 && keys === 'country,installation,schema,version' && validVersion(v.version) && country;
  const activity = v.schema === 3 && v.kind === 'activity'
    && keys === 'country,installation,kind,revision,schema,version' && revision && validVersion(v.version) && country;
  const detailed = v.schema === 4 && v.kind === 'activity'
    && keys === 'country,first_started_at,installation,kind,revision,schema,version'
    && revision && validVersion(v.version) && country
    && (v.first_started_at === null || validTime(v.first_started_at));
  const channel = v.schema === 5 && v.kind === 'activity'
    && keys === 'country,first_started_at,installation,installed_app_version,kind,package_format,revision,schema,update_channel,version'
    && revision && validVersion(v.version) && country
    && (v.first_started_at === null || validTime(v.first_started_at))
    && validInstallation(v.package_format, v.update_channel, v.installed_app_version);
  const features = v.schema === 6 && v.kind === 'activity'
    && keys === 'country,features,first_started_at,installation,installed_app_version,kind,package_format,revision,schema,update_channel,version'
    && revision && validVersion(v.version) && country
    && (v.first_started_at === null || validTime(v.first_started_at))
    && validInstallation(v.package_format, v.update_channel, v.installed_app_version)
    && canonicalFeatures(v.features) !== null;
  // Client action time is informational, never a receipt or retention clock.
  const status = v.schema === 3 && v.kind === 'status'
    && keys === 'changed_at,enabled,installation,kind,revision,schema'
    && revision && v.revision > 0 && typeof v.enabled === 'boolean' && validTime(v.changed_at);
  if (!(legacy || activity || detailed || channel || features || status)) throw Error('Invalid report');
  if (legacy) v.kind = 'activity';
  return v;
}

export const dayOf = date => Math.floor(date.getTime() / 86400000);
export const isoDay = day => new Date(day * 86400000).toISOString().slice(0, 10);

export async function installationId(token) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token));
  return [...new Uint8Array(digest)].map(b => b.toString(16).padStart(2, '0')).join('');
}

const statusName = enabled => enabled ? 'enabled' : 'opted_out';

// One read, one guarded write batch. The guard is the row state that was read,
// so a concurrent report for the same installation makes the batch a no-op and
// the decision is taken again from fresh state.
export async function applyReport(db, id, report, now) {
  for (let attempt = 0; attempt < 4; attempt++) {
    const row = await db.prepare('SELECT * FROM installations WHERE id = ?').bind(id).first();
    const plan = report.kind === 'status' ? planStatus(db, row, id, report, now) : planActivity(db, row, id, report, now);
    if (!plan.statements.length) return plan.ack;
    const results = await db.batch(plan.statements);
    if (results[0].meta.changes === 1) return plan.ack;
    if (!row) {
      const exists = await db.prepare('SELECT 1 AS present FROM installations WHERE id = ?').bind(id).first();
      if (!exists) throw Error('Capacity reached');
    }
  }
  throw Error('Busy');
}

function capacityGuard() {
  return `(SELECT COUNT(*) FROM installations) < ${MAX_INSTALLATIONS}`;
}

function planStatus(db, row, id, report, now) {
  const ack = {ok: true, revision: report.revision, enabled: report.enabled};
  const choice = new Date(report.changed_at * 1000).toISOString();
  const status = statusName(report.enabled);
  if (row && report.revision < row.revision)
    return {statements: [], ack: {ok: true, revision: row.revision, enabled: row.status === 'enabled'}};
  if (row && report.revision === row.revision) {
    // Exact retries are no-ops; neither receipt nor retention is refreshed.
    if (row.status !== status || row.choice_at && row.choice_at !== choice) throw Error('Revision conflict');
    return {statements: [], ack};
  }
  const received = now.toISOString();
  const statements = [];
  if (!row) {
    statements.push(db.prepare(`INSERT INTO installations (id, status, revision, choice_at, opted_out_at, status_received_at)
      SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE ${capacityGuard()} ON CONFLICT (id) DO NOTHING`)
      .bind(id, status, report.revision, choice, report.enabled ? null : choice, received));
  } else {
    statements.push(db.prepare(`UPDATE installations SET status = ?1, revision = ?2, choice_at = ?3,
        opted_out_at = CASE WHEN ?1 = 'opted_out' THEN ?3 ELSE opted_out_at END, status_received_at = ?4
      WHERE id = ?5 AND revision = ?6 AND status = ?7`)
      .bind(status, report.revision, choice, received, id, row.revision, row.status));
  }
  // Turning sharing off also removes that installation's daily activity.
  if (!report.enabled) {
    statements.push(db.prepare(`DELETE FROM activity WHERE id = ?1 AND EXISTS
      (SELECT 1 FROM installations WHERE id = ?1 AND revision = ?2 AND status = 'opted_out')`)
      .bind(id, report.revision));
  }
  return {statements, ack};
}

function planActivity(db, row, id, report, now) {
  const today = dayOf(now);
  const ack = {ok: true, day: isoDay(today)};
  // Old clients may continue enabled activity but never change preference state.
  if (row && (row.status !== 'enabled' || report.schema >= 3 && report.revision !== row.revision))
    return {statements: [], ack};
  const received = now.toISOString();
  const country = validCountry(report.country) ? report.country : null;
  const started = report.schema >= 4 && report.first_started_at !== null
    ? new Date(report.first_started_at * 1000).toISOString() : null;
  const revision = row ? row.revision : (report.revision || 0);
  const statements = [];
  if (!row) {
    statements.push(db.prepare(`INSERT INTO installations (id, status, revision, country, first_seen_at,
        first_seen_day, first_started_at, first_version, last_active_day, last_version, last_received_at)
      SELECT ?1, 'enabled', ?2, ?3, ?4, ?5, ?6, ?7, ?5, ?7, ?4 WHERE ${capacityGuard()}
      ON CONFLICT (id) DO NOTHING`)
      .bind(id, revision, country, received, today, started, report.version));
  } else {
    statements.push(db.prepare(`UPDATE installations SET last_active_day = ?1, last_version = ?2,
        country = COALESCE(country, ?3),
        first_version = CASE WHEN first_seen_at IS NULL THEN ?2 ELSE COALESCE(first_version, last_version, ?2) END,
        first_seen_day = CASE WHEN first_seen_at IS NULL THEN ?1 ELSE first_seen_day END,
        first_seen_at = COALESCE(first_seen_at, ?4),
        first_started_at = COALESCE(first_started_at, ?5), last_received_at = ?4
      WHERE id = ?6 AND status = 'enabled' AND revision = ?7`)
      .bind(today, report.version, country, received, started, id, row.revision));
  }
  const detailed = report.schema >= 5;
  statements.push(db.prepare(`INSERT INTO activity (id, day, version, package_format, update_channel, installed_version, received_at, features)
      SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?9
      WHERE EXISTS (SELECT 1 FROM installations WHERE id = ?1 AND status = 'enabled' AND revision = ?8 AND last_received_at = ?7)
      ON CONFLICT (id, day) DO UPDATE SET version = excluded.version, package_format = excluded.package_format,
        update_channel = excluded.update_channel, installed_version = excluded.installed_version,
        received_at = excluded.received_at, features = excluded.features`)
    .bind(id, today, report.version, detailed ? report.package_format : 'unknown',
      detailed ? report.update_channel : 'unknown', detailed ? report.installed_app_version : null,
      received, revision, report.schema === 6 ? canonicalFeatures(report.features) : null));
  return {statements, ack};
}

// Identifiable rows expire as before: opted-out rows 90 days after the status
// receipt; enabled rows 90 days after their latest activity or choice.
export async function applyRetention(db, now) {
  const cutoff = dayOf(now) - RETENTION_DAYS + 1;
  const cutoffIso = new Date(cutoff * 86400000).toISOString();
  await db.batch([
    db.prepare('DELETE FROM activity WHERE day < ?').bind(cutoff),
    db.prepare(`DELETE FROM installations WHERE status = 'opted_out'
      AND (status_received_at IS NULL OR status_received_at < ?)`).bind(cutoffIso),
    db.prepare(`DELETE FROM installations WHERE status = 'enabled' AND COALESCE(last_active_day, -1) < ?1
      AND (status_received_at IS NULL OR status_received_at < ?2)`).bind(cutoff, cutoffIso),
    db.prepare(`UPDATE installations SET opted_out_at = NULL, status_received_at = NULL, choice_at = NULL
      WHERE status = 'enabled' AND status_received_at < ?`).bind(cutoffIso),
    db.prepare(`DELETE FROM activity WHERE id IN (SELECT id FROM installations WHERE status = 'opted_out')`),
  ]);
}
