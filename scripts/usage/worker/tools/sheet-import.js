// One-time import of the former Google Sheet collector ("Installations" tab,
// exported as CSV) into D1. Pure functions; the CLI is import-sheet.mjs.
// Merge rules match the collector's duplicate handling: the higher preference
// revision wins, an opt-out wins an equal revision, and activity days merge.

import {RETENTION_DAYS, validCountry, validInstallation, validVersion} from '../src/report.js';

const SERIAL_EPOCH = 25569; // Google Sheets day serial of 1970-01-01

export function parseCsv(text) {
  const rows = [];
  let row = [], field = '', quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') { row.push(field); field = ''; }
    else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++;
      row.push(field); rows.push(row); row = []; field = '';
    } else field += c;
  }
  if (field !== '' || row.length) { row.push(field); rows.push(row); }
  return rows;
}

// A day cell may be a Sheets serial number or a displayed date.
export function parseDay(value) {
  const v = String(value ?? '').trim();
  if (!v) return null;
  if (/^\d{5}$/.test(v)) return Number(v) - SERIAL_EPOCH;
  const iso = v.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (iso) return Math.floor(Date.UTC(+iso[1], +iso[2] - 1, +iso[3]) / 86400000);
  // Sheets display format, for example "5 Nov 2026".
  const shown = v.match(/^(\d{1,2}) (Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) (\d{4})$/);
  if (shown) {
    const month = 'JanFebMarAprMayJunJulAugSepOctNovDec'.indexOf(shown[2]) / 3;
    return Math.floor(Date.UTC(+shown[3], month, +shown[1]) / 86400000);
  }
  throw Error('Unrecognized day value');
}

// Timestamps were written as ISO strings; Sheets may display them as
// "YYYY-MM-DD HH:MM:SS" in the workbook time zone (UTC for this workbook).
export function parseTime(value) {
  const v = String(value ?? '').trim();
  if (!v) return null;
  const m = v.match(/^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,3}))?Z?$/);
  if (!m) throw Error('Unrecognized timestamp');
  const date = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6], +(m[7] || '0').padEnd(3, '0')));
  if (!Number.isFinite(date.getTime())) throw Error('Unrecognized timestamp');
  return date.toISOString();
}

const dayOfIso = iso => iso === null ? null : Math.floor(new Date(iso).getTime() / 86400000);
const jsonCell = value => String(value ?? '').trim() ? JSON.parse(value) : [];

export function sheetRecord(cells, today) {
  const id = String(cells[0] ?? '').trim();
  if (!/^[a-f0-9]{64}$/.test(id)) return null;
  const status = String(cells[4] || 'enabled').trim();
  if (!['enabled', 'opted_out'].includes(status)) throw Error('Invalid status');
  const revision = String(cells[7] ?? '').trim() === '' ? 0 : Number(cells[7]);
  if (!Number.isSafeInteger(revision) || revision < 0) throw Error('Invalid revision');
  const version = validVersion(String(cells[2] ?? '').trim()) ? String(cells[2]).trim() : null;
  const firstVersion = validVersion(String(cells[11] ?? '').trim()) ? String(cells[11]).trim() : null;
  const firstSeen = parseTime(cells[9]);
  const lastActive = parseDay(cells[1]);
  const record = {
    id, status, revision,
    opted_out_at: parseTime(cells[5]), status_received_at: parseTime(cells[6]), choice_at: parseTime(cells[8]),
    country: validCountry(String(cells[3] ?? '').trim()) ? String(cells[3]).trim() : null,
    first_seen_at: firstSeen, first_seen_day: dayOfIso(firstSeen), first_started_at: parseTime(cells[10]),
    first_version: firstSeen ? firstVersion || version : firstVersion,
    last_active_day: lastActive, last_version: version, last_received_at: parseTime(cells[14]),
  };
  const oldest = today - RETENTION_DAYS + 1;
  const days = new Set(jsonCell(cells[13]).map(d => d - SERIAL_EPOCH));
  if (lastActive !== null) days.add(lastActive); // a last-active date is an observed activity day
  const versions = jsonCell(cells[15]).map(([d, v]) => [d - SERIAL_EPOCH, v]).sort((a, b) => a[0] - b[0]);
  const observations = new Map(jsonCell(cells[16]).map(o => [o[0] - SERIAL_EPOCH, o]));
  const versionOn = day => {
    let v = null;
    for (const [d, value] of versions) if (d <= day) v = value;
    return v ?? (day === lastActive ? version : null);
  };
  record.activity = status === 'opted_out' ? [] : [...days].filter(d => d >= oldest && d <= today).sort((a, b) => a - b).map(day => {
    const o = observations.get(day);
    if (o && validInstallation(o[1], o[2], o[3] || null) && validVersion(o[4]))
      return {day, version: o[4], package_format: o[1], update_channel: o[2], installed_version: o[3] || null, received_at: parseTime(o[5])};
    const v = versionOn(day);
    return {day, version: validVersion(v) ? v : null, package_format: 'unknown', update_channel: 'unknown', installed_version: null, received_at: null};
  });
  return record;
}

const minOf = (a, b) => a === null ? b : b === null ? a : (a < b ? a : b);
const maxOf = (a, b) => a === null ? b : b === null ? a : (a > b ? a : b);

// Merge a sheet record with the row already in D1 (if any).
export function mergeRecords(sheet, current) {
  if (!current) return sheet;
  const sheetWins = sheet.revision > current.revision
    || sheet.revision === current.revision && sheet.status === 'opted_out' && current.status !== 'opted_out';
  const choice = sheetWins ? sheet : current, other = sheetWins ? current : sheet;
  const earliest = minOf(sheet.first_seen_at, current.first_seen_at);
  const firstFrom = earliest === sheet.first_seen_at ? sheet : current;
  const latestDay = maxOf(sheet.last_active_day, current.last_active_day);
  const lastFrom = latestDay === current.last_active_day ? current : sheet;
  return {
    id: sheet.id, status: choice.status, revision: choice.revision, choice_at: choice.choice_at,
    opted_out_at: choice.opted_out_at ?? (choice.status === 'enabled' ? other.opted_out_at : null),
    status_received_at: choice.status_received_at,
    country: sheet.country ?? current.country,
    first_seen_at: earliest, first_seen_day: dayOfIso(earliest),
    first_version: earliest === null ? (sheet.first_version ?? current.first_version) : firstFrom.first_version,
    first_started_at: minOf(sheet.first_started_at, current.first_started_at),
    last_active_day: latestDay, last_version: lastFrom.last_version,
    last_received_at: maxOf(sheet.last_received_at, current.last_received_at),
    activity: choice.status === 'opted_out' ? [] : sheet.activity,
  };
}

const COLUMNS = ['id', 'status', 'revision', 'choice_at', 'opted_out_at', 'status_received_at', 'country', 'first_seen_at',
  'first_seen_day', 'first_started_at', 'first_version', 'last_active_day', 'last_version', 'last_received_at'];
export const sql = v => v === null || v === undefined ? 'NULL'
  : typeof v === 'number' ? (Number.isSafeInteger(v) ? String(v) : (() => { throw Error('Invalid number'); })())
  : `'${String(v).replace(/'/g, "''")}'`;

export function changed(merged, current) {
  if (!current) return true;
  return COLUMNS.some(c => (merged[c] ?? null) !== (current[c] ?? null));
}

// Statements for one installation. The upsert is guarded by the state that was
// read, so a report arriving during the import is never overwritten.
export function statementsFor(merged, current) {
  const out = [];
  const values = COLUMNS.map(c => sql(merged[c] ?? null)).join(', ');
  if (!current) {
    out.push(`INSERT INTO installations (${COLUMNS.join(', ')}) VALUES (${values}) ON CONFLICT (id) DO NOTHING;`);
  } else if (changed(merged, current)) {
    const set = COLUMNS.slice(1).map(c => `${c} = excluded.${c}`).join(', ');
    out.push(`INSERT INTO installations (${COLUMNS.join(', ')}) VALUES (${values}) ON CONFLICT (id) DO UPDATE SET ${set}`
      + ` WHERE installations.revision = ${sql(current.revision)} AND installations.status = ${sql(current.status)}`
      + ` AND installations.last_received_at IS ${sql(current.last_received_at)};`);
  }
  if (merged.status === 'opted_out') {
    out.push(`DELETE FROM activity WHERE id = ${sql(merged.id)} AND EXISTS (SELECT 1 FROM installations WHERE id = ${sql(merged.id)} AND status = 'opted_out');`);
  } else {
    for (const a of merged.activity) {
      out.push(`INSERT INTO activity (id, day, version, package_format, update_channel, installed_version, received_at) SELECT `
        + [merged.id, a.day, a.version, a.package_format, a.update_channel, a.installed_version, a.received_at].map(sql).join(', ')
        + ` WHERE EXISTS (SELECT 1 FROM installations WHERE id = ${sql(merged.id)} AND status = 'enabled') ON CONFLICT (id, day) DO NOTHING;`);
    }
  }
  return out;
}
