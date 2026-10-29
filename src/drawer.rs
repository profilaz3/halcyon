use cosmic::iced::Point;
use std::time::{Duration, Instant};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Calendar,
    Projects,
    Devbox,
    Weather,
    Clock,
    Glance,
    /// Desktop tools: optional, free-floating, off until chosen.
    Music,
    Notes,
    Focus,
}
impl Widget {
    pub const ALL: [Self; 9] = [
        Self::Calendar,
        Self::Projects,
        Self::Devbox,
        Self::Weather,
        Self::Clock,
        Self::Glance,
        Self::Music,
        Self::Notes,
        Self::Focus,
    ];
    /// The widgets that existed before desktop tools; their surfaces share
    /// one geometry and are recreated together.
    pub const BASE: [Self; 6] = [
        Self::Calendar,
        Self::Projects,
        Self::Devbox,
        Self::Weather,
        Self::Clock,
        Self::Glance,
    ];
    pub const CARDS: [Self; 4] = [Self::Calendar, Self::Projects, Self::Devbox, Self::Weather];
    /// Tools keep their visibility in their own config section (like Glance)
    /// and always have a surface of their own, placed freely.
    pub const TOOLS: [Self; 3] = [Self::Music, Self::Notes, Self::Focus];
    pub fn is_tool(self) -> bool {
        Self::TOOLS.contains(&self)
    }
    /// Placed by their own position in every layout mode, so moving one
    /// never turns the default arrangement into a custom one.
    pub fn floating(self) -> bool {
        self == Self::Glance || self.is_tool()
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::Projects => "projects",
            Self::Devbox => "devbox",
            Self::Weather => "weather",
            Self::Clock => "clock",
            Self::Glance => "glance",
            Self::Music => "music",
            Self::Notes => "notes",
            Self::Focus => "focus",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Calendar => "Calendar",
            Self::Projects => "Projects",
            Self::Devbox => "System monitor",
            Self::Weather => "Weather",
            Self::Clock => "Clock",
            Self::Glance => "Daily overview",
            Self::Music => "Music",
            Self::Notes => "Notes",
            Self::Focus => "Focus timer",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Calendar => "x-office-calendar-symbolic",
            Self::Projects => "utilities-terminal-symbolic",
            Self::Devbox => "utilities-system-monitor-symbolic",
            Self::Weather => "weather-clear-symbolic",
            Self::Clock => "preferences-system-time-symbolic",
            Self::Glance => "view-grid-symbolic",
            Self::Music => "multimedia-audio-player-symbolic",
            Self::Notes => "accessories-text-editor-symbolic",
            Self::Focus => "alarm-symbolic",
        }
    }
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|widget| widget.key() == key)
    }
}

pub const SIDEBAR_WIDTH: u32 = 184;
pub const SIDEBAR_HEIGHT: u32 = 548;
pub const FIRST_ROW: f32 = 64.;
pub const ROW_STEP: f32 = 50.;
pub const ROW_HEIGHT: f32 = 44.;
pub const CARD_GAP: f32 = 14.;
pub const EDGE_DWELL: Duration = Duration::from_secs(1);
const HIDDEN_MARGIN: i32 = -(SIDEBAR_WIDTH as i32) - 8;

pub struct Sidebar {
    pub visible: bool,
    pub hovered: bool,
    pub edge_hovered: bool,
    pub progress: f32,
    pub pointer: Point,
    pub drag: Option<ReorderDrag>,
    dismiss_at: Option<Instant>,
    reveal_at: Option<Instant>,
    last_frame: Instant,
}
impl Sidebar {
    pub fn new() -> Self {
        Self {
            visible: false,
            hovered: false,
            edge_hovered: false,
            progress: 0.,
            pointer: Point::ORIGIN,
            drag: None,
            dismiss_at: None,
            reveal_at: None,
            last_frame: Instant::now(),
        }
    }
    pub fn show(&mut self, now: Instant) {
        self.visible = true;
        self.dismiss_at = None;
        self.reveal_at = None;
        self.last_frame = now;
    }
    pub fn hide(&mut self) {
        self.visible = false;
        self.dismiss_at = None;
        self.reveal_at = None;
        self.last_frame = Instant::now();
    }
    pub fn hover(&mut self, edge: bool, entered: bool, now: Instant) {
        if edge {
            let was_hovered = self.edge_hovered;
            self.edge_hovered = entered;
            if !entered {
                self.reveal_at = None;
            } else if !self.visible {
                if !was_hovered {
                    self.reveal_at = Some(now + EDGE_DWELL);
                    self.last_frame = now;
                }
                self.dismiss_at = None;
                return;
            }
        } else {
            self.hovered = entered;
        }
        if entered && (self.visible || self.progress > 0.) {
            self.show(now);
        } else {
            self.dismiss_if_outside(now);
        }
    }
    pub fn dismiss_if_outside(&mut self, now: Instant) {
        if !self.hovered
            && !self.edge_hovered
            && self.drag.is_none()
            && (self.visible || self.progress > 0.)
        {
            self.dismiss_at = Some(now + Duration::from_millis(400));
        }
    }
    pub fn animating(&self) -> bool {
        (self.progress - if self.visible { 1. } else { 0. }).abs() > 0.001
    }
    pub fn next_transition(&self) -> Option<Instant> {
        self.dismiss_at.into_iter().chain(self.reveal_at).min()
    }
    /// Advances reveal/dismissal and the slide. `slide`: off, the sidebar
    /// is fully shown or hidden on the first frame.
    pub fn tick(&mut self, now: Instant, slide: bool) {
        if self.reveal_at.is_some_and(|at| now >= at) {
            self.reveal_at = None;
            if self.edge_hovered {
                self.show(now);
            }
        }
        if self.dismiss_at.is_some_and(|at| now >= at) {
            self.dismiss_at = None;
            if !self.hovered && !self.edge_hovered && self.drag.is_none() {
                self.visible = false;
            }
        }
        let dt = now
            .saturating_duration_since(self.last_frame)
            .as_secs_f32()
            .min(0.05);
        self.last_frame = now;
        let target = if self.visible { 1. } else { 0. };
        if !slide {
            self.progress = target;
            return;
        }
        self.progress += (target - self.progress) * (1. - (-22. * dt).exp());
        if (target - self.progress).abs() < 0.005 {
            self.progress = target;
        }
    }
    pub fn margin(&self) -> i32 {
        (HIDDEN_MARGIN as f32 * (1. - self.progress)).round() as i32
    }
}

pub struct ReorderDrag {
    pub widget: Widget,
    pub original_order: Vec<Widget>,
    pub moved: bool,
    start: Point,
}
impl ReorderDrag {
    pub fn new(widget: Widget, order: Vec<Widget>, start: Point) -> Self {
        Self {
            widget,
            original_order: order,
            start,
            moved: false,
        }
    }
    pub fn preview(&mut self, point: Point, order: &mut Vec<Widget>) {
        self.moved |= point.distance(self.start) > 6.;
        if !self.moved || !(0. ..=SIDEBAR_WIDTH as f32).contains(&point.x) {
            return;
        }
        let Some(from) = order.iter().position(|w| *w == self.widget) else {
            return;
        };
        let to = ((point.y - FIRST_ROW) / ROW_STEP)
            .floor()
            .clamp(0., (order.len() - 1) as f32) as usize;
        if from != to {
            let item = order.remove(from);
            order.insert(to, item);
        }
    }
    pub fn finish(self, point: Point, order: Vec<Widget>) -> DragResult {
        let inside = (0. ..=SIDEBAR_WIDTH as f32).contains(&point.x)
            && (0. ..=SIDEBAR_HEIGHT as f32).contains(&point.y);
        if !inside {
            return DragResult::Cancel(self.original_order);
        }
        if self.moved {
            return DragResult::Reorder(order);
        }
        let index = self
            .original_order
            .iter()
            .position(|w| *w == self.widget)
            .unwrap_or(0);
        let row_y = point.y - FIRST_ROW - index as f32 * ROW_STEP;
        if (0. ..=ROW_HEIGHT).contains(&row_y) {
            DragResult::Click(self.widget)
        } else {
            DragResult::Cancel(self.original_order)
        }
    }
}
#[derive(Debug, PartialEq)]
pub enum DragResult {
    Click(Widget),
    Reorder(Vec<Widget>),
    Cancel(Vec<Widget>),
}

// Budget the right-column card heights. Content-sized cards shrink within
// these limits; the resulting stack is centered on the desktop.
#[cfg(test)]
pub fn card_layout(widgets: &[Widget], available: f32) -> Vec<(Widget, f32)> {
    card_layout_with_devbox(widgets, available, 172.)
}
pub fn card_layout_with_devbox(
    widgets: &[Widget],
    available: f32,
    devbox_height: f32,
) -> Vec<(Widget, f32)> {
    let desired = |widget| match widget {
        Widget::Calendar => 540.,
        Widget::Projects => 300.,
        Widget::Devbox => devbox_height,
        Widget::Weather => crate::widget_presentation::WEATHER_SIZE[1],
        Widget::Clock | Widget::Glance | Widget::Music | Widget::Notes | Widget::Focus => 0.,
    };
    let total: f32 = widgets.iter().map(|w| desired(*w)).sum();
    let gaps = widgets.len().saturating_sub(1) as f32 * CARD_GAP;
    let scale = ((available - gaps).max(0.) / total.max(1.)).min(1.);
    widgets.iter().map(|w| (*w, desired(*w) * scale)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_dwell_and_dismissal_wait_without_animation_frames() {
        let now = Instant::now();
        let mut sidebar = Sidebar::new();
        sidebar.hover(true, true, now);
        assert!(!sidebar.animating());
        assert_eq!(sidebar.next_transition(), Some(now + EDGE_DWELL));
        sidebar.tick(now + EDGE_DWELL, true);
        assert!(sidebar.visible && sidebar.animating());
        assert!(sidebar.next_transition().is_none());

        // Once the panel has settled, its dismissal grace period also sleeps.
        sidebar.progress = 1.;
        let leave = now + Duration::from_secs(2);
        sidebar.hover(true, false, leave);
        assert!(!sidebar.animating());
        assert_eq!(
            sidebar.next_transition(),
            Some(leave + Duration::from_millis(400))
        );
        sidebar.tick(leave + Duration::from_millis(400), true);
        assert!(!sidebar.visible && sidebar.animating());
        assert!(sidebar.next_transition().is_none());
    }
    #[test]
    fn without_the_slide_the_sidebar_shows_and_hides_on_one_frame() {
        let now = Instant::now();
        let mut sidebar = Sidebar::new();
        sidebar.hover(true, true, now);
        sidebar.tick(now + EDGE_DWELL, false);
        assert!(sidebar.visible && !sidebar.animating());
        assert_eq!(sidebar.margin(), 0);
        let leave = now + Duration::from_secs(2);
        sidebar.hover(true, false, leave);
        sidebar.tick(leave + Duration::from_millis(400), false);
        assert!(!sidebar.visible && !sidebar.animating());
        assert!(sidebar.margin() < -(SIDEBAR_WIDTH as i32));
    }
    #[test]
    fn edge_requires_a_full_second_then_leaving_hides_the_sidebar() {
        let now = Instant::now();
        let mut sidebar = Sidebar::new();
        assert!(sidebar.margin() < -(SIDEBAR_WIDTH as i32));
        sidebar.hover(true, true, now);
        sidebar.tick(now + Duration::from_millis(999), true);
        assert!(!sidebar.visible);
        assert_eq!(sidebar.progress, 0.);
        // Repeated motion on the edge must not reset the dwell timer.
        sidebar.hover(true, true, now + Duration::from_millis(999));
        sidebar.tick(now + Duration::from_secs(1), true);
        assert!(sidebar.visible);
        sidebar.hover(false, true, now + Duration::from_millis(1100));
        sidebar.hover(true, false, now + Duration::from_millis(1100));
        sidebar.tick(now + Duration::from_millis(1200), true);
        assert!(sidebar.visible);
        sidebar.hover(false, false, now + Duration::from_millis(1200));
        for step in 1..=60 {
            sidebar.tick(now + Duration::from_millis(1200 + step * 16), true);
        }
        assert!(!sidebar.visible);
        assert!(sidebar.margin() < -(SIDEBAR_WIDTH as i32));
        assert!(!sidebar.animating());
    }
    #[test]
    fn brief_edge_visits_cancel_and_reentry_starts_a_new_second() {
        let now = Instant::now();
        let mut sidebar = Sidebar::new();
        sidebar.hover(true, true, now);
        sidebar.hover(true, false, now + Duration::from_millis(900));
        sidebar.tick(now + Duration::from_secs(2), true);
        assert!(!sidebar.visible);
        assert_eq!(sidebar.progress, 0.);
        assert!(!sidebar.animating());
        sidebar.hover(true, true, now + Duration::from_secs(3));
        sidebar.tick(now + Duration::from_millis(3999), true);
        assert!(!sidebar.visible);
        sidebar.tick(now + Duration::from_secs(4), true);
        assert!(sidebar.visible);
        sidebar.hide();
        sidebar.tick(now + Duration::from_secs(5), true);
        assert!(!sidebar.visible);
    }
    #[test]
    fn dragging_reorders_without_toggling_and_outside_drop_cancels() {
        let original = vec![Widget::Weather, Widget::Devbox, Widget::Projects];
        let start = Point::new(70., FIRST_ROW + 20.);
        let mut drag = ReorderDrag::new(Widget::Weather, original.clone(), start);
        let mut order = original.clone();
        let end = Point::new(70., FIRST_ROW + ROW_STEP * 2. + 20.);
        drag.preview(end, &mut order);
        assert_eq!(order.last(), Some(&Widget::Weather));
        assert_eq!(drag.finish(end, order.clone()), DragResult::Reorder(order));
        let mut drag = ReorderDrag::new(Widget::Weather, original.clone(), start);
        let mut order = original.clone();
        drag.preview(end, &mut order);
        assert_eq!(
            drag.finish(Point::new(-25., end.y), order),
            DragResult::Cancel(original)
        );
    }
    #[test]
    fn small_pointer_jitter_is_a_click() {
        let start = Point::new(70., FIRST_ROW + 20.);
        let mut order = vec![Widget::Weather, Widget::Devbox, Widget::Projects];
        let mut drag = ReorderDrag::new(Widget::Weather, order.clone(), start);
        let end = Point::new(72., start.y + 2.);
        drag.preview(end, &mut order);
        assert_eq!(drag.finish(end, order), DragResult::Click(Widget::Weather));
    }
    #[test]
    fn all_cards_fit_and_reordering_does_not_change_their_heights() {
        for height in [384., 624., 984., 1344.] {
            let mut order = vec![Widget::Weather, Widget::Devbox, Widget::Projects];
            let before = card_layout(&order, height);
            assert!(before.iter().map(|(_, h)| h).sum::<f32>() + CARD_GAP * 2. <= height + 0.01);
            order.reverse();
            let after = card_layout(&order, height);
            for (widget, h) in before {
                assert_eq!(after.iter().find(|(w, _)| *w == widget).unwrap().1, h);
            }
        }
    }
}
