import test from 'node:test';
import assert from 'node:assert/strict';
import {createD1} from './d1.js';
import {applyReport, dayOf, installationId, validateReport} from '../src/report.js';
import {maintain} from '../src/stats.js';
import {mergeRecords, parseCsv, parseDay, parseTime, sheetRecord, statementsFor} from '../tools/sheet-import.js';

const now = new Date('2026-11-05T12:00:00Z'), today = dayOf(now), serial = d => d + 25569;
const id = 'b'.repeat(64);
const csvCell = v => /[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
const sheetRow = (overrides = {}) => {
  const cells = [id, String(serial(today - 1)), '0.6.0', 'HR', 'enabled', '', '', '0', '',
    '2026-10-27T08:00:00.000Z', '', '0.5.6', '3', JSON.stringify([serial(today - 9), serial(today - 3), serial(today - 1)]),
    '2026-11-04T08:00:00.000Z', JSON.stringify([[serial(today - 9), '0.5.6'], [serial(today - 1), '0.6.0']]),
    JSON.stringify([[serial(today - 1), 'deb', 'apt', '0.6.0', '0.6.0', '2026-11-04T08:00:00.000Z']])];
  for (const [k, v] of Object.entries(overrides)) cells[k] = v;
  return cells;
};
const apply = (db, statements) => { for (const s of statements) db.raw.exec(s); };

test('CSV parsing keeps quoted JSON cells intact', () => {
  const row = sheetRow();
  const rows = parseCsv(['Installation hash,x', row.map(csvCell).join(',')].join('\r\n') + '\r\n');
  assert.deepEqual(rows[1], row);
});

test('day and time cells accept Sheets serials and displayed values', () => {
  assert.equal(parseDay('46330'), 46330 - 25569);
  assert.equal(parseDay('2026-11-05'), today);
  assert.equal(parseDay('5 Nov 2026'), today);
  assert.equal(parseDay('26 Oct 2026'), today - 10);
  assert.equal(parseTime('2026-10-27 08:00:00'), '2026-10-27T08:00:00.000Z');
  assert.equal(parseTime('2026-10-27T08:00:00.000Z'), '2026-10-27T08:00:00.000Z');
  assert.throws(() => parseDay('11/5/2026'));
  assert.throws(() => parseTime('yesterday'));
});

test('a sheet row becomes an installation with dated activity and known versions', () => {
  const r = sheetRecord(sheetRow(), today);
  assert.equal(r.first_seen_day, dayOf(new Date('2026-10-27T08:00:00Z')));
  assert.equal(r.first_version, '0.5.6');
  assert.deepEqual(r.activity.map(a => [a.day - today, a.version, a.package_format]),
    [[-9, '0.5.6', 'unknown'], [-3, '0.5.6', 'unknown'], [-1, '0.6.0', 'deb']]);
  const legacy = sheetRecord(sheetRow({9: '', 11: '', 13: '', 15: '', 16: ''}), today);
  assert.deepEqual(legacy.activity.map(a => [a.day - today, a.version]), [[-1, '0.6.0']]);
  assert.equal(legacy.first_seen_at, null);
  const off = sheetRecord(sheetRow({4: 'opted_out', 5: '2026-11-01T00:00:00.000Z', 6: '2026-11-01T00:00:01.000Z', 7: '2', 8: '2026-11-01T00:00:00.000Z'}), today);
  assert.deepEqual(off.activity, []);
  assert.equal(sheetRecord(['', '', ''], today), null);
  const old = sheetRecord(sheetRow({13: JSON.stringify([serial(today - 120), serial(today - 1)])}), today);
  assert.deepEqual(old.activity.map(a => a.day - today), [-1], 'days outside retention are not imported');
});

test('merging favors the higher revision, then an opt-out, and keeps first-use facts', () => {
  const sheet = sheetRecord(sheetRow({7: '1'}), today);
  const live = {...sheet, revision: 1, status: 'opted_out', country: null, first_seen_at: null, last_active_day: null};
  assert.equal(mergeRecords(sheet, live).status, 'opted_out');
  assert.deepEqual(mergeRecords(sheet, live).activity, []);
  const newer = {...sheet, revision: 2, status: 'enabled', country: 'US', first_seen_at: '2026-11-05T09:00:00.000Z', last_active_day: today, last_version: '0.6.1'};
  const m = mergeRecords(sheet, newer);
  assert.equal(m.revision, 2);
  assert.equal(m.country, 'HR');
  assert.equal(m.first_seen_at, '2026-10-27T08:00:00.000Z');
  assert.equal(m.last_version, '0.6.1');
});

test('import statements merge with live data, are repeatable and never overwrite a newer choice', async () => {
  const db = createD1();
  const token = 'c'.repeat(64), liveId = await installationId(token);
  await applyReport(db, liveId, validateReport(JSON.stringify({schema: 5, kind: 'activity', installation: token, revision: 0,
    version: '0.6.0', country: null, first_started_at: null, package_format: 'rpm', update_channel: 'dnf', installed_app_version: '0.6.0'})), now);
  const sheetForLive = sheetRecord(sheetRow({0: liveId}), today);
  const current = {...db.raw.prepare('SELECT * FROM installations WHERE id = ?').get(liveId)};
  const fresh = sheetRecord(sheetRow(), today);
  const statements = [...statementsFor(mergeRecords(sheetForLive, current), current), ...statementsFor(fresh, null)];
  apply(db, statements);
  apply(db, statements);
  const live = {...db.raw.prepare('SELECT * FROM installations WHERE id = ?').get(liveId)};
  assert.equal(live.country, 'HR');
  assert.equal(live.first_seen_at, '2026-10-27T08:00:00.000Z');
  assert.equal(live.last_active_day, today);
  assert.equal(db.raw.prepare('SELECT COUNT(*) AS n FROM activity WHERE id = ?').get(liveId).n, 4);
  assert.equal(db.raw.prepare('SELECT package_format AS f FROM activity WHERE id = ? AND day = ?').get(liveId, today).f, 'rpm');
  assert.equal(db.raw.prepare('SELECT COUNT(*) AS n FROM activity WHERE id = ?').get(id).n, 3);

  // A choice that lands between reading and writing wins over the import.
  const stale = {...db.raw.prepare('SELECT * FROM installations WHERE id = ?').get(id)};
  await applyReport(db, id, {schema: 3, kind: 'status', revision: 5, enabled: false, changed_at: 1}, now);
  apply(db, statementsFor(mergeRecords(sheetRecord(sheetRow({2: '0.6.1'}), today), stale), stale));
  const after = {...db.raw.prepare('SELECT * FROM installations WHERE id = ?').get(id)};
  assert.equal(after.status, 'opted_out');
  assert.equal(after.revision, 5);
  assert.equal(db.raw.prepare('SELECT COUNT(*) AS n FROM activity WHERE id = ?').get(id).n, 0);

  const summary = await maintain(db, now, {fetchImpl: async () => new Response('[]')});
  assert.equal(summary.history_start, '2026-10-27');
  assert.equal(summary.by_segment.all.active[summary.days.indexOf('2026-11-04')], 1);
});
