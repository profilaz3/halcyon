import test from 'node:test';
import assert from 'node:assert/strict';
import {createD1} from './d1.js';
import worker, {handleReport} from '../src/index.js';
import {dayOf, isoDay} from '../src/report.js';
import {DIRECT_SINCE, buildSummary, compareVersions, maintain, rollupDay} from '../src/stats.js';

const start = new Date('2026-10-26T09:00:00Z'); // a Monday
const at = days => new Date(+start + days * 86400000);
const tokenOf = i => i.toString(16).padStart(64, '0');
const report = (i, extra = {}) => ({schema: 5, kind: 'activity', installation: tokenOf(i), revision: 0, version: '0.5.6',
  country: i % 3 ? 'HR' : 'DE', first_started_at: null, package_format: 'deb', update_channel: 'apt',
  installed_app_version: '0.5.6', ...extra});
const post = (env, body, now) => handleReport(new Request('https://reports.test/v1/report', {method: 'POST', body: JSON.stringify(body)}), env, now);
const releases = async () => new Response(JSON.stringify([
  {tag_name: 'v0.6.0', published_at: at(7).toISOString(), draft: false, prerelease: false},
  {tag_name: 'v0.5.6', published_at: '2026-10-20T10:00:00Z', draft: false, prerelease: false},
  {tag_name: 'v0.7.0-rc1', published_at: at(8).toISOString(), draft: false, prerelease: true},
]));

test('versions sort numerically with pre-releases first', () => {
  assert.deepEqual(['0.10.0', '0.9.1', '0.9.1-rc1', '0.2.0'].sort(compareVersions), ['0.2.0', '0.9.1-rc1', '0.9.1', '0.10.0']);
});

test('a day rolls up per install method and version', () => {
  const rows = [
    {n: 1, day: 5, version: '0.6.0', format: 'deb', channel: 'apt', first: 5, country: 'HR'},
    {n: 2, day: 5, version: '0.5.6', format: 'rpm', channel: 'dnf', first: 1, country: 'HR'},
    {n: 3, day: 5, version: null, format: 'unknown', channel: 'unknown', first: null, country: null},
    {n: 1, day: 6, version: '0.6.0', format: 'deb', channel: 'apt', first: 5, country: 'HR'},
  ];
  const out = rollupDay(rows, 5);
  const get = (segment, metric, key = '') => out.find(r => r.segment === segment && r.metric === metric && r.key === key)?.value;
  assert.equal(get('all', 'active'), 3);
  assert.equal(get('all', 'new'), 1);
  assert.equal(get('deb', 'active'), 1);
  assert.equal(get('all', 'version', 'unknown'), 1);
  assert.equal(get('source', 'active'), undefined);
});

test('end to end: hourly maintenance publishes exact aggregates only', async () => {
  const env = {DB: createD1()};
  // 12 deb installations every day for 21 days; 3 rpm ones.
  // From day 8 (after 0.6.0 is published on day 7) installations move to 0.6.0.
  for (let d = 0; d < 21; d++) {
    for (let i = 1; i <= 12; i++) {
      if (i > 6 && d % 2) continue;
      await post(env, report(i, d >= 8 && i <= 8 ? {version: '0.6.0', installed_app_version: '0.6.0'} : {}), at(d));
    }
    for (let i = 13; i <= 15; i++) await post(env, report(i, {package_format: 'rpm', update_channel: 'manual', country: 'BR'}), at(d));
  }
  await post(env, report(99, {country: 'JP'}), at(14)); // a lone country and a late cohort member
  await post(env, {schema: 3, kind: 'status', installation: tokenOf(200), revision: 1, enabled: false, changed_at: 1}, at(3));

  const summary = await maintain(env.DB, at(21), {fetchImpl: releases, since: '0.5.6'});
  const text = JSON.stringify(summary);
  assert.equal(/[0-9a-f]{64}/.test(text), false, 'no installation hashes in the public summary');
  assert.equal(summary.history_start, '2026-10-26');
  assert.equal(summary.days.length, 21);
  assert.deepEqual(summary.segments.map(s => s.id), ['all', 'deb', 'rpm'], 'every install method seen gets a filter');

  const all = summary.by_segment.all;
  assert.equal(all.active[0], 15);
  assert.equal(all.active[1], 9);
  assert.equal(all.new[0], 15);
  assert.equal(all.windows[7].active, 15);
  assert.equal(all.windows[7].complete, true);
  assert.equal(all.windows[30].complete, false);
  assert.equal(all.windows[7].previous, 16);
  assert.deepEqual(all.versions.names, ['0.6.0', '0.5.6']);
  assert.deepEqual(all.install_methods.map(m => [m.name, m.count]), [['DEB package', 13], ['RPM package', 3]]);
  assert.deepEqual(all.countries.map(c => [c.name, c.count]), [['Croatia', 8], ['Germany', 4], ['Brazil', 3], ['Japan', 1]],
    'small groups are shown with their exact count');
  assert.ok(all.updates.total >= 13 && all.updates.on === 13);
  assert.equal(all.frequency.reduce((n, b) => n + b.count, 0), all.installs_28d);

  assert.equal(summary.latest_release.version, '0.6.0');
  const r060 = summary.releases.find(r => r.version === '0.6.0');
  assert.equal(r060.days_to_half, 1);
  assert.equal(summary.releases.some(r => r.version === '0.5.6'), false, 'released before history starts');
  assert.equal(summary.releases.some(r => r.version.includes('rc')), false);

  assert.equal(summary.cohorts[0].start, '2026-10-26');
  assert.equal(summary.cohorts[0].size, 15);
  assert.equal(summary.cohorts[0].rates[0], 1);
  assert.equal(summary.cohorts.find(c => c.start === '2026-11-09').size, 1, 'a cohort of one is shown');
  assert.equal(summary.opt_out_share, 1 / 17);
  assert.deepEqual(summary.sharing, {on: 16, off: 1});

  const response = await worker.fetch(new Request('https://reports.test/api/summary.json'), env);
  assert.equal(response.status, 200);
  assert.match(response.headers.get('Cache-Control'), /max-age=300/);
  assert.equal((await response.json()).generated_at, summary.generated_at);

  // Completed days are frozen: a later opt-out does not rewrite yesterday.
  const before = env.DB.raw.prepare(`SELECT value FROM daily WHERE day = ? AND segment = 'all' AND metric = 'active'`).get(dayOf(at(20))).value;
  await post(env, {schema: 3, kind: 'status', installation: tokenOf(1), revision: 1, enabled: false, changed_at: 2}, at(21));
  const again = await maintain(env.DB, new Date(+at(21) + 3600000), {fetchImpl: releases, since: '0.5.6'});
  assert.equal(again.by_segment.all.active[20], before);
});

test('only direct reports count: older versions and days before the release are left out', async () => {
  assert.equal(DIRECT_SINCE, '0.6.1');
  const env = {DB: createD1()};
  const direct = async () => new Response(JSON.stringify([
    {tag_name: 'v0.6.1', published_at: at(5).toISOString(), draft: false, prerelease: false},
    {tag_name: 'v0.6.0', published_at: at(5).toISOString(), draft: false, prerelease: false},
  ]));
  for (let d = 0; d < 10; d++) {
    // Eight installations on 0.6.0 throughout; six move to 0.6.1 on release day.
    for (let i = 1; i <= 8; i++) await post(env, report(i, {version: '0.6.0'}), at(d));
    // A pre-release 0.6.1 build tested before the release does not start history.
    if (d === 2) await post(env, report(50, {version: '0.6.1'}), at(d));
    if (d >= 5) for (let i = 11; i <= 16; i++) await post(env, report(i, {version: '0.6.1'}), at(d));
  }
  await post(env, {schema: 3, kind: 'status', installation: tokenOf(3), revision: 1, enabled: false, changed_at: 1}, at(9));
  // An earlier run under the old scope froze days that counted 0.6.0.
  await maintain(env.DB, at(8), {fetchImpl: direct, since: '0.6.0'});
  const summary = await maintain(env.DB, at(10), {fetchImpl: direct});
  assert.equal(summary.since_version, '0.6.1');
  assert.equal(summary.history_start, isoDay(dayOf(at(5))));
  assert.deepEqual(summary.by_segment.all.active, [6, 6, 6, 6, 6]);
  assert.deepEqual(summary.by_segment.all.versions.names, ['0.6.1']);
  // Everything counted runs 0.6.1 or newer, so 0.6.1 is the baseline: no
  // adoption time, no latest-version share, and 0.6.0 is not a release here.
  assert.deepEqual(summary.releases, [{version: '0.6.1', date: isoDay(dayOf(at(5))), days_to_half: null, baseline: true}]);
  assert.equal(summary.by_segment.all.latest_share, null);
  assert.equal(summary.by_segment.all.windows[7].active, 6);
  assert.equal(summary.opt_out_share, 0, 'an older version turning sharing off is not counted');
  const days = env.DB.raw.prepare(`SELECT MIN(day) AS first FROM daily`).get().first;
  assert.equal(days, dayOf(at(5)), 'totals frozen under the old scope are rebuilt');
});

test('summary handles an empty database', () => {
  const summary = buildSummary({rows: [], daily: [], releases: [], statuses: [], now: start});
  assert.equal(summary.history_start, null);
  assert.deepEqual(summary.days, []);
  assert.equal(summary.by_segment.all.windows[7].active, 0);
  assert.equal(summary.retention_4w, null);
  assert.equal(summary.opt_out_share, null);
  assert.equal(isoDay(dayOf(start)), summary.today);
});

test('summary endpoint is not ready before the first maintenance run', async () => {
  const response = await worker.fetch(new Request('https://reports.test/api/summary.json'), {DB: createD1()});
  assert.equal(response.status, 503);
});
