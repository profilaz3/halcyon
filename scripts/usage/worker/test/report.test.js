import test from 'node:test';
import assert from 'node:assert/strict';
import {createD1} from './d1.js';
import worker, {handleReport} from '../src/index.js';
import {applyReport, applyRetention, dayOf, installationId, validateReport} from '../src/report.js';

const token = 'a'.repeat(64);
const legacy = {schema: 1, installation: token, version: '0.2.0'};
const v5 = {schema: 5, kind: 'activity', installation: token, revision: 0, version: '0.6.0', country: 'HR',
  first_started_at: 1792756800, package_format: 'deb', update_channel: 'apt', installed_app_version: '0.6.0'};
const now = new Date('2026-10-23T12:00:00Z');
const later = days => new Date(+now + days * 86400000);

const post = (env, body, at = now) => handleReport(new Request('https://reports.test/v1/report', {
  method: 'POST', headers: {'Content-Type': 'application/json'}, body: typeof body === 'string' ? body : JSON.stringify(body),
}), env, at).then(r => r.json());
const state = db => ({
  installations: db.raw.prepare('SELECT * FROM installations ORDER BY id').all().map(r => ({...r})),
  activity: db.raw.prepare('SELECT * FROM activity ORDER BY id, day').all().map(r => ({...r})),
});
const row = (db, id) => ({...db.raw.prepare('SELECT * FROM installations WHERE id = ?').get(id)});

test('accepts only the documented report shapes', () => {
  assert.equal(validateReport(JSON.stringify(legacy)).kind, 'activity');
  assert.equal(validateReport(JSON.stringify(v5)).package_format, 'deb');
  for (const bad of [{...legacy, email: 'private@example.com'}, {...legacy, schema: 2}, {...legacy, version: '=IMPORTXML("url")'},
    {...legacy, installation: 'short'}, null, [], {...v5, package_format: 'rpm', update_channel: 'apt'},
    {...v5, package_format: 'source', update_channel: 'source', installed_app_version: '0.6.0'}, {...v5, hostname: 'x'}])
    assert.throws(() => validateReport(JSON.stringify(bad)));
  assert.throws(() => validateReport(' '.repeat(1025)));
  const status = {schema: 3, kind: 'status', installation: token, revision: 1, enabled: false, changed_at: 0};
  assert.equal(validateReport(JSON.stringify(status)).enabled, false);
  for (const bad of [{...status, revision: 0}, {...status, revision: 1.5}, {...status, enabled: 'false'},
    {...status, changed_at: -1}, {...status, country: 'HR'}, {...status, changed_at: 253402300800}])
    assert.throws(() => validateReport(JSON.stringify(bad)));
  for (const country of ['hr', 'H', 'HRV', '1A', '=US', {}, 42])
    assert.throws(() => validateReport(JSON.stringify({...legacy, schema: 2, country})));
});

test('handler stores a hash, one row per day, and answers like the Apps Script collector', async () => {
  const env = {DB: createD1()};
  assert.deepEqual(await post(env, legacy), {ok: true, day: '2026-10-23'});
  assert.deepEqual(await post(env, legacy), {ok: true, day: '2026-10-23'});
  const s = state(env.DB);
  assert.equal(s.installations.length, 1);
  assert.equal(s.installations[0].id, await installationId(token));
  assert.notEqual(s.installations[0].id, token);
  assert.equal(s.activity.length, 1);
  assert.deepEqual(await post(env, {...legacy, email: 'private@example.com'}), {ok: false});
  assert.deepEqual(await post(env, 'x'.repeat(2000)), {ok: false});
  assert.deepEqual(await post(env, '{'), {ok: false});
  const health = await worker.fetch(new Request('https://reports.test/v1/report'), env);
  assert.deepEqual(await health.json(), {service: 'halcyon-activity', schema: 6});
  assert.equal((await worker.fetch(new Request('https://reports.test/v1/report', {method: 'PUT'}), env)).status, 405);
  assert.equal(JSON.stringify(state(env.DB)).includes(token), false);
});

test('schema 5 keeps first-use metadata and one package observation per day', async () => {
  const env = {DB: createD1()}, id = await installationId(token);
  await post(env, v5);
  await post(env, {...v5, version: '0.6.1', installed_app_version: '0.6.1', first_started_at: 0, country: 'US'}, new Date(+now + 3600000));
  let r = row(env.DB, id);
  assert.equal(r.first_seen_at, now.toISOString());
  assert.equal(r.first_seen_day, dayOf(now));
  assert.equal(r.first_started_at, new Date(v5.first_started_at * 1000).toISOString());
  assert.equal(r.first_version, '0.6.0');
  assert.equal(r.last_version, '0.6.1');
  assert.equal(r.country, 'HR');
  let s = state(env.DB);
  assert.equal(s.activity.length, 1);
  assert.equal(s.activity[0].version, '0.6.1');
  await post(env, {schema: 3, kind: 'activity', installation: token, revision: 0, version: '0.6.1', country: null}, later(1));
  s = state(env.DB);
  assert.equal(s.activity.length, 2);
  assert.deepEqual([s.activity[1].package_format, s.activity[1].update_channel, s.activity[1].installed_version], ['unknown', 'unknown', null]);
  r = row(env.DB, id);
  assert.equal(r.last_active_day, dayOf(now) + 1);
  assert.equal(r.first_seen_at, now.toISOString());
});

test('opt-out removes activity, ignores replays and stale reports, and re-enable keeps the off time', async () => {
  const env = {DB: createD1()}, id = await installationId(token);
  const off = {schema: 3, kind: 'status', installation: token, revision: 1, enabled: false, changed_at: 123};
  await post(env, {...legacy, schema: 2, country: 'HR'});
  assert.deepEqual(await post(env, off), {ok: true, revision: 1, enabled: false});
  let r = row(env.DB, id);
  assert.equal(r.status, 'opted_out');
  assert.equal(r.opted_out_at, new Date(123000).toISOString());
  assert.equal(r.status_received_at, now.toISOString());
  assert.equal(state(env.DB).activity.length, 0);
  const frozen = state(env.DB);
  for (const report of [off, legacy, {...legacy, schema: 2, country: 'US'},
    {schema: 3, kind: 'activity', installation: token, revision: 0, version: '0.2.1', country: 'US'},
    {schema: 3, kind: 'activity', installation: token, revision: 2, version: '0.2.1', country: null}]) {
    assert.equal((await post(env, report, later(1))).ok, true);
    assert.deepEqual(state(env.DB), frozen);
  }
  assert.deepEqual(await post(env, {...off, changed_at: 124}, later(1)), {ok: false});
  const on = {...off, revision: 2, enabled: true, changed_at: 5};
  assert.deepEqual(await post(env, on, later(1)), {ok: true, revision: 2, enabled: true});
  r = row(env.DB, id);
  assert.equal(r.status, 'enabled');
  assert.equal(r.opted_out_at, new Date(123000).toISOString());
  assert.deepEqual(await post(env, off, later(1)), {ok: true, revision: 2, enabled: true});
  await post(env, {schema: 3, kind: 'activity', installation: token, revision: 1, version: '0.2.1', country: null}, later(1));
  assert.equal(state(env.DB).activity.length, 0);
  await post(env, {schema: 3, kind: 'activity', installation: token, revision: 2, version: '0.2.1', country: null}, later(2));
  assert.equal(row(env.DB, id).last_active_day, dayOf(now) + 2);
  assert.equal(row(env.DB, id).country, 'HR');
});

test('first-run opt-out stores no activity details and expires 90 days after receipt', async () => {
  const env = {DB: createD1()}, id = await installationId(token);
  await post(env, {schema: 3, kind: 'status', installation: token, revision: 3, enabled: false, changed_at: 253402300799});
  const r = row(env.DB, id);
  for (const field of ['country', 'first_seen_at', 'first_started_at', 'last_active_day', 'last_version']) assert.equal(r[field], null);
  await applyRetention(env.DB, later(89));
  assert.equal(state(env.DB).installations.length, 1);
  await applyRetention(env.DB, later(90));
  assert.equal(state(env.DB).installations.length, 0);
});

test('retention prunes old days, quiet installations and old choice timestamps', async () => {
  const env = {DB: createD1()};
  const other = {...v5, installation: 'b'.repeat(64)};
  await post(env, {schema: 3, kind: 'status', installation: token, revision: 1, enabled: true, changed_at: 1});
  await post(env, {...v5, revision: 1});
  await post(env, other);
  for (let d = 1; d < 100; d++) await post(env, {...v5, revision: 1}, later(d));
  await applyRetention(env.DB, later(99));
  const s = state(env.DB);
  assert.equal(s.installations.length, 1);
  const kept = s.installations[0];
  assert.equal(kept.choice_at, null);
  assert.equal(kept.status_received_at, null);
  assert.equal(s.activity.length, 90);
  assert.ok(s.activity.every(a => a.day >= dayOf(later(99)) - 89));
});

test('a concurrent choice between read and write is never overwritten by activity', async () => {
  const db = createD1(), id = await installationId(token);
  await applyReport(db, id, validateReport(JSON.stringify({...v5, revision: 1})), now);
  const batch = db.batch;
  let raced = false;
  db.batch = async statements => {
    if (!raced) {
      raced = true;
      db.raw.prepare(`UPDATE installations SET status = 'opted_out', revision = 2 WHERE id = ?`).run(id);
      db.raw.prepare('DELETE FROM activity WHERE id = ?').run(id);
    }
    return batch(statements);
  };
  const ack = await applyReport(db, id, validateReport(JSON.stringify({...v5, revision: 1})), later(1));
  assert.deepEqual(ack, {ok: true, day: '2026-10-24'});
  assert.equal(state(db).activity.length, 0);
  assert.equal(row(db, id).last_active_day, dayOf(now));
});

test('existing installations still report at capacity, new ones are refused', async () => {
  const env = {DB: createD1()};
  await post(env, v5);
  env.DB.raw.exec(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 99999)
    INSERT INTO installations (id, status, revision) SELECT printf('%064x', i), 'enabled', 0 FROM n`);
  assert.equal((await post(env, {...v5, installation: 'c'.repeat(64)})).ok, false);
  assert.equal((await post(env, v5, later(1))).ok, true);
});
