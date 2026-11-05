#!/usr/bin/env node
// Import the former Google Sheet collector into D1.
//
//   node tools/import-sheet.mjs Installations.csv --sql out.sql          # dry run
//   node tools/import-sheet.mjs Installations.csv --sql out.sql --apply # write
//
// Export only the "Installations" tab (File > Download > CSV). Safe to repeat:
// rows are merged with live data, never overwritten blindly. Wrangler must be
// authenticated through its usual environment variables. Prints counts only;
// never prints installation hashes.

import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {dayOf} from '../src/report.js';
import {changed, mergeRecords, parseCsv, sheetRecord, statementsFor} from './sheet-import.js';

const HEADER = 'Installation hash';
const BYTE_ORDER_MARK = String.fromCharCode(0xfeff);
const args = process.argv.slice(2);
const csvPath = args.find(a => !a.startsWith('--') && a !== args[args.indexOf('--sql') + 1]);
const sqlPath = args.includes('--sql') ? args[args.indexOf('--sql') + 1] : null;
const apply = args.includes('--apply');
if (!csvPath || !sqlPath) {
  console.error('Usage: import-sheet.mjs <Installations.csv> --sql <out.sql> [--apply]');
  process.exit(2);
}
const cwd = fileURLToPath(new URL('..', import.meta.url));
const wrangler = extra => execFileSync('npx', ['-y', 'wrangler@4.158.0', 'd1', 'execute', 'halcyon-reports', '--remote', ...extra],
  {cwd, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, env: {...process.env, WRANGLER_SEND_METRICS: 'false', CI: '1'}});

let text = readFileSync(csvPath, 'utf8');
if (text.startsWith(BYTE_ORDER_MARK)) text = text.slice(1);
const rows = parseCsv(text);
if (rows[0]?.[0] !== HEADER) throw Error('Not an Installations export: unexpected header');
const today = dayOf(new Date());
const records = [], errors = new Map();
let skipped = 0;
for (const cells of rows.slice(1)) {
  if (cells.every(c => c === '')) continue;
  try {
    const record = sheetRecord(cells, today);
    if (record) records.push(record); else skipped++;
  } catch (error) {
    errors.set(error.message, (errors.get(error.message) || 0) + 1);
  }
}
if (errors.size) {
  console.error('Refusing to import: some rows could not be read.');
  for (const [message, count] of errors) console.error(`  ${count} x ${message}`);
  process.exit(1);
}
// The sheet may hold duplicate rows for one installation; merge them first.
const byId = new Map();
for (const r of records) {
  const previous = byId.get(r.id);
  if (!previous) { byId.set(r.id, r); continue; }
  const merged = mergeRecords(r, previous);
  const days = new Map([...previous.activity, ...r.activity].map(a => [a.day, a]));
  merged.activity = merged.status === 'opted_out' ? [] : [...days.values()].sort((a, b) => a.day - b.day);
  byId.set(r.id, merged);
}

const live = JSON.parse(wrangler(['--json', '--command', 'SELECT * FROM installations']))[0].results;
const current = new Map(live.map(r => [r.id, r]));
const statements = [];
const counts = {sheet_rows: records.length, installations: byId.size, skipped_without_hash: skipped,
  new: 0, updated: 0, unchanged: 0, opted_out: 0, activity_days: 0};
for (const record of byId.values()) {
  const existing = current.get(record.id) || null;
  const merged = mergeRecords(record, existing);
  if (!existing) counts.new++; else if (changed(merged, existing)) counts.updated++; else counts.unchanged++;
  if (merged.status === 'opted_out') counts.opted_out++;
  counts.activity_days += merged.activity.length;
  statements.push(...statementsFor(merged, existing));
}
// Recompute daily totals for the whole retained window from merged raw data.
statements.push(`DELETE FROM meta WHERE key = 'rolled_through';`);
statements.push(`INSERT INTO meta (key, value) VALUES ('sheet_imported_at', '${new Date().toISOString()}') ON CONFLICT (key) DO UPDATE SET value = excluded.value;`);
writeFileSync(sqlPath, statements.join('\n') + '\n', {mode: 0o600});
console.log(JSON.stringify({...counts, statements: statements.length, applied: apply}, null, 2));
if (apply) {
  wrangler(['--yes', '--file', sqlPath]);
  console.log('Applied. Daily totals are rebuilt at the next hourly run.');
}
