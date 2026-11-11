import test from 'node:test';
import assert from 'node:assert/strict';
import {createD1} from './d1.js';
import {handleReport} from '../src/index.js';
import {canonicalFeatures, validateReport} from '../src/report.js';
import {featureSummary, maintain, rollupDay} from '../src/stats.js';

const tokenOf = i => (i + 1000).toString(16).padStart(64, '0');
const features = (extra = {}) => ({wallpaper_effect: 'rain', activity_island: true, penumbra: false, islands: null,
  widgets: ['weather', 'clock'], visualizer: false, wallpaper_depth: true, theme_style: 'glass', automation: false, ...extra});
const v6 = (i, extra = {}) => ({schema: 6, kind: 'activity', installation: tokenOf(i), revision: 0, version: '0.6.1',
  country: 'HR', first_started_at: null, package_format: 'deb', update_channel: 'apt', installed_app_version: '0.6.1',
  features: features(), ...extra});
const post = (env, body, now) => handleReport(new Request('https://reports.test/v1/report', {method: 'POST', body: JSON.stringify(body)}), env, now)
  .then(r => r.json());

test('schema 6 accepts only the fixed feature keys and values', () => {
  assert.equal(validateReport(JSON.stringify(v6(1))).schema, 6);
  assert.ok(JSON.stringify(v6(1, {features: features({widgets: ['calendar', 'projects', 'devbox', 'weather', 'clock',
    'glance', 'music', 'notes', 'focus']})})).length < 1024, 'a full snapshot fits the body limit');
  const all = Object.fromEntries(Object.keys(features()).map(k => [k, null]));
  assert.equal(validateReport(JSON.stringify(v6(1, {features: all}))).schema, 6);
  for (const bad of [
    features({wallpaper_effect: 'lava'}), features({penumbra: 'yes'}), features({widgets: ['weather', 'weather']}),
    features({widgets: ['/home/me']}), features({theme_style: 'Glass'}), features({location: 'Zagreb'}),
    (({automation, ...rest}) => rest)(features()), [], 'rain', null,
  ]) assert.throws(() => validateReport(JSON.stringify(v6(1, {features: bad}))), JSON.stringify(bad));
  assert.throws(() => validateReport(JSON.stringify({...v6(1), schema: 5})), 'schema 5 has no features');
  assert.throws(() => validateReport(JSON.stringify((({features, ...rest}) => rest)(v6(1)))), 'schema 6 needs features');
  assert.equal(canonicalFeatures(features({widgets: ['clock', 'weather']})), canonicalFeatures(features()), 'widget order is canonical');
});

test('the snapshot is stored with the day and counted only in aggregates', async () => {
  const env = {DB: createD1()}, now = new Date('2026-11-10T09:00:00Z');
  for (let i = 0; i < 7; i++) assert.deepEqual(await post(env, v6(i, i < 2 ? {features: features({activity_island: false, theme_style: 'neon'})} : {}), now), {ok: true, day: '2026-11-10'});
  assert.deepEqual(await post(env, {...v6(7), schema: 5, features: undefined}, now), {ok: true, day: '2026-11-10'});
  const stored = env.DB.raw.prepare('SELECT features FROM activity WHERE features IS NOT NULL').all();
  assert.equal(stored.length, 7);
  assert.deepEqual(JSON.parse(stored[0].features).widgets, ['weather', 'clock']);

  const summary = await maintain(env.DB, new Date('2026-11-11T09:00:00Z'), {fetchImpl: async () => new Response('[]')});
  const f = summary.features;
  assert.equal(f.reported, 7);
  const sw = Object.fromEntries(f.switches.map(s => [s.key, s]));
  assert.equal(sw.activity_island.on, 5);
  assert.equal(sw.penumbra.on, 0);
  assert.equal(sw.islands.known, 0);
  assert.equal(sw.islands.on, 0);
  assert.equal(sw.wallpaper_effect.on, 7);
  assert.equal(f.widgets.items.find(w => w.key === 'weather').on, 7);
  assert.equal(f.widgets.items.find(w => w.key === 'music').on, 0);
  assert.deepEqual(f.theme_style.map(t => [t.key, t.count]), [['glass', 5], ['neon', 2]], 'two neon are shown as two');
  assert.ok(!JSON.stringify(summary).includes(tokenOf(0)));

  const rolled = rollupDay(env.DB.raw.prepare(`SELECT 1 AS n, day, version, package_format AS format, features FROM activity`).all().map(r => ({...r})), Math.floor(+now / 864e5));
  const get = (metric, key) => rolled.find(r => r.metric === metric && r.key === key)?.value;
  assert.equal(get('feature_known', 'activity_island'), 7);
  assert.equal(get('feature', 'activity_island=on'), 5);
  assert.equal(get('feature', 'widgets=clock'), 7);
  assert.equal(get('feature', 'theme_style=neon'), 2);
  assert.equal(get('feature_known', 'islands'), undefined);
});

test('a single snapshot is published with exact counts', () => {
  const f = featureSummary([{n: 1, day: 10, features: canonicalFeatures(features())}], 0, 20);
  assert.equal(f.reported, 1);
  assert.equal(f.switches.find(s => s.key === 'activity_island').on, 1);
  assert.equal(f.widgets.items.find(w => w.key === 'clock').on, 1);
  assert.deepEqual(f.theme_style.map(t => [t.key, t.count]), [['glass', 1]]);
  assert.equal(featureSummary([], 0, 20).switches, null, 'nothing reported, nothing shown');
});
