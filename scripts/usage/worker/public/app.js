// Halcyon public dashboard. Plain JS and SVG; reads /api/summary.json only.
(() => {
'use strict';
const $ = id => document.getElementById(id);
const NS = 'http://www.w3.org/2000/svg';
const DAY = 864e5;
const parseDay = s => new Date(s + 'T00:00:00Z');
const fmtDay = s => parseDay(s).toLocaleDateString('en-GB', {day: 'numeric', month: 'short', timeZone: 'UTC'});
const fmt = n => n >= 10000 ? (n / 1000).toFixed(1).replace(/\.0$/, '') + 'K' : Math.round(n).toLocaleString('en-US');
const pct = v => (v * 100).toFixed(v > 0 && v < .1 ? 1 : 0) + '%';
const css = name => getComputedStyle(document.documentElement).getPropertyValue('--' + name).trim();
const el = (tag, attrs = {}, parent) => {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.appendChild(e);
  return e;
};
const txt = (parent, x, y, s, attrs = {}) => { const t = el('text', {x, y, ...attrs}, parent); t.textContent = s; return t; };
const html = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e; };
// Axis maximum whose quarter ticks are round numbers.
const niceMax = v => {
  const quarter = Math.max(1, v / 4), p = Math.pow(10, Math.floor(Math.log10(quarter)));
  for (const m of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (m * p >= quarter) return Number.isInteger(m * p) ? m * p * 4 : Math.ceil(m * p) * 4;
  return 40 * p;
};
const SLOTS = ['s1', 's2', 's3', 's4'];
const METHOD_SLOT = {deb: 's1', rpm: 's2', source: 's3', unknown: 's4', other: 'gray'};

let data = null;
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
  const W = Math.max(280, body.clientWidth), H = height, narrow = W < 520;
  const m = {t: 30, r: stacked && narrow ? 12 : right, b: 24, l: 40};
  const svg = el('svg', {width: W, height: H, role: 'img', 'aria-label': label}, body);
  const iw = W - m.l - m.r, ih = H - m.t - m.b, n = days.length;
  const x = i => m.l + (n === 1 ? iw / 2 : i / (n - 1) * iw);
  const max = stacked ? 1 : niceMax(Math.max(1, ...series[0].values) * 1.08);
  const y = v => m.t + ih - v / max * ih;
  const ticks = [0, .25, .5, .75, 1].map(f => f * max);
  for (const v of ticks) {
    el('line', {x1: m.l, x2: m.l + iw, y1: y(v), y2: y(v), stroke: v === 0 ? css('axis') : css('grid'), 'stroke-width': 1}, svg);
    txt(svg, m.l - 6, y(v) + 3.5, stacked ? Math.round(v * 100) + '%' : fmt(v), {'text-anchor': 'end'});
  }
  const step = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(iw / 80))));
  for (let i = n - 1; i >= 0; i -= step) txt(svg, x(i), H - 6, fmtDay(days[i]), {'text-anchor': i === n - 1 && n > 1 ? 'end' : 'middle'});
  for (const r of data.releases) {
    const i = days.indexOf(r.date);
    if (i < 0) continue;
    el('line', {x1: x(i), x2: x(i), y1: m.t - 8, y2: m.t + ih, stroke: css('axis'), 'stroke-width': 1}, svg);
    txt(svg, x(i), m.t - 14, r.version, {class: 'lbl', 'text-anchor': 'middle'});
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
    values.forEach((v, i) => { line += (i ? 'L' : 'M') + x(i).toFixed(1) + ',' + y(v).toFixed(1); });
    if (n > 1) el('path', {d: line + `L${x(n - 1)},${y(0)}L${x(0)},${y(0)}Z`, fill: css('s1'), 'fill-opacity': .1}, svg);
    el('path', {d: line, fill: 'none', stroke: css('s1'), 'stroke-width': 2, 'stroke-linejoin': 'round', 'stroke-linecap': 'round'}, svg);
    el('circle', {cx: x(n - 1), cy: y(values[n - 1]), r: 4, fill: css('s1'), stroke: css('surface'), 'stroke-width': 2}, svg);
    txt(svg, x(n - 1) - 8, y(values[n - 1]) - 10, fmt(values[n - 1]), {class: 'lbl-strong', 'text-anchor': 'end'});
    opts.tipRows = i => series.map((ser, k) => ({label: ser.name, value: fmt(ser.values[i]), color: k ? null : css('s1')}));
  }
  const cross = el('line', {y1: m.t, y2: m.t + ih, stroke: css('muted'), 'stroke-width': 1, visibility: 'hidden'}, svg);
  const dot = el('circle', {r: 4, fill: css('s1'), stroke: css('surface'), 'stroke-width': 2, visibility: 'hidden'}, svg);
  const hit = el('rect', {x: m.l, y: m.t, width: iw, height: ih, fill: 'transparent', tabindex: 0, 'aria-label': label + '. Use the arrow keys to read values.'}, svg);
  let cur = n - 1;
  const show = (i, cx, cy) => {
    cur = i;
    cross.setAttribute('x1', x(i)); cross.setAttribute('x2', x(i)); cross.setAttribute('visibility', 'visible');
    if (!stacked) { dot.setAttribute('cx', x(i)); dot.setAttribute('cy', y(series[0].values[i])); dot.setAttribute('visibility', 'visible'); }
    showTip(cx, cy, fmtDay(days[i]), opts.tipRows(i));
  };
  const hide = () => { cross.setAttribute('visibility', 'hidden'); dot.setAttribute('visibility', 'hidden'); hideTip(); };
  const index = clientX => { const r = svg.getBoundingClientRect(); return Math.max(0, Math.min(n - 1, Math.round((clientX - r.left - m.l) / iw * (n - 1)))); };
  hit.addEventListener('pointermove', e => show(index(e.clientX), e.clientX, e.clientY));
  hit.addEventListener('pointerleave', hide);
  hit.addEventListener('blur', hide);
  const keyShow = i => { const r = svg.getBoundingClientRect(); show(i, r.left + x(i), r.top + m.t + 20); };
  hit.addEventListener('focus', () => keyShow(cur));
  hit.addEventListener('keydown', e => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    keyShow(Math.max(0, Math.min(n - 1, cur + (e.key === 'ArrowRight' ? 1 : -1))));
  });
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

// Feature shares: a withheld count (fewer than 5) is shown as such, never as a number.
const shareRow = r => ({name: r.name, on: r.on, known: r.known, value: r.on === null || !r.known ? 0 : r.on / r.known,
  muted: r.on === null, tipLabel: 'Installations'});
const shareText = r => r.on === null ? (r.known < 5 ? 'not enough data' : 'fewer than 5') : `${pct(r.value)} · ${fmt(r.on)} of ${fmt(r.known)}`;
const shareCells = r => [r.name, r.on === null ? (r.known < 5 ? 'not enough data' : 'fewer than 5') : fmt(r.on), r.on === null ? '' : pct(r.value)];
function featureEmpty(body, f) {
  const n = f ? f.reported : 0;
  empty(body, `Shown once at least 5 installations run Halcyon 0.6.1 or newer${n ? ` (${fmt(n)} so far)` : ''}.`);
}

function kpis() {
  const s = seg(), w = s.windows[state.range];
  const tiles = [];
  const delta = (cur, prev) => prev ? cur / prev - 1 : null;
  const partial = !w.complete && data.history_start ? `since ${fmtDay(data.history_start)}, when tracking began` : null;
  tiles.push({hero: true, label: `Active installations, last ${state.range} days`, value: fmt(w.active),
    delta: delta(w.active, w.previous), vs: `previous ${state.range} days`, note: partial || 'not enough history to compare yet'});
  tiles.push({label: 'Active yesterday', value: s.yesterday === null ? 'n/a' : fmt(s.yesterday),
    delta: s.week_earlier ? delta(s.yesterday, s.week_earlier) : null, vs: 'the same day a week earlier', note: `${fmt(s.today)} so far today (UTC)`});
  tiles.push({label: `New, last ${state.range} days`, value: fmt(w.new), delta: delta(w.new, w.previous_new), vs: `previous ${state.range} days`,
    note: 'first report received in this period'});
  tiles.push({label: 'Still active after 4 weeks', value: data.retention_4w === null ? 'n/a' : pct(data.retention_4w), delta: null,
    note: data.retention_4w === null ? 'needs five complete weeks of data' : 'of new installations, reporting in their fifth week'});
  tiles.push({label: 'On the latest version', value: s.latest_share === null ? 'n/a' : pct(s.latest_share), delta: null,
    note: data.latest_release ? `${data.latest_release.version}, released ${fmtDay(data.latest_release.date)}; last 7 days` : 'no release data yet'});
  const box = $('kpis');
  box.replaceChildren();
  for (const t of tiles) {
    const c = html('div', 'card tile' + (t.hero ? ' hero' : ''));
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
    const {days, from} = view(), s = seg();
    if (!days.length) return empty(body, 'The first complete day appears here tomorrow.');
    const active = s.active.slice(from), fresh = s.new.slice(from);
    if (tableMode['c-active']) return body.appendChild(table(['Date', 'Active', 'New'], days.map((d, i) => [fmtDay(d), fmt(active[i]), fmt(fresh[i])]).reverse()));
    timeChart(body, {days, height: 260, label: 'Daily active installations',
      series: [{name: 'Active', values: active}, {name: 'New', values: fresh}]});
  },
  'c-versions'(body) {
    const {days, from} = view(), s = seg();
    if (!days.length || !s.versions.names.length) return empty(body, 'Not enough data yet.');
    // Four newest named versions; anything older joins the last series.
    // Only versions present in the selected range; the four newest get colors.
    const inRange = n => s.versions.counts[n].slice(from).some(Boolean);
    const names = s.versions.names.filter(inRange), named = names.filter(n => /^\d/.test(n)).slice(0, 4);
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
    const rows = data.releases.map(r => ({name: `${r.version} · ${fmtDay(r.date)}`, value: r.days_to_half ?? 0, days: r.days_to_half, tipLabel: 'Days to half'}));
    const text = r => r.days === null ? 'not yet' : r.days === 0 ? 'same day' : r.days + (r.days === 1 ? ' day' : ' days');
    if (tableMode['c-speed']) {
      body.appendChild(table(['Release', 'Days to half'], rows.map(r => [r.name, text(r)])));
      if (s.updates) body.appendChild(table(['Package installs', 'Installations'], [['Signed updates on', fmt(s.updates.on)], ['Updates off, manual or not reported', fmt(s.updates.other)]]));
      return;
    }
    if (rows.length) {
      hbars(body, rows, 'Days until half of installations updated', text);
      body.appendChild(html('div', 'note', 'Shorter is better.'));
    } else empty(body, 'Shown from the first release after tracking began.');
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
      body.appendChild(html('div', 'note', `Tracking began ${fmtDay(data.history_start)}, so fewer than 28 days are possible so far.`));
  },
  'c-countries'(body) {
    const rows = seg().countries.map(c => ({name: c.name, value: c.count, muted: c.key === 'other' || c.key === 'unknown'}));
    if (!rows.length) return empty(body, 'Not enough data yet.');
    if (tableMode['c-countries']) return body.appendChild(table(['Country', 'Installations'], rows.map(r => [r.name, fmt(r.value)])));
    hbars(body, rows, 'Installations by country', r => fmt(r.value));
  },
  'c-features'(body) {
    const f = data.features;
    if (!f || !f.switches) return featureEmpty(body, f);
    const rows = f.switches.map(shareRow);
    if (tableMode['c-features']) return body.appendChild(table(['Feature', 'On', 'Share'], rows.map(shareCells)));
    hbars(body, rows, 'Share of installations with each feature on', shareText);
  },
  'c-widgets'(body) {
    const f = data.features;
    if (!f || !f.widgets) return featureEmpty(body, f);
    if (!f.widgets.items.length) return empty(body, 'Not enough installations reported their widgets yet.');
    const rows = f.widgets.items.map(w => shareRow({...w, known: f.widgets.known})).sort((a, b) => b.value - a.value);
    if (tableMode['c-widgets']) return body.appendChild(table(['Widget', 'Showing', 'Share'], rows.map(shareCells)));
    hbars(body, rows, 'Share of installations showing each widget', shareText);
  },
  'c-looks'(body) {
    const f = data.features;
    if (!f || !f.switches) return featureEmpty(body, f);
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
      segbar(body, items.map(i => ({...i, slot: i.key === 'other' || slot >= SLOTS.length ? 'gray' : SLOTS[slot++]})), title);
    });
  },
  'c-retention'(body) {
    const cohorts = data.cohorts;
    if (!cohorts.length) return empty(body, 'The first complete week appears here after it ends.');
    const weeks = Math.max(...cohorts.map(c => c.rates.length));
    if (tableMode['c-retention']) {
      return body.appendChild(table(['Week starting', 'New', ...Array.from({length: weeks}, (_, w) => 'Week ' + w)],
        cohorts.map(c => [fmtDay(c.start), c.size === null ? 'fewer than 5' : fmt(c.size),
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
      txt(svg, m.l - 10, yy + ch / 2 + 4, c.size === null ? '<5' : fmt(c.size), {'text-anchor': 'end'});
      if (c.size === null) {
        txt(svg, m.l, yy + ch / 2 + 4, 'Hidden: fewer than 5 new installations', {class: 'hm-row-note'});
        return;
      }
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
  const share = data.opt_out_share;
  $('optout').textContent = share === null ? '' : `${pct(share)} of installations seen in the last 90 days have turned sharing off.`;
}

function setup() {
  const select = $('segment');
  for (const s of data.segments) { const o = html('option', '', s.name); o.value = s.id; select.appendChild(o); }
  select.hidden = data.segments.length < 2;
  select.addEventListener('change', () => { state.segment = select.value; renderAll(); });
  const updated = new Date(data.generated_at);
  $('sub').textContent = `Installations that share usage data · updated hourly, last at ${updated.toLocaleString('en-GB', {day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', timeZone: 'UTC'})} UTC`;
  $('history').textContent = data.history_start ? `Daily history starts ${fmtDay(data.history_start)}` : '';
  document.querySelectorAll('.tbl-toggle').forEach(b => b.addEventListener('click', () => {
    const id = b.closest('.card').id;
    tableMode[id] = !tableMode[id];
    renderAll();
  }));
  document.querySelectorAll('#range button').forEach(b => b.addEventListener('click', () => {
    state.range = Number(b.dataset.v);
    document.querySelectorAll('#range button').forEach(o => o.setAttribute('aria-pressed', String(o === b)));
    renderAll();
  }));
  let timer;
  window.addEventListener('resize', () => { clearTimeout(timer); timer = setTimeout(renderAll, 120); });
}

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

fetch('/api/summary.json', {headers: {Accept: 'application/json'}})
  .then(r => r.ok ? r.json() : Promise.reject(new Error(r.status === 503 ? 'The first numbers are being prepared. Check back within the hour.' : 'Could not load the numbers.')))
  .then(summary => {
    data = summary;
    $('status').hidden = true;
    $('content').hidden = false;
    setup();
    renderAll();
  })
  .catch(error => { $('status').textContent = error.message || 'Could not load the numbers.'; });
})();
