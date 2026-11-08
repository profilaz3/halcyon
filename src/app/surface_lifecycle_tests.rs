//! Windows exist only while they show something. Each open layer surface
//! holds its own swapchain buffers and renderer, so a hidden widget, the
//! folded-away edge menu and the idle arrange surface close (or shrink)
//! instead of waiting off screen.
use super::*;
use crate::displays::{
    OutputEvent,
    test_support::{Wire, info, isolated},
};
use cosmic::iced::futures::StreamExt;
use cosmic::iced::keyboard;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::Action as LayerAction;
use cosmic::iced::runtime::{Action as RuntimeAction, platform_specific, task};
use std::collections::BTreeMap;

/// The layer surfaces the compositor would have open, built by applying
/// every create and destroy in the order the app sends them.
#[derive(Default)]
struct Desk {
    open: BTreeMap<window::Id, SctkLayerSurfaceSettings>,
    destroyed: Vec<window::Id>,
}

impl Desk {
    fn run(&mut self, cards: &mut Cards, task: Task<Message>) {
        let mut pending = vec![task];
        while let Some(task) = pending.pop() {
            let Some(mut stream) = task::into_stream(task) else {
                continue;
            };
            cosmic::iced::futures::executor::block_on(async {
                while let Some(action) = stream.next().await {
                    match action {
                        RuntimeAction::Output(cosmic::Action::App(message)) => {
                            pending.push(cards.update(message))
                        }
                        RuntimeAction::PlatformSpecific(platform_specific::Action::Wayland(
                            platform_specific::wayland::Action::LayerSurface(action),
                        )) => match action {
                            LayerAction::LayerSurface { builder } => {
                                assert!(
                                    !self.open.contains_key(&builder.id),
                                    "{} created twice",
                                    builder.namespace
                                );
                                self.open.insert(builder.id, builder);
                            }
                            LayerAction::Destroy(id) => {
                                self.destroyed.push(id);
                                self.open.remove(&id);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            });
        }
    }
    fn send(&mut self, cards: &mut Cards, message: Message) {
        let task = cards.update(message);
        self.run(cards, task);
    }
    /// The compositor sizes one of the app's own surfaces (`Cards::ids`).
    fn resize(&mut self, cards: &mut Cards, index: usize, width: f32, height: f32) {
        let id = cards.ids[index];
        self.send(
            cards,
            Message::SurfaceResized(id, cosmic::iced::Size::new(width, height)),
        );
    }
    /// The open windows by name, without the `halcyon-` prefix.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .open
            .values()
            .map(|s| s.namespace.trim_start_matches("halcyon-").to_owned())
            .collect();
        names.sort();
        names
    }
    fn window(&self, cards: &Cards, widget: Widget) -> Option<&SctkLayerSurfaceSettings> {
        self.open.get(&cards.widget_id(widget))
    }
}

/// A 1920×1080 display as the compositor reports it: the output, then the
/// arrange strip along the top (the usable width) and the edge strip (the
/// usable height).
fn start(config: Config) -> (Cards, Desk, Wire) {
    // Showing and hiding saves through the file, so it holds the fixture.
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let wire = Wire::new();
    let (mut cards, _) = Cards::init(Core::default(), Config::try_load().unwrap());
    let mut desk = Desk::default();
    desk.send(
        &mut cards,
        Message::OutputChanged(
            OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
            wire.output(1),
        ),
    );
    let probe = desk
        .open
        .get(&cards.ids[5])
        .expect("the arrange surface measures the display");
    assert_eq!(probe.size, Some((Some(0), Some(1))), "a strip a pixel tall");
    assert_eq!(probe.anchor, Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
    assert!(
        desk.open.contains_key(&cards.ids[3]),
        "the edge strip measures the height"
    );
    desk.resize(&mut cards, 5, 1920., 1.);
    assert!(!cards.output_known, "the width alone isn't the display");
    desk.resize(&mut cards, 3, 2., 1080.);
    assert!(cards.output_known);
    assert_eq!(cards.screen_size, [1920., 1080.]);
    (cards, desk, wire)
}

/// Like the measured desktop: Clock and a Focus timer on screen, everything
/// else off, with saved places.
fn clock_and_focus() -> Config {
    let mut config = Config::default();
    config.enabled_widgets = vec!["clock".into()];
    config.glance.enabled = false;
    config.glance.initialized = true;
    config.focus.enabled = true;
    config.notes.enabled = false;
    config.music.enabled = false;
    config.widget_positions.insert("clock".into(), [0.06, 0.12]);
    config.widget_positions.insert("focus".into(), [0.93, 0.75]);
    config.widget_positions.insert("weather".into(), [0.5, 0.5]);
    config
}

#[test]
fn hidden_widgets_have_no_window_and_showing_one_restores_it_with_its_state() {
    if isolated(
        "app::surface_lifecycle_tests::hidden_widgets_have_no_window_and_showing_one_restores_it_with_its_state",
    ) {
        return;
    }
    let (mut cards, mut desk, _wire) = start(clock_and_focus());
    assert!(cards.custom_layout());
    // Only what draws something: the two widgets, the edge strip that
    // reveals the edge menu, and the arrange strip that measures the width.
    assert_eq!(desk.names(), ["arrange", "clock", "edge", "focus"]);
    for widget in [
        Widget::Weather,
        Widget::Calendar,
        Widget::Devbox,
        Widget::Projects,
        Widget::Glance,
        Widget::Music,
        Widget::Notes,
    ] {
        assert!(
            desk.window(&cards, widget).is_none(),
            "{widget:?} has a window"
        );
        assert!(!cards.card_surface_open(widget) && !cards.tool_surface_open(widget));
    }

    // A running focus round survives its widget being hidden.
    let today = crate::tools::focus::today();
    cards
        .tools
        .focus
        .toggle(crate::tools::focus::now_ms(), &cards.config.focus, &today);
    assert!(cards.tools.focus.running());
    let session = cards.tools.focus.clone();
    let focus_window = desk.window(&cards, Widget::Focus).cloned().unwrap();
    let clock = cards.widget_id(Widget::Clock);

    desk.send(&mut cards, Message::ToggleWidget(Widget::Focus));
    assert!(!cards.config.widget_visible(Widget::Focus));
    assert!(desk.window(&cards, Widget::Focus).is_none());
    assert!(!cards.tool_surface_open(Widget::Focus));
    assert_eq!(cards.tools.focus, session);
    assert!(
        cards.tools_wake().is_some(),
        "the round still wakes the app"
    );

    desk.send(&mut cards, Message::ToggleWidget(Widget::Focus));
    let reopened = desk.window(&cards, Widget::Focus).expect("Focus is back");
    assert_eq!(reopened.size, focus_window.size);
    assert_eq!(
        (reopened.margin.top, reopened.margin.left),
        (focus_window.margin.top, focus_window.margin.left),
        "in its saved place"
    );
    assert_eq!(reopened.input_zone, focus_window.input_zone);
    assert_eq!(cards.tools.focus, session);
    assert!(cards.tools.focus.running());

    // A card opens alone, where it was saved, and closes again; the other
    // windows are never recreated.
    desk.destroyed.clear();
    desk.send(&mut cards, Message::ToggleWidget(Widget::Weather));
    let weather = desk
        .window(&cards, Widget::Weather)
        .expect("Weather opens")
        .clone();
    let place = cards.placed_position(Widget::Weather);
    let (inset, _) = cards.shadow_px();
    assert_eq!(weather.margin.left, place[0].round() as i32 - inset);
    assert_eq!(weather.margin.top, place[1].round() as i32 - inset);
    assert!(!desk.destroyed.contains(&clock));
    assert_eq!(
        desk.names(),
        ["arrange", "clock", "edge", "focus", "weather"]
    );
    desk.send(&mut cards, Message::ToggleWidget(Widget::Weather));
    assert!(desk.window(&cards, Widget::Weather).is_none());
    assert_eq!(
        cards.config.widget_positions.get("weather"),
        Some(&[0.5, 0.5])
    );
    assert!(!desk.destroyed.contains(&clock));

    // Hiding every widget at once closes all of their windows.
    let mut hidden = cards.config.clone();
    hidden.widgets_visible = false;
    let task = cards.update(Message::Preview);
    desk.run(&mut cards, task);
    let task = cards.apply_config(hidden);
    desk.run(&mut cards, task);
    let task = cards.update(Message::Preview);
    desk.run(&mut cards, task);
    assert_eq!(desk.names(), ["arrange", "edge"]);
    assert_eq!(cards.tools.focus, session);
    let mut shown = cards.config.clone();
    shown.widgets_visible = true;
    let task = cards.apply_config(shown);
    desk.run(&mut cards, task);
    let task = cards.update(Message::Preview);
    desk.run(&mut cards, task);
    assert_eq!(desk.names(), ["arrange", "clock", "edge", "focus"]);
    assert!(cards.tools.focus.running());
}

#[test]
fn fixed_columns_open_only_the_columns_that_hold_cards() {
    if isolated("app::surface_lifecycle_tests::fixed_columns_open_only_the_columns_that_hold_cards")
    {
        return;
    }
    let mut config = Config::default();
    config.enabled_widgets = vec!["clock".into(), "calendar".into()];
    config.glance.enabled = false;
    config.glance.initialized = true;
    config.focus.enabled = false;
    config.notes.enabled = false;
    config.music.enabled = false;
    config.widget_positions.clear();
    let (mut cards, mut desk, _wire) = start(config);
    assert!(!cards.custom_layout());
    // Devbox and Projects live in the right-hand column (the Weather
    // surface), which holds no card yet.
    assert_eq!(desk.names(), ["arrange", "calendar", "clock", "edge"]);
    desk.send(&mut cards, Message::ToggleWidget(Widget::Devbox));
    assert_eq!(
        desk.names(),
        ["arrange", "calendar", "clock", "edge", "weather"]
    );
    assert!(desk.window(&cards, Widget::Devbox).is_none());
    desk.send(&mut cards, Message::ToggleWidget(Widget::Devbox));
    assert_eq!(desk.names(), ["arrange", "calendar", "clock", "edge"]);
}

#[test]
fn edge_menu_window_exists_only_while_the_menu_is_out() {
    if isolated("app::surface_lifecycle_tests::edge_menu_window_exists_only_while_the_menu_is_out")
    {
        return;
    }
    let mut config = clock_and_focus();
    config.sidebar_slide = false;
    let (mut cards, mut desk, _wire) = start(config);
    let controls = cards.ids[2];
    assert!(!desk.open.contains_key(&controls));
    // Brushing the edge waits out the reveal delay without a window.
    desk.send(&mut cards, Message::Hover(true, true));
    assert!(!desk.open.contains_key(&controls));
    desk.send(&mut cards, Message::Hover(true, false));
    desk.send(&mut cards, Message::ShowControls);
    assert!(desk.open.contains_key(&controls));
    desk.send(&mut cards, Message::Animate);
    assert_eq!(cards.sidebar.margin(), 0);
    desk.send(&mut cards, Message::HideControls);
    desk.send(&mut cards, Message::Animate);
    assert!(!desk.open.contains_key(&controls), "folded away");
    desk.send(&mut cards, Message::ShowControls);
    assert!(desk.open.contains_key(&controls), "and back");
}

#[test]
fn arranging_covers_the_display_and_the_strip_returns_afterwards() {
    if isolated(
        "app::surface_lifecycle_tests::arranging_covers_the_display_and_the_strip_returns_afterwards",
    ) {
        return;
    }
    let (mut cards, mut desk, _wire) = start(clock_and_focus());
    desk.send(&mut cards, Message::Arrange);
    assert!(cards.layout_editor.active);
    let overlay = desk.open.get(&cards.ids[5]).unwrap();
    assert_eq!(overlay.size, Some((Some(0), Some(0))));
    assert_eq!(
        overlay.anchor,
        Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
    );
    // Arranging keeps every shown widget's window, so leaving is instant.
    assert!(desk.window(&cards, Widget::Clock).is_some());
    assert!(desk.window(&cards, Widget::Focus).is_some());
    desk.resize(&mut cards, 5, 1920., 1080.);
    assert_eq!(cards.screen_size, [1920., 1080.]);
    let overlay = cards.ids[5];
    desk.send(
        &mut cards,
        Message::SurfaceInput(
            overlay,
            Box::new(Message::LayoutKey(
                keyboard::Key::Named(keyboard::key::Named::Escape),
                false,
            )),
        ),
    );
    assert!(!cards.layout_editor.active);
    let strip = desk.open.get(&cards.ids[5]).unwrap();
    assert_eq!(strip.size, Some((Some(0), Some(1))));
    desk.resize(&mut cards, 5, 1920., 1.);
    assert_eq!(
        cards.screen_size,
        [1920., 1080.],
        "a pixel-tall strip keeps the height"
    );
    assert_eq!(desk.names(), ["arrange", "clock", "edge", "focus"]);
}
