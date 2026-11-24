import test from 'node:test';
import assert from 'node:assert/strict';
import {journalDays, releaseReplay, snapshotState} from '../public/journal.js';

const days = Array.from({length: 8}, (_, i) => '2026-11-0' + (i + 1));
const data = {
  days, today: '2026-11-09', generated_at: '2026-11-09T22:17:00Z',
  releases: [{version: '2', date: days[6]}, {version: '1', date: days[1]}],
  by_segment: {deb: {active: [100, 1, 9, 2, 8, 3, 7, 4], new: days.map(() => 1), today: 0,
    versions: {names: ['2', '1', 'Not reported'], counts: {'2': [0,0,0,0,0,0,1,0], '1': [0,3,2,0,1,0,3,0], 'Not reported': days.map(() => 5)}}}},
};

test('journal uses previous complete days outside the visible range and keeps missing first reports distinct from zero', () => {
  const [last, today] = journalDays(data, 'deb', 2);
  assert.equal(last.median, 7);
  assert.equal(today.median, 4);
  assert.equal(today.active, 0);
  assert.equal(today.returning, null);
  const withFirst = structuredClone(data);
  withFirst.by_segment.deb.today_new = 0;
  assert.equal(journalDays(withFirst, 'deb', 1)[0].returning, 0);
  assert.deepEqual(journalDays(data, 'deb', 8).find(d => d.date === days[6]).releases, ['2']);
  assert.equal(journalDays(data, 'deb', 8)[0].median, null);
});

test('UTC midnight and overdue summaries are independent of the local calendar day', () => {
  assert.deepEqual(snapshotState(data, new Date('2026-11-10T00:01:00+01:00')), {current: true, delayed: false});
  assert.deepEqual(snapshotState(data, new Date('2026-11-10T01:01:00+01:00')), {current: false, delayed: false});
  assert.equal(snapshotState(data, new Date('2026-11-10T01:17:00+01:00')).delayed, true);
});

test('release replay aligns completed UTC days and excludes unknown versions from the denominator', () => {
  const frames = releaseReplay(data, 'deb', 7);
  assert.deepEqual(frames.map(f => f.age), [0, 1]);
  assert.deepEqual(frames[0].entries.map(e => [e.count,e.total]), [[1,4],[3,3]]);
  assert.equal(frames[1].entries[0].total, 0);
  assert.equal(releaseReplay(data, 'deb', 1).length, 1);
  assert.deepEqual(releaseReplay({...data, releases: data.releases.slice(1)}, 'deb', 7), []);
});

test('a first-day snapshot works without completed days or a previous release', () => {
  const fresh = {...data, days: [], releases: []};
  const [day] = journalDays(fresh, 'deb', 30);
  assert.equal(day.date, data.today);
  assert.equal(day.partial, true);
  assert.equal(day.median, null);
  assert.deepEqual(releaseReplay(fresh, 'deb', 30), []);
});
