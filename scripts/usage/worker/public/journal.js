// The partial snapshot is separate from the frozen daily series.
export function journalDays(summary, segment, range) {
  const series = summary.by_segment[segment];
  const dates = [...summary.days, summary.today];
  const start = Math.max(0, dates.length - range);
  return dates.slice(start).map((date, offset) => {
    const i = start + offset;
    const partial = i === summary.days.length;
    const active = partial ? series.today : series.active[i];
    const first = partial ? series.today_new : series.new[i];
    const previous = series.active.slice(Math.max(0, i - 7), i);
    const median = previous.length === 7 ? previous.sort((a, b) => a - b)[3] : null;
    return {date, partial, active, first, returning: first == null ? null : active - first, median,
      releases: summary.releases.filter(r => r.date === date).map(r => r.version)};
  });
}

export function snapshotState(summary, now = new Date()) {
  const current = now.toISOString().slice(0, 10) === summary.today;
  const delayed = now.getTime() - Date.parse(summary.generated_at) >= 2 * 3600000;
  return {current, delayed};
}

// Align releases by elapsed UTC calendar days. Only compare completed days
// present in both histories; an empty denominator means no observation.
export function releaseReplay(summary, segment, range) {
  const releases = summary.releases.filter(r => !r.baseline).slice(0, 2);
  if (releases.length < 2) return [];
  const versions = summary.by_segment[segment].versions;
  const known = versions.names.filter(name => /^\d/.test(name));
  const dates = new Map(summary.days.map((day, i) => [day, i]));
  const launches = releases.map(r => Date.parse(r.date + 'T00:00:00Z'));
  const frames = [];
  for (let age = 0; age < range; age++) {
    const entries = releases.map((r, k) => {
      const date = new Date(launches[k] + age * 86400000).toISOString().slice(0, 10);
      const i = dates.get(date);
      if (i === undefined) return null;
      const total = known.reduce((sum, name) => sum + versions.counts[name][i], 0);
      return {version: r.version, date, total, count: versions.counts[r.version]?.[i] ?? 0};
    });
    if (entries.every(Boolean)) frames.push({age, entries});
  }
  return frames;
}
