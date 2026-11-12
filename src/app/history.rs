use crate::config::{DevboxPreferences, DevboxSection, Palette};
use crate::data::Metrics;
use crate::widget_style::tint;
use cosmic::iced::{Alignment, Color, Length, Point, Rectangle, mouse};
use cosmic::widget::canvas;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(300);
const MAX_SAMPLES: usize = 301;
pub(super) const HEIGHT: f32 = crate::layout_state::DEVBOX_GRAPH_HEIGHT;
/// The ceilings a percentage graph steps between, lowest first.
const CEILINGS: [f64; 3] = [25., 50., 100.];
/// A lower ceiling is taken only once the whole window sits this far under
/// it, so a reading hovering at a step does not flip the scale back and forth.
const CEILING_HEADROOM: f64 = 0.8;
/// Room kept around the plot so the line and the latest dot are not clipped.
const INSET: f32 = 2.5;

#[derive(Debug, Default)]
pub(super) struct MetricHistories {
    cpu: Percent,
    network: NetworkHistory,
    gpus: BTreeMap<String, Percent>,
    gpu_reset_at: Option<Instant>,
    at: Option<Instant>,
}

impl MetricHistories {
    /// A bounded, frozen history for Settings examples. It is built once from
    /// illustrative readings, never from a hardware poll or persisted data.
    pub(super) fn settings_sample(metrics: &Metrics) -> Self {
        let now = Instant::now();
        let mut histories = Self {
            at: Some(now),
            ..Self::default()
        };
        for (index, variation) in SHAPE.into_iter().enumerate() {
            let age = Duration::from_secs((SHAPE.len() - 1 - index) as u64 * 10);
            let at = now.checked_sub(age).unwrap_or(now);
            histories.cpu.push(
                at,
                Some((f64::from(metrics.cpu) * variation).clamp(0., 100.)),
            );
            if let Some(network) = &metrics.network {
                // Upload moves on its own rhythm, so the two areas read apart.
                let upload = SHAPE[(index + 11) % SHAPE.len()];
                histories.network.push(
                    at,
                    network.download.map(|value| value * variation),
                    network.upload.map(|value| value * upload),
                );
            }
            for gpu in &metrics.gpus {
                let variation = if index == SHAPE.len() - 1 {
                    1.
                } else {
                    SHAPE[(index + 5) % SHAPE.len()]
                };
                histories.gpus.entry(gpu.id.clone()).or_default().push(
                    at,
                    gpu.utilization
                        .map(|value| (f64::from(value) * variation).clamp(0., 100.)),
                );
            }
        }
        histories
    }

    pub(super) fn enabled(preferences: &DevboxPreferences, section: DevboxSection) -> bool {
        section.graphed(preferences)
    }

    pub(super) fn any_enabled(preferences: &DevboxPreferences) -> bool {
        DevboxSection::ALL
            .into_iter()
            .any(|section| Self::enabled(preferences, section))
    }

    pub(super) fn record(
        &mut self,
        metrics: &Metrics,
        preferences: &DevboxPreferences,
        visible: bool,
    ) {
        self.reconcile(preferences, visible);
        if !visible {
            return;
        }
        let at = Instant::now();
        let previous_at = self.at.replace(at);
        if Self::enabled(preferences, DevboxSection::Cpu) {
            self.cpu.push(
                at,
                metrics
                    .cpu_sampled
                    .then_some(metrics.cpu as f64)
                    .and_then(percentage),
            );
        }
        if Self::enabled(preferences, DevboxSection::Network) {
            let network = metrics
                .network
                .as_ref()
                .filter(|network| network.interfaces > 0);
            self.network.push(
                at,
                network.and_then(|network| network.download),
                network.and_then(|network| network.upload),
            );
        }
        if Self::enabled(preferences, DevboxSection::Gpu) {
            let mut identities = BTreeMap::<&str, usize>::new();
            for gpu in &metrics.gpus {
                if !gpu.id.is_empty() {
                    *identities.entry(gpu.id.as_str()).or_default() += 1;
                }
            }
            self.gpus
                .retain(|id, _| identities.get(id.as_str()) == Some(&1));
            for gpu in &metrics.gpus {
                if identities.get(gpu.id.as_str()) != Some(&1) {
                    continue;
                }
                // A newly enabled or rediscovered device must not bring back
                // a cached point from before its history was reset.
                let since = self.gpu_reset_at.max(
                    (!self.gpus.contains_key(&gpu.id))
                        .then_some(previous_at)
                        .flatten(),
                );
                let history = self.gpus.entry(gpu.id.clone()).or_default();
                let sample = gpu
                    .utilization_at
                    .filter(|sample_at| {
                        *sample_at <= at && since.is_none_or(|since| *sample_at > since)
                    })
                    .zip(gpu.utilization.map(f64::from).and_then(percentage))
                    .filter(|_| !gpu.asleep);
                match sample {
                    Some((sample_at, value)) => history.push(sample_at, Some(value)),
                    None => history.push(at, None),
                }
                history.prune(at);
            }
        }
    }

    pub(super) fn reconcile(&mut self, preferences: &DevboxPreferences, visible: bool) {
        if !visible || !Self::enabled(preferences, DevboxSection::Cpu) {
            self.cpu = Percent::default();
        }
        if !visible || !Self::enabled(preferences, DevboxSection::Network) {
            self.network = NetworkHistory::default();
        }
        if !visible || !Self::enabled(preferences, DevboxSection::Gpu) {
            self.gpus.clear();
            self.gpu_reset_at = Some(Instant::now());
        }
        if !visible {
            self.at = None;
        }
    }

    pub(super) fn unavailable(&mut self, preferences: &DevboxPreferences, visible: bool) {
        self.reconcile(preferences, visible);
        if !visible {
            return;
        }
        let at = Instant::now();
        self.at = Some(at);
        if Self::enabled(preferences, DevboxSection::Cpu) {
            self.cpu.push(at, None);
        }
        if Self::enabled(preferences, DevboxSection::Network) {
            self.network.push(at, None, None);
        }
        for history in self.gpus.values_mut() {
            history.push(at, None);
        }
    }

    /// The section's area graph. CPU and GPU name their ceiling in the top
    /// right corner; Network's scale is shared by both directions.
    pub(super) fn view<'a, Message: 'a>(
        &'a self,
        section: DevboxSection,
        gpu_id: Option<&str>,
        palette: Palette,
    ) -> cosmic::Element<'a, Message> {
        let percent = |history: &'a Percent| {
            (
                vec![Trace {
                    series: &history.series,
                    scale: history.ceiling,
                    color: palette.accent,
                }],
                Some(history.ceiling),
            )
        };
        let (traces, ceiling) = match section {
            DevboxSection::Cpu => percent(&self.cpu),
            // Upload first, so download (the main reading) draws on top.
            DevboxSection::Network => (
                vec![
                    Trace {
                        series: &self.network.upload,
                        scale: self.network.scale,
                        color: palette.muted,
                    },
                    Trace {
                        series: &self.network.download,
                        scale: self.network.scale,
                        color: palette.accent,
                    },
                ],
                None,
            ),
            DevboxSection::Gpu => gpu_id
                .and_then(|id| self.gpus.get(id))
                .map(percent)
                .unwrap_or_default(),
            DevboxSection::Memory
            | DevboxSection::Disk
            | DevboxSection::Uptime
            | DevboxSection::Services => (Vec::new(), None),
        };
        let graph: cosmic::Element<'a, Message> = canvas(Sparkline {
            traces,
            at: self.at,
            track: tint(palette.accent, palette.background, 0.06),
            baseline: tint(palette.accent, palette.background, 0.2),
        })
        .width(Length::Fill)
        .height(Length::Fixed(HEIGHT))
        .into();
        let Some(ceiling) = ceiling else {
            return graph;
        };
        let label = cosmic::widget::text::body(format!("{ceiling:.0}%"))
            .size(9.)
            .line_height(1.)
            .font(crate::widget_style::font())
            .class(cosmic::theme::Text::Color(palette.muted));
        cosmic::iced::widget::stack(vec![
            graph,
            cosmic::widget::container(label)
                .width(Length::Fill)
                .align_x(Alignment::End)
                .padding([2, 4])
                .into(),
        ])
        .height(Length::Fixed(HEIGHT))
        .into()
    }
}

/// The Settings example's few minutes: a gentle rise and fall around the
/// reading, ending on it, rather than a regular saw tooth.
const SHAPE: [f64; 31] = [
    0.70, 0.74, 0.82, 0.78, 0.72, 0.76, 0.88, 1.02, 1.12, 1.06, 0.94, 0.86, 0.90, 1.00, 1.18, 1.32,
    1.24, 1.08, 0.96, 0.84, 0.80, 0.86, 0.98, 1.10, 1.04, 0.92, 0.88, 0.94, 1.02, 0.98, 1.00,
];

/// A percentage reading with the ceiling its graph is drawn against: 25, 50
/// or 100%, so a mostly idle processor still shows its shape.
#[derive(Debug)]
struct Percent {
    series: Series,
    ceiling: f64,
}

impl Default for Percent {
    fn default() -> Self {
        Self {
            series: Series::default(),
            ceiling: CEILINGS[0],
        }
    }
}

impl Percent {
    fn push(&mut self, at: Instant, value: Option<f64>) {
        self.series.push(at, value);
        self.rescale();
    }

    fn prune(&mut self, now: Instant) {
        self.series.prune(now);
        self.rescale();
    }

    /// Rises as soon as a reading passes the ceiling; steps down only when
    /// the whole window fits comfortably under a lower one.
    fn rescale(&mut self) {
        let peak = self
            .series
            .samples
            .iter()
            .filter_map(|(_, value)| *value)
            .fold(0., f64::max);
        let step = |headroom: f64| {
            CEILINGS
                .into_iter()
                .find(|ceiling| peak <= ceiling * headroom)
                .unwrap_or(CEILINGS[CEILINGS.len() - 1])
        };
        let needed = step(1.);
        self.ceiling = if needed > self.ceiling {
            needed
        } else {
            self.ceiling.min(step(CEILING_HEADROOM)).max(needed)
        };
    }
}

struct Trace<'a> {
    series: &'a Series,
    scale: f64,
    color: Color,
}

struct Sparkline<'a> {
    traces: Vec<Trace<'a>>,
    at: Option<Instant>,
    /// The faint band behind the plot, the whole width of the card.
    track: Color,
    /// The zero line along the bottom.
    baseline: Color,
}

impl<Message> canvas::Program<Message, cosmic::Theme> for Sparkline<'_> {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (width, height) = (bounds.width, bounds.height);
        frame.fill(
            &canvas::Path::rounded_rectangle(Point::ORIGIN, bounds.size(), 3.into()),
            self.track,
        );
        let bottom = height - 0.5;
        frame.stroke(
            &canvas::Path::line(Point::new(0., bottom), Point::new(width, bottom)),
            canvas::Stroke::default()
                .with_color(self.baseline)
                .with_width(1.),
        );
        let Some(at) = self.at else {
            return vec![frame.into_geometry()];
        };
        let point = |(x, y): (f32, f32)| {
            Point::new(
                INSET + x * (width - 2. * INSET).max(0.),
                INSET + y * (height - 2. * INSET).max(0.),
            )
        };
        for trace in &self.traces {
            let segments = trace.series.segments(at, trace.scale);
            for segment in &segments {
                let first = point(segment[0]);
                let last = point(segment[segment.len() - 1]);
                if segment.len() > 1 {
                    let area = canvas::Path::new(|path| {
                        path.move_to(Point::new(first.x, bottom));
                        for sample in segment {
                            path.line_to(point(*sample));
                        }
                        path.line_to(Point::new(last.x, bottom));
                        path.close();
                    });
                    frame.fill(
                        &area,
                        Color {
                            a: trace.color.a * 0.22,
                            ..trace.color
                        },
                    );
                    let line = canvas::Path::new(|path| {
                        path.move_to(first);
                        for sample in &segment[1..] {
                            path.line_to(point(*sample));
                        }
                    });
                    frame.stroke(
                        &line,
                        canvas::Stroke::default()
                            .with_color(trace.color)
                            .with_width(1.75)
                            .with_line_join(canvas::LineJoin::Round)
                            .with_line_cap(canvas::LineCap::Round),
                    );
                } else {
                    frame.fill(&canvas::Path::circle(first, 0.9), trace.color);
                }
            }
            // The newest reading is marked, unless the newest sample is a gap.
            if trace
                .series
                .samples
                .back()
                .is_some_and(|(_, value)| value.is_some())
                && let Some(latest) = segments.last().and_then(|segment| segment.last())
            {
                frame.fill(&canvas::Path::circle(point(*latest), 2.25), trace.color);
            }
        }
        vec![frame.into_geometry()]
    }
}

fn percentage(value: f64) -> Option<f64> {
    (value.is_finite() && (0. ..=100.).contains(&value)).then_some(value)
}

#[derive(Debug, Default)]
struct Series {
    samples: VecDeque<(Instant, Option<f64>)>,
}

impl Series {
    fn push(&mut self, at: Instant, value: Option<f64>) {
        // Cached device readings retain their measurement timestamp. Do not
        // turn them into fresh points, or reconnect a gap to an older reading.
        if self.samples.back().is_some_and(|(last, _)| *last >= at) {
            return;
        }
        self.samples
            .push_back((at, value.filter(|value| value.is_finite())));
        self.prune(at);
    }

    fn prune(&mut self, now: Instant) {
        while self.samples.front().is_some_and(|(at, _)| {
            now.saturating_duration_since(*at) > WINDOW || self.samples.len() > MAX_SAMPLES
        }) {
            self.samples.pop_front();
        }
    }

    fn segments(&self, now: Instant, scale: f64) -> Vec<Vec<(f32, f32)>> {
        let mut segments = Vec::new();
        let mut current = Vec::new();
        for (at, value) in &self.samples {
            let age = now.saturating_duration_since(*at);
            if age > WINDOW {
                continue;
            }
            match value {
                Some(value) => current.push((
                    1. - age.as_secs_f32() / WINDOW.as_secs_f32(),
                    (1. - value / scale).clamp(0., 1.) as f32,
                )),
                None if !current.is_empty() => segments.push(std::mem::take(&mut current)),
                None => {}
            }
        }
        if !current.is_empty() {
            segments.push(current);
        }
        segments
    }
}

#[derive(Debug)]
struct NetworkHistory {
    download: Series,
    upload: Series,
    scale: f64,
}

impl Default for NetworkHistory {
    fn default() -> Self {
        Self {
            download: Series::default(),
            upload: Series::default(),
            scale: 1024.,
        }
    }
}

impl NetworkHistory {
    fn push(&mut self, at: Instant, download: Option<f64>, upload: Option<f64>) {
        self.download
            .push(at, download.filter(|value| *value >= 0.));
        self.upload.push(at, upload.filter(|value| *value >= 0.));
        let peak = self
            .download
            .samples
            .iter()
            .chain(&self.upload.samples)
            .filter_map(|(_, value)| *value)
            .fold(0., f64::max);
        // Both directions share a power-of-two ceiling. Keep it until traffic
        // exceeds it, or the entire recent window fits below a quarter of it.
        // This makes ordinary variation move the line, not the graph's scale.
        if peak > self.scale || peak < self.scale / 4. {
            self.scale = 1024.;
            while self.scale < peak {
                self.scale = (self.scale * 2.).min(f64::MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> DevboxPreferences {
        DevboxPreferences {
            history: crate::config::DevboxHistoryPreferences {
                cpu: true,
                memory: true,
                disk: true,
                network: true,
                gpu: true,
            },
            ..Default::default()
        }
    }

    fn metrics() -> Metrics {
        let mut metrics = Metrics {
            cpu: 25.,
            cpu_sampled: true,
            memory_used: 8.,
            memory_total: 32.,
            memory_sampled: true,
            disk: Some((50., 200.)),
            network: Some(Default::default()),
            gpus: vec![Default::default(), Default::default()],
            ..Default::default()
        };
        let network = metrics.network.as_mut().unwrap();
        network.interfaces = 1;
        network.download = Some(1024.);
        network.upload = Some(512.);
        for (index, gpu) in metrics.gpus.iter_mut().enumerate() {
            gpu.id = format!("0000:0{index}:00.0");
            gpu.name = "Same model".into();
            gpu.utilization = Some((index + 1) as f32 * 20.);
            gpu.utilization_at = Some(Instant::now());
        }
        metrics
    }

    #[test]
    fn only_valid_new_percentage_readings_enter_history() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        histories.record(&metrics, &enabled(), true);
        assert_eq!(histories.cpu.series.samples.back().unwrap().1, Some(25.));
        metrics.cpu_sampled = false;
        histories.record(&metrics, &enabled(), true);
        assert_eq!(histories.cpu.series.samples.back().unwrap().1, None);
        metrics.cpu_sampled = true;
        metrics.cpu = f32::NAN;
        histories.record(&metrics, &enabled(), true);
        assert_eq!(histories.cpu.series.samples.back().unwrap().1, None);
    }

    #[test]
    fn disabling_a_history_or_section_discards_its_old_samples() {
        let mut histories = MetricHistories::default();
        let mut preferences = enabled();
        histories.record(&metrics(), &preferences, true);
        preferences.history.cpu = false;
        preferences.memory = false;
        preferences.history.network = false;
        preferences.gpu = false;
        histories.reconcile(&preferences, true);
        assert!(histories.cpu.series.samples.is_empty());
        assert!(histories.network.download.samples.is_empty());
        assert!(histories.gpus.is_empty());
        histories.record(&metrics(), &preferences, true);
        assert!(histories.cpu.series.samples.is_empty());
        assert!(histories.network.download.samples.is_empty());
        assert!(histories.gpus.is_empty());
    }

    #[test]
    fn hidden_cards_stop_collecting_and_reset_before_becoming_visible() {
        let mut histories = MetricHistories::default();
        histories.record(&metrics(), &enabled(), true);
        histories.reconcile(&enabled(), false);
        histories.record(&metrics(), &enabled(), false);
        assert!(histories.cpu.series.samples.is_empty());
        assert!(histories.network.download.samples.is_empty());
        assert!(histories.gpus.is_empty());
        histories.record(&metrics(), &enabled(), true);
        assert_eq!(histories.cpu.series.samples.len(), 1);
    }

    #[test]
    fn sample_failure_adds_gaps_to_existing_visible_histories() {
        let mut histories = MetricHistories::default();
        histories.record(&metrics(), &enabled(), true);
        histories.unavailable(&enabled(), true);
        assert_eq!(histories.cpu.series.samples.back().unwrap().1, None);
        assert_eq!(histories.network.download.samples.back().unwrap().1, None);
        assert_eq!(histories.network.upload.samples.back().unwrap().1, None);
        for series in histories.gpus.values() {
            assert_eq!(series.series.samples.back().unwrap().1, None);
        }
    }

    #[test]
    fn gpu_histories_follow_device_identity_across_reorder_and_disappearance() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        histories.record(&metrics, &enabled(), true);
        metrics.gpus.reverse();
        for gpu in &mut metrics.gpus {
            gpu.utilization_at = Some(Instant::now());
            gpu.utilization = gpu.utilization.map(|value| value + 5.);
        }
        histories.record(&metrics, &enabled(), true);
        assert_eq!(
            histories.gpus["0000:00:00.0"]
                .series
                .samples
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>(),
            [Some(20.), Some(25.)]
        );
        assert_eq!(
            histories.gpus["0000:01:00.0"]
                .series
                .samples
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>(),
            [Some(40.), Some(45.)]
        );
        let missing = metrics.gpus.pop().unwrap();
        histories.record(&metrics, &enabled(), true);
        assert!(!histories.gpus.contains_key(&missing.id));
        metrics.gpus.push(missing);
        histories.record(&metrics, &enabled(), true);
        assert_eq!(histories.gpus["0000:00:00.0"].series.samples.len(), 1);
    }

    #[test]
    fn gpu_cache_is_not_repeated_and_sleeping_devices_produce_gaps() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        histories.record(&metrics, &enabled(), true);
        histories.record(&metrics, &enabled(), true);
        assert_eq!(histories.gpus["0000:00:00.0"].series.samples.len(), 1);
        metrics.gpus[0].asleep = true;
        histories.record(&metrics, &enabled(), true);
        assert_eq!(
            histories.gpus["0000:00:00.0"]
                .series
                .samples
                .back()
                .unwrap()
                .1,
            None
        );
    }

    #[test]
    fn reenabled_gpu_history_waits_for_a_measurement_after_reset() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        let mut preferences = enabled();
        histories.record(&metrics, &preferences, true);
        preferences.history.gpu = false;
        histories.reconcile(&preferences, true);
        preferences.history.gpu = true;
        histories.record(&metrics, &preferences, true);
        assert_eq!(
            histories.gpus["0000:00:00.0"]
                .series
                .samples
                .back()
                .unwrap()
                .1,
            None
        );
        metrics.gpus[0].utilization_at = Some(Instant::now());
        histories.record(&metrics, &preferences, true);
        assert_eq!(
            histories.gpus["0000:00:00.0"]
                .series
                .samples
                .back()
                .unwrap()
                .1,
            Some(20.)
        );
    }

    #[test]
    fn rediscovered_gpu_cannot_restore_a_cached_point_from_before_disappearance() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        histories.record(&metrics, &enabled(), true);
        let gpu = metrics.gpus.pop().unwrap();
        histories.record(&metrics, &enabled(), true);
        metrics.gpus.push(gpu);
        histories.record(&metrics, &enabled(), true);
        assert_eq!(
            histories.gpus["0000:01:00.0"]
                .series
                .samples
                .back()
                .unwrap()
                .1,
            None
        );
    }

    #[test]
    fn ambiguous_gpu_identifiers_cannot_merge_two_device_histories() {
        let mut histories = MetricHistories::default();
        let mut metrics = metrics();
        metrics.gpus[1].id = metrics.gpus[0].id.clone();
        histories.record(&metrics, &enabled(), true);
        assert!(histories.gpus.is_empty());
        metrics.gpus[0].id.clear();
        metrics.gpus[1].id.clear();
        histories.record(&metrics, &enabled(), true);
        assert!(histories.gpus.is_empty());
    }

    #[test]
    fn history_keeps_a_bounded_five_minute_window() {
        let start = Instant::now();
        let mut series = Series::default();
        for second in 0..600 {
            series.push(start + Duration::from_secs(second), Some(second as f64));
        }
        assert_eq!(series.samples.len(), 301);
        assert_eq!(series.samples.front().unwrap().1, Some(299.));
        assert_eq!(series.samples.back().unwrap().1, Some(599.));
    }

    #[test]
    fn unexpectedly_fast_samples_still_have_a_hard_memory_bound() {
        let start = Instant::now();
        let mut series = Series::default();
        for millis in 0..10_000 {
            series.push(start + Duration::from_millis(millis), Some(42.));
        }
        assert_eq!(series.samples.len(), 301);
    }

    #[test]
    fn missing_and_invalid_samples_break_paths_without_becoming_zero() {
        let start = Instant::now();
        let mut series = Series::default();
        for (second, value) in [Some(25.), None, Some(75.), Some(f64::NAN), Some(50.)]
            .into_iter()
            .enumerate()
        {
            series.push(start + Duration::from_secs(second as u64), value);
        }
        let segments = series.segments(start + Duration::from_secs(4), 100.);
        assert_eq!(segments.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1, 1]);
        assert_eq!(segments[0][0].1, 0.75);
        assert_eq!(segments[1][0].1, 0.25);
        assert_eq!(segments[2][0], (1., 0.5));
    }

    #[test]
    fn plot_coordinates_do_not_stretch_while_history_fills() {
        let start = Instant::now();
        let mut series = Series::default();
        series.push(start, Some(50.));
        series.push(start + Duration::from_secs(150), Some(100.));
        assert_eq!(
            series.segments(start + Duration::from_secs(150), 100.),
            [vec![(0.5, 0.5), (1., 0.)]]
        );
    }

    #[test]
    fn repeated_cached_samples_do_not_create_new_history_points() {
        let start = Instant::now();
        let mut series = Series::default();
        series.push(start, Some(50.));
        series.push(start, Some(50.));
        series.push(start - Duration::from_secs(1), Some(40.));
        assert_eq!(series.samples.len(), 1);
    }

    #[test]
    fn network_uses_one_stable_scale_for_both_directions() {
        let start = Instant::now();
        let mut network = NetworkHistory::default();
        network.push(start, Some(1200.), Some(6000.));
        assert_eq!(network.scale, 8192.);
        for second in 1..=300 {
            network.push(start + Duration::from_secs(second), Some(1100.), Some(50.));
            assert_eq!(network.scale, 8192.);
        }
        network.push(start + Duration::from_secs(301), Some(1100.), Some(50.));
        assert_eq!(network.scale, 2048.);
    }

    #[test]
    fn invalid_network_rates_are_gaps_and_zero_is_a_real_measurement() {
        let start = Instant::now();
        let mut network = NetworkHistory::default();
        network.push(start, Some(-1.), Some(f64::INFINITY));
        network.push(start + Duration::from_secs(1), Some(0.), Some(0.));
        assert_eq!(network.download.samples[0].1, None);
        assert_eq!(network.upload.samples[0].1, None);
        assert_eq!(network.download.samples[1].1, Some(0.));
        assert_eq!(network.scale, 1024.);
    }

    #[test]
    fn percentage_graphs_step_their_ceiling_with_hysteresis() {
        let start = Instant::now();
        let mut cpu = Percent::default();
        let at = |second: u64| start + Duration::from_secs(second);
        cpu.push(at(0), Some(12.));
        assert_eq!(cpu.ceiling, 25., "an idle processor fills its graph");
        // A reading over the ceiling raises it at once, to the step it fits.
        cpu.push(at(1), Some(30.));
        assert_eq!(cpu.ceiling, 50.);
        cpu.push(at(2), Some(64.));
        assert_eq!(cpu.ceiling, 100.);
        // Quiet readings keep it while the 64% is still in the window...
        for second in 3..=302 {
            cpu.push(at(second), Some(22.));
            assert_eq!(cpu.ceiling, 100., "{second}");
        }
        // ...then it steps down, but only as far as the window fits with room:
        // 22% is under 25% but too close to it, so 50% it is.
        cpu.push(at(303), Some(22.));
        assert_eq!(cpu.ceiling, 50.);
        for second in 304..=603 {
            cpu.push(at(second), Some(19.));
            assert_eq!(cpu.ceiling, 50., "22% is still in the window");
        }
        cpu.push(at(604), Some(19.));
        assert_eq!(cpu.ceiling, 25.);
        // Gaps never count as zero, and a reset starts at the lowest step.
        cpu.push(at(605), None);
        assert_eq!(cpu.ceiling, 25.);
        assert_eq!(Percent::default().ceiling, 25.);
        assert!(
            cpu.series
                .segments(at(605), cpu.ceiling)
                .iter()
                .flatten()
                .all(|(_, y)| (0. ..=1.).contains(y))
        );
    }

    #[test]
    fn memory_and_disk_never_collect_a_history() {
        let mut histories = MetricHistories::default();
        histories.record(&metrics(), &enabled(), true);
        for section in [DevboxSection::Memory, DevboxSection::Disk] {
            assert!(!MetricHistories::enabled(&enabled(), section));
        }
        let only_memory = DevboxPreferences {
            history: crate::config::DevboxHistoryPreferences {
                memory: true,
                disk: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!MetricHistories::any_enabled(&only_memory));
    }

    #[test]
    fn the_settings_example_ends_on_its_readings_under_a_fitting_ceiling() {
        let sample = &crate::app::settings_preview::monitor_sample().metrics;
        let histories = MetricHistories::settings_sample(sample);
        let last = |series: &Series| series.samples.back().unwrap().1.unwrap();
        assert_eq!(last(&histories.cpu.series), f64::from(sample.cpu));
        // 26% wanders up to about a third, so the example reads against 50%.
        assert_eq!(histories.cpu.ceiling, 50.);
        let gpu = histories.gpus.values().next().unwrap();
        assert_eq!(last(&gpu.series), 18.);
        assert_eq!(gpu.ceiling, 25.);
        let network = sample.network.as_ref().unwrap();
        assert_eq!(last(&histories.network.download), network.download.unwrap());
        assert!(histories.network.scale >= network.download.unwrap() * 1.32);
        assert_eq!(histories.cpu.series.samples.len(), SHAPE.len());
    }
}
