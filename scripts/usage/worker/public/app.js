// Halcyon public dashboard. Plain JS and SVG; reads /api/summary.json only.
import {journalDays, releaseReplay, snapshotState} from './journal.js';
(() => {
'use strict';
const $ = id => document.getElementById(id);
const NS = 'http://www.w3.org/2000/svg';
const DAY = 864e5;
const parseDay = s => new Date(s + 'T00:00:00Z');
const fmtDay = s => parseDay(s).toLocaleDateString('en-GB', {day: 'numeric', month: 'short', timeZone: 'UTC'});
const fmt = n => n >= 10000 ? (n / 1000).toFixed(1).replace(/\.0$/, '') + 'K' : Math.round(n).toLocaleString('en-US');
const count = new Intl.NumberFormat('en-US').format;
const pct = v => (v * 100).toFixed(v > 0 && v < .1 ? 1 : 0) + '%';
const css = name => getComputedStyle(document.documentElement).getPropertyValue('--' + name).trim();
const el = (tag, attrs = {}, parent) => {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.appendChild(e);
  return e;
};
const txt = (parent, x, y, s, attrs = {}) => { const t = el('text', {x, y, ...attrs}, parent); t.textContent = s; return t; };
// A zero-length segment with a round cap keeps an isolated observation visible.
const linePoint = (x, y, start) => {
  const point = x.toFixed(1) + ',' + y.toFixed(1);
  return start ? 'M' + point + 'l0,0' : 'L' + point;
};
const html = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e; };
// Axis maximum whose quarter ticks are round numbers.
const niceMax = v => {
  const quarter = Math.max(1, v / 4), p = Math.pow(10, Math.floor(Math.log10(quarter)));
  for (const m of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (m * p >= quarter) return Number.isInteger(m * p) ? m * p * 4 : Math.ceil(m * p) * 4;
  return 40 * p;
};
const SLOTS = ['s1', 's2', 's3', 's4'];
const METHOD_SLOT = {deb: 's1', rpm: 's2', source: 's3', unknown: 'gray'};

let data = null;
let journalDate = null; // Follow the latest snapshot until a historical day is selected.
let refreshJournalDate;
let replayAge = null;
const state = {range: 30, segment: 'all'};
const tableMode = {};
const seg = () => data.by_segment[state.segment] || data.by_segment.all;

// Tooltip
const tip = $('tip');
function showTip(x, y, title, rows) {
  tip.replaceChildren(html('div', 't', title));
  for (const r of rows) {
    const row = html('div', 'row');
    const k = html('span', 'k');
    if (r.color) { const lk = html('i', 'lk'); lk.style.background = r.color; k.appendChild(lk); }
    k.appendChild(document.createTextNode(r.label));
    row.append(k, html('b', '', r.value));
    tip.appendChild(row);
  }
  tip.style.display = 'block';
  const w = tip.offsetWidth, h = tip.offsetHeight;
  tip.style.left = Math.max(8, Math.min(window.innerWidth - w - 8, x + 14)) + 'px';
  tip.style.top = Math.max(8, y - h - 12) + 'px';
}
const hideTip = () => { tip.style.display = 'none'; };
function hover(target, show) {
  const on = e => { target.style.opacity = .85; const b = target.getBoundingClientRect(); show(e && e.clientX || b.left + 8, e && e.clientY || b.top); };
  const off = () => { target.style.opacity = 1; hideTip(); };
  target.addEventListener('pointermove', on);
  target.addEventListener('focus', () => on());
  target.addEventListener('pointerleave', off);
  target.addEventListener('blur', off);
}

function table(head, rows) {
  const wrap = html('div', 'tablewrap'), t = html('table'), thead = html('thead'), tr = html('tr'), tb = html('tbody');
  head.forEach(h => tr.appendChild(html('th', '', h)));
  thead.appendChild(tr);
  rows.forEach(r => { const row = html('tr'); r.forEach(c => row.appendChild(html('td', '', String(c)))); tb.appendChild(row); });
  t.append(thead, tb);
  wrap.appendChild(t);
  return wrap;
}
const empty = (body, text) => body.appendChild(html('p', 'empty', text));

// Range of completed days currently shown.
function view() {
  const days = data.days, n = Math.min(state.range, days.length);
  return {days: days.slice(days.length - n), from: days.length - n};
}

function timeChart(body, opts) {
  const {days, height, label, stacked, series, right = 12} = opts;
  const W = Math.max(200, body.clientWidth), H = height, narrow = W < 520;
  const svg = el('svg', {width: W, height: H, role: 'img', 'aria-label': label}, body);
  // Stacked charts name each band at the right edge; leave room for the longest.
  const ends = stacked && !narrow ? series.map(ser => txt(svg, 0, -100, ser.name + ' 100%', {class: 'lbl'}).getComputedTextLength()) : [];
  ends.length && svg.replaceChildren();
  const m = {t: 30, r: stacked && narrow ? 12 : Math.max(right, ...ends.map(w => w + 12)), b: 24, l: 40};
  const iw = W - m.l - m.r, ih = H - m.t - m.b, n = days.length;
  const x = i => m.l + (n === 1 ? iw / 2 : i / (n - 1) * iw);
  const max = stacked ? 1 : niceMax(Math.max(1, ...series[0].values, ...(opts.medians || [])) * 1.08);
  const y = v => m.t + ih - v / max * ih;
  const ticks = [0, .25, .5, .75, 1].map(f => f * max);
  for (const v of ticks) {
    el('line', {x1: m.l, x2: m.l + iw, y1: y(v), y2: y(v), stroke: v === 0 ? css('axis') : css('grid'), 'stroke-width': 1}, svg);
    txt(svg, m.l - 6, y(v) + 3.5, stacked ? Math.round(v * 100) + '%' : fmt(v), {'text-anchor': 'end'});
  }
  const step = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(iw / 80))));
  const dateLabel = i => fmtDay(days[i]) + (opts.partial && i === n - 1 ? ' · partial' : '');
  for (let i = n - 1; i >= 0; i -= step) txt(svg, x(i), H - 6, fmtDay(days[i]), {'text-anchor': i === n - 1 && n > 1 ? 'end' : 'middle'});
  // One marker per day: releases shipped the same day share a line and a label.
  const marks = new Map();
  for (const r of data.releases) {
    const i = days.indexOf(r.date);
    if (i >= 0) marks.set(i, [...(marks.get(i) || []), r.version]);
  }
  let free = W;
  for (const i of [...marks.keys()].sort((a, b) => b - a)) {
    el('line', {x1: x(i), x2: x(i), y1: m.t - 8, y2: m.t + ih, stroke: css('axis'), 'stroke-width': 1}, svg);
    const versions = marks.get(i).sort((a, b) => b.localeCompare(a, 'en', {numeric: true}));
    const t = txt(svg, x(i), m.t - 14, versions.join(', '), {class: 'lbl', 'text-anchor': 'middle'});
    const w = t.getComputedTextLength();
    // Keep the label inside the chart, and drop it rather than overlap a newer one.
    const cx = Math.min(Math.max(x(i), m.l + w / 2), Math.min(W - 2, free - 8) - w / 2);
    if (cx - w / 2 < 0 || Math.abs(cx - x(i)) > w / 2) { t.remove(); continue; }
    t.setAttribute('x', cx);
    free = cx - w / 2;
  }
  if (stacked) {
    const totals = days.map((_, i) => series.reduce((s, ser) => s + ser.values[i], 0));
    const share = (s, i) => totals[i] ? series[s].values[i] / totals[i] : 0;
    // Draw oldest at the bottom: series are listed newest first.
    for (let s = series.length - 1, lower = days.map(() => 0); s >= 0; s--) {
      let top = '', bot = '';
      days.forEach((_, i) => {
        const lo = lower[i], hi = lo + share(s, i);
        top += (i ? 'L' : 'M') + x(i).toFixed(1) + ',' + y(hi).toFixed(1);
        bot = 'L' + x(i).toFixed(1) + ',' + y(lo).toFixed(1) + bot;
        lower[i] = hi;
      });
      if (n === 1) { top += 'h1'; }
      el('path', {d: top + bot + 'Z', fill: css(series[s].slot), stroke: css('surface'), 'stroke-width': 1.5, 'stroke-linejoin': 'round'}, svg);
    }
    let lo = 0;
    for (let s = series.length - 1; s >= 0; s--) {
      const v = share(s, n - 1);
      if (v >= .08 && !narrow) txt(svg, m.l + iw + 6, y(lo + v / 2) + 4, series[s].name + ' ' + pct(v), {class: 'lbl'});
      lo += v;
    }
    opts.tipRows = i => series.map(ser => ({label: ser.name, value: totals[i] ? pct(ser.values[i] / totals[i]) : 'n/a', color: css(ser.slot)}));
  } else {
    const values = series[0].values;
    let line = '';
    const completed = opts.partial ? n - 1 : n;
    for (let i = 0; i < completed; i++) line += (i ? 'L' : 'M') + x(i).toFixed(1) + ',' + y(values[i]).toFixed(1);
    if (completed > 1) {
      el('path', {d: line + `L${x(completed - 1)},${y(0)}L${x(0)},${y(0)}Z`, fill: css('s1'), 'fill-opacity': .1}, svg);
      el('path', {d: line, fill: 'none', stroke: css('s1'), 'stroke-width': 2, 'stroke-linejoin': 'round'}, svg);
    }
    if (opts.medians) {
      let reference = '';
      opts.medians.forEach((value, i) => {
        if (value == null) return;
        const start = !i || opts.medians[i - 1] == null;
        reference += linePoint(x(i), y(value), start);
      });
      if (reference) el('path', {d: reference, fill: 'none', stroke: css('s2'), 'stroke-width': 1.7, 'stroke-linejoin': 'round', 'stroke-linecap': 'round'}, svg);
    }
    if (opts.partial && n > 1) el('line', {x1: x(n - 2), y1: y(values[n - 2]), x2: x(n - 1), y2: y(values[n - 1]), stroke: css('s1'), 'stroke-width': 2, 'stroke-dasharray': '4 4'}, svg);
    el('circle', {cx: x(n - 1), cy: y(values[n - 1]), r: 4, fill: css('s1'), stroke: css('surface'), 'stroke-width': 2}, svg);
    txt(svg, x(n - 1) - 8, y(values[n - 1]) - 10, fmt(values[n - 1]), {class: 'lbl-strong', 'text-anchor': 'end'});
    opts.tipRows = i => series.map((ser, k) => ({label: ser.name, value: ser.values[i] == null ? 'Pending' : count(ser.values[i]), color: k ? null : css('s1')}));
  }
  const cross = el('line', {y1: m.t, y2: m.t + ih, stroke: css('muted'), 'stroke-width': 1, visibility: 'hidden'}, svg);
  const dot = el('circle', {r: 4, fill: css('s1'), stroke: css('surface'), 'stroke-width': 2, visibility: 'hidden'}, svg);
  const hit = el('rect', {x: m.l, y: m.t, width: iw, height: ih, fill: 'transparent', tabindex: 0, 'aria-label': label + '. Use the arrow keys to read values.'}, svg);
  let cur = opts.selectedIndex ?? n - 1;
  const marker = opts.onSelect ? el('circle', {r: 6, fill: css('surface'), stroke: css('s1'), 'stroke-width': 3, 'pointer-events': 'none'}, svg) : null;
  const select = i => {
    cur = i;
    if (marker) {
      marker.setAttribute('cx', x(i));
      marker.setAttribute('cy', y(series[0].values[i]));
      opts.onSelect(i);
    }
  };
  select(cur);
  const show = (i, cx, cy) => {
    cross.setAttribute('x1', x(i)); cross.setAttribute('x2', x(i)); cross.setAttribute('visibility', 'visible');
    if (!stacked) { dot.setAttribute('cx', x(i)); dot.setAttribute('cy', y(series[0].values[i])); dot.setAttribute('visibility', 'visible'); }
    showTip(cx, cy, dateLabel(i), opts.tipRows(i));
  };
  const hide = () => { cross.setAttribute('visibility', 'hidden'); dot.setAttribute('visibility', 'hidden'); hideTip(); };
  const index = clientX => { const r = svg.getBoundingClientRect(); return Math.max(0, Math.min(n - 1, Math.round((clientX - r.left - m.l) / iw * (n - 1)))); };
  hit.addEventListener('pointermove', e => show(index(e.clientX), e.clientX, e.clientY));
  if (marker) hit.addEventListener('click', e => select(index(e.clientX)));
  hit.addEventListener('pointerleave', hide);
  hit.addEventListener('blur', hide);
  const keyShow = i => { const r = svg.getBoundingClientRect(); show(i, r.left + x(i), r.top + m.t + 20); };
  hit.addEventListener('focus', () => keyShow(cur));
  hit.addEventListener('keydown', e => {
    const moves = {ArrowLeft: cur - 1, ArrowRight: cur + 1, Home: 0, End: n - 1};
    if (!Object.hasOwn(moves, e.key)) return;
    e.preventDefault();
    select(Math.max(0, Math.min(n - 1, moves[e.key])));
    keyShow(cur);
  });
  return select;
}

const releaseAgeLabel = age => age === 0 ? 'Launch day' : 'Day ' + age;
const releaseShare = entry => entry.total
  ? pct(entry.count / entry.total) + ' · ' + count(entry.count) + ' of ' + count(entry.total)
  : 'No known versions';

function releaseChart(body, frames) {
  const width = body.clientWidth, left = 36, right = width - 10, top = 12, bottom = 86;
  const first = frames[0].age, last = frames.at(-1).age;
  const x = age => first === last ? (left + right) / 2 : left + (right - left) * (age - first) / (last - first);
  const y = share => bottom - share * (bottom - top);
  const svg = el('svg', {width, height: 107, role: 'img', 'aria-label': 'Daily share of installations on each release by days since launch'}, body);
  for (const share of [0, .5, 1]) {
    el('line', {x1: left, x2: right, y1: y(share), y2: y(share), stroke: css('grid')}, svg);
    txt(svg, left - 6, y(share) + 3, pct(share), {'text-anchor': 'end'});
  }
  txt(svg, x(first), 103, releaseAgeLabel(first), {'text-anchor': first === last ? 'middle' : 'start'});
  if (first !== last) txt(svg, right, 103, releaseAgeLabel(last), {'text-anchor': 'end'});
  const colors = [css('s1'), css('s3')];
  colors.forEach((color, k) => {
    let line = '', previousAge;
    for (const {age, entries} of frames) {
      const entry = entries[k];
      if (!entry.total) continue;
      const start = previousAge !== age - 1;
      line += linePoint(x(age), y(entry.count / entry.total), start);
      previousAge = age;
    }
    if (line) el('path', {d: line, fill: 'none', stroke: color, 'stroke-width': 2, 'stroke-linejoin': 'round', 'stroke-linecap': 'round', 'stroke-dasharray': k ? '4 4' : 'none'}, svg);
  });
  const cursor = el('line', {y1: top, y2: bottom, stroke: css('ink-2'), 'stroke-opacity': .65}, svg);
  const dots = colors.map(color => el('circle', {r: 4, fill: css('surface'), stroke: color, 'stroke-width': 2}, svg));
  return frame => {
    cursor.setAttribute('x1', x(frame.age));
    cursor.setAttribute('x2', x(frame.age));
    frame.entries.forEach((entry, k) => {
      dots[k].setAttribute('visibility', entry.total ? 'visible' : 'hidden');
      if (entry.total) {
        dots[k].setAttribute('cx', x(frame.age));
        dots[k].setAttribute('cy', y(entry.count / entry.total));
      }
    });
  };
}

function hbars(body, rows, label, valueText) {
  const W = Math.max(260, body.clientWidth), rowH = 30, labelW = Math.min(150, W * .36), right = 92;
  const svg = el('svg', {width: W, height: rows.length * rowH + 4, role: 'img', 'aria-label': label}, body);
  const max = Math.max(1, ...rows.map(r => r.value)), bw = W - labelW - right;
  rows.forEach((r, k) => {
    const yy = k * rowH, h = 16, w = Math.max(2, r.value / max * bw), y0 = yy + (rowH - h) / 2;
    txt(svg, 0, yy + rowH / 2 + 4, r.name, {class: 'lbl'});
    const g = el('g', {tabindex: 0, 'aria-label': r.name + ': ' + valueText(r)}, svg);
    el('rect', {x: labelW, y: yy + 2, width: bw + right, height: rowH - 4, fill: 'transparent'}, g);
    el('path', {d: w > 4 ? `M${labelW},${y0}h${w - 4}a4,4 0 0 1 4,4v${h - 8}a4,4 0 0 1 -4,4h${-(w - 4)}Z` : `M${labelW},${y0}h${w}v${h}h${-w}Z`, fill: r.muted ? css('gray') : css('s1')}, g);
    txt(svg, labelW + w + 6, yy + rowH / 2 + 4, valueText(r), {class: 'lbl-strong'});
    hover(g, (cx, cy) => showTip(cx, cy, r.name, [{label: r.tipLabel || 'Installations', value: valueText(r)}]));
  });
}

function segbar(body, items, label) {
  const total = items.reduce((n, i) => n + i.count, 0);
  const bar = html('div', 'segbar');
  bar.setAttribute('role', 'img');
  bar.setAttribute('aria-label', label + ': ' + items.map(i => `${i.name} ${pct(i.count / total)}`).join(', '));
  for (const i of items) {
    const d = html('div');
    d.style.flex = String(i.count);
    d.style.background = css(i.slot);
    if (i.slot === 's4') d.classList.add('dark-ink');
    if (i.slot === 'gray') d.style.color = css('ink');
    if (i.count / total >= .1) d.textContent = pct(i.count / total);
    d.tabIndex = 0;
    hover(d, (cx, cy) => showTip(cx, cy, i.name, [{label: 'Installations', value: fmt(i.count)}, {label: 'Share', value: pct(i.count / total)}]));
    bar.appendChild(d);
  }
  body.appendChild(bar);
  const list = html('div', 'seglist');
  for (const i of items) {
    const k = html('i', 'key-rect');
    k.style.background = css(i.slot);
    list.append(k, html('span', '', i.name), html('span', 'n', fmt(i.count)), html('span', 'p', pct(i.count / total)));
  }
  body.appendChild(list);
}

// Feature shares, out of the installations whose setting could be read.
const shareRow = r => ({name: r.name, on: r.on, known: r.known, value: r.known ? r.on / r.known : 0,
  muted: !r.known, tipLabel: 'Installations'});
const shareText = r => r.known ? `${pct(r.value)} · ${fmt(r.on)} of ${fmt(r.known)}` : 'not reported';
const shareCells = r => [r.name, r.known ? fmt(r.on) : 'not reported', r.known ? pct(r.value) : ''];
const featureEmpty = body => empty(body, `Shown once an installation on ${data.since_version || 'a current version'} reports its settings.`);

// Use, problems and setup: installations with each key, and how many times.
const countRow = r => ({name: r.name, value: r.installations, count: r.count});
const countText = r => `${fmt(r.value)} · ${fmt(r.count)}×`;
const countCells = r => [r.name, fmt(r.value), fmt(r.count)];
const detailsEmpty = body => empty(body, 'Shown once an installation on a version that sends these details reports.');

function kpis() {
  const s = seg(), w = s.windows[state.range];
  const tiles = [];
  const delta = (cur, prev) => prev ? cur / prev - 1 : null;
  const partial = !w.complete && data.history_start ? `since ${fmtDay(data.history_start)}, when reporting began` : null;
  const currentDay = new Date().toISOString().slice(0, 10) === data.today;
  const allMethods = state.segment === 'all' ? '' : ' · all install methods';
  tiles.push({label: `Active installations, last ${state.range} days`, value: fmt(w.active),
    delta: delta(w.active, w.previous), vs: `previous ${state.range} days`, note: partial || 'not enough history to compare yet'});
  const lastDay = data.days.at(-1);
  tiles.push({label: currentDay ? 'Active yesterday' : 'Last completed day', value: s.yesterday === null ? 'n/a' : fmt(s.yesterday),
    delta: s.week_earlier ? delta(s.yesterday, s.week_earlier) : null, vs: 'the same day a week earlier', note: lastDay ? `${fmtDay(lastDay)} · full UTC day` : 'No completed day yet'});
  tiles.push({label: `New, last ${state.range} days`, value: fmt(w.new), delta: delta(w.new, w.previous_new), vs: `previous ${state.range} days`,
    note: 'first report received in this period'});
  tiles.push({label: 'Still active after 4 weeks', value: data.retention_4w === null ? 'n/a' : pct(data.retention_4w), delta: null,
    note: (data.retention_4w === null ? 'needs five complete weeks of data' : 'of new installations, reporting in their fifth week') + allMethods});
  const baseline = data.latest_release && data.latest_release.version === data.since_version;
  tiles.push({label: 'On the latest version', value: s.latest_share === null ? 'n/a' : pct(s.latest_share), delta: null,
    note: baseline ? `counting starts at ${data.since_version}; shown from the next release`
      : data.latest_release ? `${data.latest_release.version}, released ${fmtDay(data.latest_release.date)}; last 7 days` : 'no release data yet'});
  const sharing = data.sharing, seen = sharing ? sharing.on + sharing.off : 0;
  tiles.push({label: 'Turned sharing off', value: seen ? pct(sharing.off / seen) : 'n/a', delta: null,
    note: (seen ? `${fmt(sharing.off)} of ${fmt(seen)} installations seen in 90 days` : 'no choices received yet') + allMethods});
  const box = $('kpis');
  box.replaceChildren();
  for (const t of tiles) {
    const c = html('div', 'card tile');
    const d = html('div', 'delta');
    if (t.delta !== null && Number.isFinite(t.delta)) {
      const b = html('b', t.delta >= 0 ? 'up' : 'down', (t.delta >= 0 ? '▲ +' : '▼ ') + (t.delta * 100).toFixed(0) + '%');
      d.append(b, document.createTextNode(' vs ' + t.vs));
    } else d.textContent = t.note;
    c.append(html('div', 'label', t.label), html('div', 'value', t.value), d);
    box.appendChild(c);
  }
}

const renderers = {
  'c-active'(body) {
    const days = journalDays(data, state.segment, state.range);
    const latest = days.length - 1;
    const found = days.findIndex(day => day.date === journalDate);
    const selected = found < 0 ? latest : found;
    const picker = $('journal-date');
    picker.replaceChildren();
    for (let i = latest; i >= 0; i--) {
      const day = days[i], releases = day.releases.map(version => 'v' + version).join(', ');
      const option = html('option', '', fmtDay(day.date) + (day.partial ? ' · partial' : '') + (releases ? ' · ' + releases : ''));
      option.value = day.date;
      picker.appendChild(option);
    }
    picker.value = days[selected].date;
    if (tableMode['c-active']) {
      refreshJournalDate = null;
      const wrap = table(['Date (UTC)', 'Active', 'First reports', 'Returning', 'Prior 7-day median', 'Released'],
        days.slice().reverse().map(d => [fmtDay(d.date) + (d.partial ? ' · partial' : ''), count(d.active),
          d.first == null ? 'Pending' : count(d.first), d.returning == null ? 'Pending' : count(d.returning),
          d.median == null ? '—' : count(d.median), d.releases.join(', ')]));
      body.appendChild(wrap);
      const rows = wrap.querySelectorAll('tbody tr');
      const highlight = () => {
        const i = picker.selectedIndex, day = days[latest - i];
        journalDate = day.partial ? null : day.date;
        wrap.querySelector('[aria-current]')?.removeAttribute('aria-current');
        rows[i].setAttribute('aria-current', 'true');
        const top = rows[i].getBoundingClientRect().top - wrap.getBoundingClientRect().top;
        if (top < 0 || top + rows[i].offsetHeight > wrap.clientHeight) wrap.scrollTop += top - wrap.clientHeight / 2;
      };
      picker.onchange = highlight;
      highlight();
      return;
    }
    const spread = html('div', 'journal-spread'), detail = html('div', 'journal-detail'), chart = html('div', 'journal-chart');
    body.appendChild(spread); spread.append(detail, chart);
    const dateLine = html('div', 'journal-date'), total = html('strong', 'journal-total'), mix = html('div', 'journal-mix');
    const comparison = html('div', 'journal-comparison'), release = html('div', 'journal-release');
    detail.append(dateLine, total, html('div', 'journal-unit', 'active installations'), mix, comparison, release);
    const cutoff = new Date(data.generated_at).toLocaleTimeString('en-GB', {hour: '2-digit', minute: '2-digit', timeZone: 'UTC'});
    const selectDay = i => {
      const d = days[i];
      journalDate = d.partial ? null : d.date;
      picker.value = d.date;
      refreshJournalDate = () => {
        const {current, delayed} = snapshotState(data);
        dateLine.textContent = d.partial
          ? (current ? 'Today' : fmtDay(d.date)) + ' through ' + cutoff + ' UTC'
          : fmtDay(d.date) + ' · complete UTC day';
        if (d.partial && delayed) dateLine.textContent += ' · update overdue';
      };
      refreshJournalDate();
      total.textContent = fmt(d.active);
      mix.textContent = d.first == null ? 'First-report count pending' : count(d.first) + ' new · ' + count(d.returning) + ' returning';
      comparison.textContent = d.median == null ? 'Typical activity appears after seven complete days.'
        : count(d.median) + ' active on a typical day';
      comparison.title = 'Median of the previous seven complete UTC days';
      release.textContent = d.releases.length ? 'Released ' + d.releases.map(v => 'v' + v).join(', ') : '';
    };
    const choose = timeChart(chart, {days: days.map(d => d.date), height: 205, selectedIndex: selected,
      onSelect: selectDay, label: 'Daily active installations. Select a point to inspect that day.', partial: true, medians: days.map(d => d.median),
      series: [{name: 'Active', values: days.map(d => d.active)}, {name: 'New', values: days.map(d => d.first)}]});
    picker.onchange = () => choose(days.findIndex(d => d.date === picker.value));
    const legend = html('div', 'journal-legend');
    legend.appendChild(html('span', 'active-key', 'Daily active'));
    if (days.some(d => d.median != null)) legend.appendChild(html('span', 'median-key', 'Previous 7-day median'));
    legend.appendChild(html('span', 'partial-key', 'Partial day'));
    chart.appendChild(legend);
  },
  'c-versions'(body) {
    const {days, from} = view(), s = seg();
    if (!days.length || !s.versions.names.length) return empty(body, 'Not enough data yet.');
    // Only versions present in the selected range; the four newest get colors.
    const inRange = n => s.versions.counts[n].slice(from).some(Boolean);
    const names = s.versions.names.filter(inRange), named = names.filter(n => /^\d/.test(n)).slice(0, 4);
    if (!names.length) return empty(body, 'No version reports for this install method in the selected period.');
    const series = named.map((name, i) => ({name, slot: SLOTS[i], values: s.versions.counts[name].slice(from)}));
    const rest = names.filter(n => !named.includes(n));
    if (rest.length) series.push({name: rest.length === 1 ? rest[0] : 'Older or not reported', slot: 'gray',
      values: days.map((_, i) => rest.reduce((n, r) => n + s.versions.counts[r][from + i], 0))});
    if (tableMode['c-versions']) {
      return body.appendChild(table(['Date', ...series.map(r => r.name)], days.map((d, i) => {
        const total = series.reduce((n, r) => n + r.values[i], 0);
        return [fmtDay(d), ...series.map(r => total ? pct(r.values[i] / total) : 'n/a')];
      }).reverse()));
    }
    const lg = html('div', 'legend');
    for (const r of series) { const sp = html('span'), k = html('i', 'key-rect'); k.style.background = css(r.slot); sp.append(k, document.createTextNode(r.name)); lg.appendChild(sp); }
    body.appendChild(lg);
    timeChart(body, {days, height: 240, stacked: true, right: 116, label: 'Version share over time', series});
  },
  'c-speed'(body) {
    const s = seg();
    const releases = data.releases.filter(r => !r.baseline);
    const timeToHalf = days => days == null ? 'Not yet' : days === 0 ? 'Launch day' : days + (days === 1 ? ' day' : ' days');
    const frames = releaseReplay(data, state.segment, state.range);
    if (tableMode['c-speed']) {
      if (!releases.length && !s.updates) return empty(body, 'Release and update counts will appear here as reports arrive.');
      if (frames.length) body.appendChild(table(['Age', ...frames[0].entries.map(e => 'v' + e.version)], frames.map(frame =>
        [releaseAgeLabel(frame.age), ...frame.entries.map(e => fmtDay(e.date) + ' · ' + releaseShare(e))])));
      if (releases.length) body.appendChild(table(['Release (all methods)', 'Days until half ran this or newer'],
        releases.map(r => ['v' + r.version + ' · ' + fmtDay(r.date), timeToHalf(r.days_to_half)])));
      if (s.updates) body.appendChild(table(['Package installs', 'Installations'], [['Signed updates on', fmt(s.updates.on)], ['Updates off, manual or not reported', fmt(s.updates.other)]]));
      return;
    }
    if (frames.length) {
      const mark = releaseChart(body, frames);
      const control = html('label', 'replay-control'), age = html('strong'), slider = html('input');
      slider.type = 'range'; slider.min = 0; slider.max = frames.length - 1;
      slider.disabled = frames.length === 1;
      const selected = frames.findIndex(frame => frame.age === replayAge);
      slider.value = selected < 0 ? frames.length - 1 : selected;
      slider.setAttribute('aria-label', 'Days since release');
      control.append(age, slider);
      const rows = html('div', 'replay-rows');
      const values = frames[0].entries.map(entry => {
        const row = html('div', 'replay-row'), date = html('span'), share = html('b');
        row.append(html('strong', '', 'v' + entry.version), date, share);
        rows.appendChild(row);
        return {date, share};
      });
      const draw = () => {
        const frame = frames[Number(slider.value)];
        age.textContent = releaseAgeLabel(frame.age);
        slider.setAttribute('aria-valuetext', age.textContent + '. ' + frame.entries.map(e => 'v' + e.version + ', ' + releaseShare(e)).join('. '));
        mark(frame);
        frame.entries.forEach((entry, k) => {
          values[k].date.textContent = fmtDay(entry.date);
          values[k].share.textContent = releaseShare(entry);
        });
      };
      slider.addEventListener('input', () => { replayAge = frames[Number(slider.value)].age; draw(); });
      body.append(control, rows); draw();
    } else empty(body, 'A comparison appears when both releases have a completed reporting day.');
    const latest = releases[0];
    if (latest?.days_to_half != null) {
      const when = latest.days_to_half === 0 ? 'on launch day' : 'after ' + timeToHalf(latest.days_to_half);
      body.appendChild(html('div', 'note replay-milestone', 'Across all install methods, half of installations reporting a version ran v' + latest.version + ' or newer ' + when + '.'));
    }
    if (s.updates) {
      const sub = html('div', 'note', 'Signed updates through APT and DNF, DEB and RPM installs');
      sub.classList.add('spaced');
      body.appendChild(sub);
      segbar(body, [{name: 'Updates on', count: s.updates.on, slot: 's1'}, {name: 'Off, manual or not reported', count: s.updates.other, slot: 'gray'}].filter(i => i.count), 'Signed updates');
    }
  },
  'c-platforms'(body) {
    const items = seg().install_methods.map(m => ({...m, slot: METHOD_SLOT[m.key] || 'gray'}));
    if (!items.length) return empty(body, 'Not enough data yet.');
    if (tableMode['c-platforms']) {
      const total = items.reduce((n, i) => n + i.count, 0);
      return body.appendChild(table(['Install method', 'Installations', 'Share'], items.map(i => [i.name, fmt(i.count), pct(i.count / total)])));
    }
    segbar(body, items, 'Install method');
  },
  'c-usage'(body) {
    const s = seg(), total = s.frequency.reduce((n, b) => n + b.count, 0);
    if (!total) return empty(body, 'Not enough data yet.');
    const rows = s.frequency.map(b => ({name: b.name, value: b.count}));
    const text = r => `${fmt(r.value)} · ${pct(r.value / total)}`;
    if (tableMode['c-usage']) return body.appendChild(table(['Active days', 'Installations', 'Share'], rows.map(r => [r.name, fmt(r.value), pct(r.value / total)])));
    hbars(body, rows, 'Installations by active days in the last 28 days', text);
    if (data.history_start && parseDay(data.today) - parseDay(data.history_start) < 27 * DAY)
      body.appendChild(html('div', 'note', `Reporting began ${fmtDay(data.history_start)}, so fewer than 28 days are possible so far.`));
  },
  'c-countries'(body) {
    const rows = seg().countries.map(c => ({name: c.name, value: c.count, muted: c.key === 'unknown'}));
    if (!rows.length) return empty(body, 'Not enough data yet.');
    if (tableMode['c-countries']) return body.appendChild(table(['Country', 'Installations'], rows.map(r => [r.name, fmt(r.value)])));
    hbars(body, rows, 'Installations by country', r => fmt(r.value));
  },
  'c-features'(body) {
    const f = data.features;
    if (!f || !f.switches) return featureEmpty(body);
    const rows = f.switches.map(shareRow);
    if (tableMode['c-features']) return body.appendChild(table(['Feature', 'On', 'Share'], rows.map(shareCells)));
    hbars(body, rows, 'Share of installations with each feature on', shareText);
  },
  'c-widgets'(body) {
    const f = data.features;
    if (!f || !f.widgets) return featureEmpty(body);
    if (!f.widgets.known) return empty(body, 'No installation could report its widgets yet.');
    const rows = f.widgets.items.map(w => shareRow({...w, known: f.widgets.known})).sort((a, b) => b.value - a.value);
    if (tableMode['c-widgets']) return body.appendChild(table(['Widget', 'Showing', 'Share'], rows.map(shareCells)));
    hbars(body, rows, 'Share of installations showing each widget', shareText);
  },
  'c-looks'(body) {
    const f = data.features;
    if (!f || !f.switches) return featureEmpty(body);
    const parts = [['Theme style', f.theme_style], ['Wallpaper effect', f.wallpaper_effect]].filter(([, items]) => items && items.length);
    if (!parts.length) return empty(body, 'Not enough data yet.');
    if (tableMode['c-looks']) {
      for (const [title, items] of parts) {
        const total = items.reduce((n, i) => n + i.count, 0);
        body.appendChild(table([title, 'Installations', 'Share'], items.map(i => [i.name, fmt(i.count), pct(i.count / total)])));
      }
      return;
    }
    parts.forEach(([title, items], k) => {
      const head = html('div', 'note', title);
      if (k) head.classList.add('spaced');
      body.appendChild(head);
      let slot = 0;
      segbar(body, items.map(i => ({...i, slot: i.key === 'unknown' || slot >= SLOTS.length ? 'gray' : SLOTS[slot++]})), title);
    });
  },
  'c-pages'(body) {
    const d = data.details;
    if (!d || !d.pages) return detailsEmpty(body);
    if (!d.pages.length) return empty(body, 'No Settings page was opened in this period.');
    const rows = d.pages.map(countRow);
    if (tableMode['c-pages']) return body.appendChild(table(['Page', 'Installations', 'Times opened'], rows.map(countCells)));
    hbars(body, rows, 'Installations that opened each Settings page', countText);
  },
  'c-problems'(body) {
    const d = data.details;
    if (!d || !d.problems) return detailsEmpty(body);
    const p = d.problems;
    body.appendChild(html('div', 'note', `${fmt(p.clean)} of ${fmt(d.reported)} installations reported no problems.`));
    if (!p.items.length) return;
    const rows = p.items.map(countRow);
    if (tableMode['c-problems']) return body.appendChild(table(['Problem', 'Installations', 'Times'], rows.map(countCells)));
    hbars(body, rows, 'Installations that hit each problem', countText);
  },
  'c-setup'(body) {
    const d = data.details;
    if (!d || !d.setup) return detailsEmpty(body);
    const s = d.setup;
    if (!s.started) return empty(body, 'No installation opened setup in this period.');
    const steps = s.steps.map(r => ({name: r.name, value: r.installations, skipped: r.skipped}));
    const outcomes = s.outcomes.filter(o => o.installations).map(o => ({name: o.name, value: o.installations}));
    const stepText = r => `${fmt(r.value)} · ${pct(r.value / s.started)}${r.skipped ? ` · ${fmt(r.skipped)} skipped` : ''}`;
    if (tableMode['c-setup']) {
      body.appendChild(table(['Step', 'Reached', 'Share', 'Skipped'], steps.map(r => [r.name, fmt(r.value), pct(r.value / s.started), fmt(r.skipped)])));
      return body.appendChild(table(['How it ended', 'Installations'], outcomes.map(r => [r.name, fmt(r.value)])));
    }
    body.appendChild(html('div', 'note', `${fmt(s.started)} installations opened setup.`));
    hbars(body, steps, 'Installations that reached each setup step', stepText);
    if (!outcomes.length) return;
    body.appendChild(html('div', 'note spaced', 'How it ended'));
    hbars(body, outcomes, 'How setup ended', r => fmt(r.value));
  },
  'c-system'(body) {
    const d = data.details;
    if (!d || !d.system) return detailsEmpty(body);
    const sys = d.system;
    const parts = [['Distribution', sys.distro], ['COSMIC', sys.cosmic], ['Graphics', sys.gpu], ['Displays', sys.displays],
      ['Largest display', sys.resolution]].filter(([, items]) => items && items.length);
    const runtime = d.runtime;
    if (tableMode['c-system']) {
      for (const [title, items] of parts) body.appendChild(table([title, 'Installations'], items.map(i => [i.name, fmt(i.count)])));
      if (runtime) body.appendChild(table(['Hours running per report', 'Reports'], runtime.buckets.map(b => [b.name, fmt(b.count)])));
      return;
    }
    parts.forEach(([title, items], k) => {
      const head = html('div', 'note', title === 'Graphics' ? `${title} (a computer can have more than one)` : title);
      if (k) head.classList.add('spaced');
      body.appendChild(head);
      hbars(body, items.map(i => ({name: i.name, value: i.count, muted: i.key === 'unknown'})), title, r => fmt(r.value));
    });
    if (runtime) {
      body.appendChild(html('div', 'note spaced', `Hours running per report (median ${runtime.median_hours.toLocaleString('en-US')})`));
      hbars(body, runtime.buckets.map(b => ({name: b.name, value: b.count, tipLabel: 'Reports'})), 'Hours running per report', r => fmt(r.value));
    }
  },
  'c-retention'(body) {
    const cohorts = data.cohorts;
    if (!cohorts.length) return empty(body, 'The first complete week appears here after it ends.');
    const weeks = Math.max(...cohorts.map(c => c.rates.length));
    if (tableMode['c-retention']) {
      return body.appendChild(table(['Week starting', 'New', ...Array.from({length: weeks}, (_, w) => 'Week ' + w)],
        cohorts.map(c => [fmtDay(c.start), fmt(c.size),
          ...Array.from({length: weeks}, (_, w) => c.rates[w] === undefined || c.rates[w] === null ? '' : pct(c.rates[w]))])));
    }
    const W = Math.max(280, body.clientWidth), m = {l: W < 500 ? 96 : 128, t: 22}, gap = 2;
    const cw = Math.max(26, Math.min(64, (W - m.l) / Math.max(weeks, 6) - gap)), ch = 24, narrow = cw < 44;
    const svg = el('svg', {width: Math.max(W, m.l + weeks * (cw + gap)), height: m.t + cohorts.length * (ch + gap) + 4, role: 'img', 'aria-label': 'Weekly retention by first week'}, body);
    body.classList.add('scroll-x');
    for (let w = 0; w < weeks; w++) txt(svg, m.l + w * (cw + gap) + cw / 2, 14, narrow ? String(w) : (w === 0 ? 'Week 0' : 'W' + w), {'text-anchor': 'middle'});
    const steps = ['seq-0', 'seq-1', 'seq-2', 'seq-3', 'seq-4', 'seq-5', 'seq-6'];
    const colorFor = v => css(steps[Math.max(0, Math.min(6, Math.floor(v * 7 - 1e-9)))]);
    cohorts.forEach((c, r) => {
      const yy = m.t + r * (ch + gap);
      txt(svg, 0, yy + ch / 2 + 4, fmtDay(c.start), {class: 'lbl'});
      txt(svg, m.l - 10, yy + ch / 2 + 4, fmt(c.size), {'text-anchor': 'end'});
      c.rates.forEach((v, w) => {
        const g = el('g', {tabindex: 0, 'aria-label': `Started week of ${fmtDay(c.start)}, week ${w}: ${pct(v)}`}, svg);
        el('rect', {x: m.l + w * (cw + gap), y: yy, width: cw, height: ch, rx: 4, fill: colorFor(v)}, g);
        hover(g, (cx, cy) => showTip(cx, cy, `Started week of ${fmtDay(c.start)}`, [{label: `Reporting in week ${w}`, value: pct(v)}, {label: 'Started', value: fmt(c.size)}]));
      });
    });
    const sc = html('div', 'hm-scale');
    sc.appendChild(document.createTextNode('0%'));
    for (const s of steps) { const i = html('i'); i.style.background = css(s); sc.appendChild(i); }
    sc.append(document.createTextNode('100%'), html('span', 'hint', 'Columns are weeks after the first report. The number beside each date is how many started that week.'));
    body.appendChild(sc);
  },
};

function renderAll() {
  if (!data) return;
  kpis();
  for (const [id, fn] of Object.entries(renderers)) {
    const card = $(id), body = card.querySelector('.body');
    body.replaceChildren();
    fn(body);
    card.querySelector('.tbl-toggle').textContent = tableMode[id] ? 'Chart' : 'Table';
  }
}

const rangeButtons = [...document.querySelectorAll('#range button')];
function updateFilters() {
  const select = $('segment');
  if (!data.by_segment[state.segment]) state.segment = 'all';
  select.replaceChildren();
  for (const s of data.segments) { const o = html('option', '', s.name); o.value = s.id; select.appendChild(o); }
  select.value = state.segment;
  select.hidden = data.segments.length < 2;
  const updated = new Date(data.generated_at);
  $('sub').textContent = `${data.since_version ? `Halcyon ${data.since_version} and newer` : 'Installations that share usage data'} · updated hourly, last at ${updated.toLocaleString('en-GB', {day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', timeZone: 'UTC'})} UTC`;
  $('history').textContent = data.history_start ? `Daily history starts ${fmtDay(data.history_start)}` : '';
  const covers = Math.min(...rangeButtons.map(b => Number(b.dataset.v)).filter(v => v >= data.days.length + 1), Infinity);
  state.range = Math.min(state.range, covers);
  for (const b of rangeButtons) {
    b.disabled = Number(b.dataset.v) > covers;
    b.title = b.disabled && data.history_start ? `Reporting began ${fmtDay(data.history_start)}` : '';
    b.setAttribute('aria-pressed', String(Number(b.dataset.v) === state.range));
  }
}
$('segment').addEventListener('change', e => { state.segment = e.target.value; renderAll(); });
for (const b of rangeButtons) b.addEventListener('click', () => {
  state.range = Number(b.dataset.v);
  rangeButtons.forEach(other => other.setAttribute('aria-pressed', String(other === b)));
  renderAll();
});
document.querySelectorAll('.tbl-toggle').forEach(b => b.addEventListener('click', () => {
  const id = b.closest('.card').id;
  tableMode[id] = !tableMode[id];
  renderAll();
}));
let resizeTimer;
window.addEventListener('resize', () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(renderAll, 120); });

const themeBtn = $('theme');
const isDark = () => document.documentElement.dataset.theme
  ? document.documentElement.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
const syncTheme = () => { themeBtn.textContent = isDark() ? 'Light mode' : 'Dark mode'; };
themeBtn.addEventListener('click', () => {
  document.documentElement.dataset.theme = isDark() ? 'light' : 'dark';
  try { localStorage.setItem('theme', document.documentElement.dataset.theme); } catch (_) { /* optional */ }
  syncTheme();
  renderAll();
});
try { const saved = localStorage.getItem('theme'); if (saved === 'light' || saved === 'dark') document.documentElement.dataset.theme = saved; } catch (_) { /* optional */ }
syncTheme();
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { syncTheme(); renderAll(); });

let loading = false;
async function refresh() {
  if (loading) return;
  loading = true;
  try {
    const response = await fetch('/api/summary.json', {headers: {Accept: 'application/json'}});
    if (!response.ok) throw new Error(response.status === 503 ? 'The first numbers are being prepared. Check back within the hour.' : 'Could not load the numbers.');
    const summary = await response.json();
    if (!data || data.generated_at !== summary.generated_at) {
      data = summary;
      $('content').hidden = false;
      updateFilters();
      renderAll();
    } else { kpis(); refreshJournalDate?.(); }
    const {current, delayed} = snapshotState(data);
    const status = $('status');
    if (delayed) status.textContent = 'Hourly summary overdue. Showing counts from the last published update.';
    else if (!current) status.textContent = 'Waiting for the first summary of the new UTC day. The latest counts are for ' + fmtDay(data.today) + '.';
    else status.textContent = '';
    status.hidden = current && !delayed;
  } catch (error) {
    if (data) { kpis(); refreshJournalDate?.(); }
    $('status').textContent = data ? 'Could not refresh. Showing the last available update.' : error.message;
    $('status').hidden = false;
  } finally {
    loading = false;
  }
}
const refreshVisible = () => { if (!document.hidden) refresh(); };
document.addEventListener('visibilitychange', refreshVisible);
window.setInterval(refreshVisible, 5 * 60 * 1000);
refresh();
})();
