import test from 'node:test';
import assert from 'node:assert/strict';
import {createD1} from './d1.js';
import {handleReport} from '../src/index.js';
import {MAX_BODY, validateReport} from '../src/report.js';
import {PAGES, PROBLEMS, SETUP, canonicalDetails, detailsSummary} from '../src/details.js';
import {maintain} from '../src/stats.js';

const tokenOf = i => (i + 2000).toString(16).padStart(64, '0');
const features = {wallpaper_effect: 'off', activity_island: true, penumbra: false, islands: null, widgets: ['clock'],
  visualizer: false, wallpaper_depth: false, theme_style: 'default', automation: false};
const system = (extra = {}) => ({distro: 'pop', distro_version: '24.04', cosmic: '1.0.3', gpu: ['amd'], displays: 2,
  resolution: '3840x2160', ...extra});
const v7 = (i, extra = {}) => ({schema: 7, kind: 'activity', installation: tokenOf(i), revision: 0, version: '0.6.1',
  country: 'HR', first_started_at: null, package_format: 'deb', update_channel: 'apt', installed_app_version: '0.6.1',
  features, pages: {appearance: 2, weather: 1}, runtime_hours: 5, problems: {}, setup: {}, system: system(), ...extra});
const post = (env, body, now) => handleReport(new Request('https://reports.test/v1/report', {method: 'POST', body: JSON.stringify(body)}), env, now)
  .then(r => r.json());
const details = r => (({pages, problems, runtime_hours, setup, system}) => ({pages, problems, runtime_hours, setup, system}))(r);

test('schema 7 accepts only fixed keys, fixed values and whole counts', () => {
  assert.equal(validateReport(JSON.stringify(v7(1))).schema, 7);
  const nothing = v7(1, {pages: {}, runtime_hours: null, system: Object.fromEntries(Object.keys(system()).map(k => [k, null]))});
  assert.equal(validateReport(JSON.stringify(nothing)).schema, 7);
  const every = vocabulary => Object.fromEntries(Object.keys(vocabulary).map(k => [k, 100000]));
  const largest = JSON.stringify(v7(1, {pages: every(PAGES), problems: every(PROBLEMS), setup: every(SETUP), runtime_hours: 10000,
    system: system({distro: 'a'.repeat(32), distro_version: 'b'.repeat(16), cosmic: '9999.999999.999999', gpu: ['amd', 'intel', 'nvidia', 'other'],
      resolution: '16384x16384'})}));
  assert.ok(largest.length <= MAX_BODY, `the largest valid report (${largest.length} bytes) fits the body limit`);
  assert.equal(validateReport(largest).schema, 7);
  for (const bad of [
    {pages: {'/home/me': 1}}, {pages: {appearance: 0}}, {pages: {appearance: 1.5}}, {pages: {appearance: -1}},
    {pages: {appearance: 100001}}, {pages: []}, {problems: {panic: '1'}}, {problems: {'Error: disk full': 1}},
    {setup: {'skipped.look': 1}}, {runtime_hours: 1.5}, {runtime_hours: -1}, {runtime_hours: 10001},
    {system: system({distro: 'Pop!_OS'})}, {system: system({distro: 'pop os'})}, {system: system({distro: 'a'.repeat(33)})},
    {system: system({distro_version: '24.04 LTS (Noble)'})}, {system: system({cosmic: '1.0.0~alpha'})},
    {system: system({gpu: ['radeon']})}, {system: system({gpu: []})}, {system: system({gpu: ['amd', 'amd']})},
    {system: system({displays: 0})}, {system: system({displays: 17})}, {system: system({resolution: '3840*2160'})},
    {system: system({resolution: '0x0'})}, {system: system({hostname: 'desk'})}, {system: {...system(), locale: 'hr_HR'}},
    {locale: 'hr_HR'}, {language: 'hr'},
  ]) assert.throws(() => validateReport(JSON.stringify(v7(1, bad))), JSON.stringify(bad));
  assert.throws(() => validateReport(JSON.stringify((({system: _, ...rest}) => rest)(v7(1)))), 'schema 7 needs every field');
  assert.throws(() => validateReport(JSON.stringify({...v7(1), schema: 6})), 'schema 6 has no details');
  assert.equal(canonicalDetails(details(v7(1, {system: system({gpu: ['nvidia', 'amd']})}))),
    canonicalDetails(details(v7(1, {system: system({gpu: ['amd', 'nvidia']})}))), 'GPU order is canonical');
});

test('details are stored with the day, a resend replaces them, and only aggregates are published', async () => {
  const env = {DB: createD1()}, now = new Date('2026-11-10T09:00:00Z');
  for (let i = 0; i < 4; i++) assert.deepEqual(await post(env, v7(i), now), {ok: true, day: '2026-11-10'});
  // A resend of the same day (lost acknowledgement) replaces, never adds up.
  assert.deepEqual(await post(env, v7(0, {pages: {appearance: 3, weather: 1}}), now), {ok: true, day: '2026-11-10'});
  await post(env, v7(1, {problems: {panic: 1, 'update.offline': 2}, setup: {opened: 1, 'reached.look': 1, 'reached.bars': 1,
    'skipped.bars': 1, kept_for_later: 1}, system: system({distro: 'fedora', distro_version: '43', gpu: ['intel', 'nvidia'],
    displays: 1, resolution: '1920x1080'})}), now);
  await post(env, v7(2, {schema: 6, pages: undefined, problems: undefined, runtime_hours: undefined, setup: undefined, system: undefined}), now);
  const stored = env.DB.raw.prepare('SELECT details FROM activity WHERE details IS NOT NULL').all();
  assert.equal(stored.length, 3, 'a schema 6 resend clears the details of that day');

  const later = new Date('2026-11-11T09:00:00Z');
  await post(env, v7(1, {problems: {render_timeout: 1}, setup: {opened: 1, 'reached.look': 1, finished: 1}, runtime_hours: 9}), later);
  const summary = await maintain(env.DB, later, {fetchImpl: async () => new Response('[]')});
  const d = summary.details;
  assert.equal(d.reported, 3);
  assert.equal(d.reports, 4);
  const page = Object.fromEntries(d.pages.map(p => [p.key, p]));
  assert.deepEqual([page.appearance.installations, page.appearance.count], [3, 9]);
  assert.deepEqual([page.weather.installations, page.weather.count], [3, 4]);
  assert.equal(d.pages[0].name, 'Glass and transparency');
  const problem = Object.fromEntries(d.problems.items.map(p => [p.key, p]));
  assert.deepEqual([problem.panic.installations, problem['update.offline'].count, problem.render_timeout.count], [1, 2, 1]);
  assert.deepEqual([d.problems.affected, d.problems.clean], [1, 2]);
  assert.equal(d.setup.started, 1);
  const step = Object.fromEntries(d.setup.steps.map(s => [s.key, s]));
  assert.deepEqual([step['reached.look'].installations, step['reached.look'].count, step['reached.bars'].skipped], [1, 2, 1]);
  assert.ok(!('reached.weather' in step), 'the older Weather step is shown only when reported');
  const outcome = Object.fromEntries(d.setup.outcomes.map(o => [o.key, o.installations]));
  assert.deepEqual([outcome.kept_for_later, outcome.finished, outcome.cancelled], [1, 1, 0]);
  assert.deepEqual([d.runtime.reports, d.runtime.median_hours], [4, 5]);
  assert.equal(d.runtime.buckets.find(b => b.name === '9 to 16 hours').count, 1);
  // System facts come from each installation's latest report.
  assert.equal(d.system.reported, 3);
  assert.deepEqual(d.system.distro.map(x => [x.name, x.count]), [['Pop!_OS 24.04', 3]]);
  assert.deepEqual(d.system.gpu.map(x => [x.key, x.count]), [['amd', 3]]);
  assert.deepEqual(d.system.displays.map(x => [x.name, x.count]), [['2 displays', 3]]);
  assert.deepEqual(d.system.resolution.map(x => [x.name, x.count]), [['3840 × 2160', 3]]);
  assert.ok(!JSON.stringify(summary).includes(tokenOf(0)));
});

test('a single installation is published with exact counts, unknowns last', () => {
  const row = (n, day, extra) => ({n, day, details: canonicalDetails(details(v7(n, extra)))});
  const d = detailsSummary([
    row(1, 10, {}),
    row(2, 10, {runtime_hours: null, system: system({distro: null, distro_version: null, cosmic: null, gpu: null, displays: null, resolution: null})}),
  ], 0, 20);
  assert.equal(d.reported, 2);
  assert.equal(d.runtime.reports, 1);
  assert.deepEqual(d.system.distro.map(x => [x.key, x.count]), [['pop 24.04', 1], ['unknown', 1]]);
  assert.deepEqual(d.system.gpu.map(x => [x.key, x.count]), [['amd', 1], ['unknown', 1]]);
  assert.deepEqual(d.system.cosmic.map(x => [x.name, x.count]), [['1.0.3', 1], ['Not reported', 1]]);
  const empty = detailsSummary([{n: 1, day: 10, details: null}], 0, 20);
  assert.deepEqual([empty.reported, empty.pages, empty.system], [0, null, null], 'nothing reported, nothing shown');
});
