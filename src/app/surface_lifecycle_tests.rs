//! Windows exist only while they show something. Each open layer surface
//! holds its own swapchain buffers and renderer, so a hidden widget, the
//! folded-away edge menu and the idle arrange surface close (or shrink)
//! instead of waiting off screen, and a second display is measured by thin
//! strips rather than a window its size.
use super::*;
use crate::config::edge_menu::Side;
use crate::config::monitors::WidgetMonitor;
use crate::displays::WlOutput;
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
                            LayerAction::Margin { id, margin } => {
                                if let Some(surface) = self.open.get_mut(&id) {
                                    surface.margin = margin;
                                }
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
    /// The open windows on one display.
    fn on(&self, output: &WlOutput) -> Vec<&SctkLayerSurfaceSettings> {
        self.open
            .values()
            .filter(|s| matches!(&s.output, IcedOutput::Output(o) if o == output))
            .collect()
    }
    /// The open windows on one display by name, without the `halcyon-` prefix.
    fn names_on(&self, output: &WlOutput) -> Vec<String> {
        let mut names: Vec<String> = self
            .on(output)
            .into_iter()
            .map(|s| s.namespace.trim_start_matches("halcyon-").to_owned())
            .collect();
        names.sort();
        names
    }
    /// The compositor sizes one of the other display's strips: 0 is the one
    /// along the top (its usable width), 1 the one along the left (its usable
    /// height).
    fn resize_probe(&mut self, cards: &mut Cards, strip: usize, width: f32, height: f32) {
        let id = cards.extra_probes[0].ids[strip];
        self.send(
            cards,
            Message::SurfaceResized(id, cosmic::iced::Size::new(width, height)),
        );
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

/// Clock and Focus on the first display (DP-1), Weather on a second (DP-2).
fn weather_on_a_second_display() -> Config {
    let mut config = clock_and_focus();
    config.enabled_widgets = vec!["clock".into(), "weather".into()];
    config.widget_display = Some("DP-1".into());
    config.widget_monitors.insert(
        "weather".into(),
        WidgetMonitor {
            display: Some("DP-2".into()),
            hide_disconnected: false,
        },
    );
    config.widget_positions.insert("weather".into(), [0.8, 0.3]);
    config.widget_sizes.insert("weather".into(), [700., 500.]);
    config
}

/// DP-2's usable area: 2560×1440 less a 32 px panel along the top, a 48 px
/// dock along the bottom, a 64 px dock on the left and a 40 px one on the
/// right, each with an exclusive zone.
const DP2_USABLE: [f32; 2] = [2560. - 64. - 40., 1440. - 32. - 48.];

/// `start`, then a 2560×1440 display joins. Every window moves to a fresh
/// one, so DP-1 is measured again; DP-2 waits for its own strips.
fn start_two() -> (Cards, Desk, Wire, WlOutput) {
    let (mut cards, mut desk, wire) = start(weather_on_a_second_display());
    let external = wire.output(2);
    desk.send(
        &mut cards,
        Message::OutputChanged(
            OutputEvent::Created(Some(info(Some("DP-2"), (2560, 1440)))),
            external.clone(),
        ),
    );
    assert_eq!(cards.extra_probes.len(), 1);
    desk.resize(&mut cards, 5, 1920., 1.);
    desk.resize(&mut cards, 3, 2., 1080.);
    assert!(cards.output_known);
    (cards, desk, wire, external)
}

/// The compositor sizes DP-2's strips inside what its panels leave free.
fn measure_dp2(cards: &mut Cards, desk: &mut Desk, usable: [f32; 2]) {
    desk.resize_probe(cards, 0, usable[0], 1.);
    desk.resize_probe(cards, 1, 1., usable[1]);
}

/// A display that isn't drawn on is only measured: two invisible strips a
/// pixel thick, one sized to its usable width and one to its usable height,
/// that never take the pointer or the keyboard.
fn assert_measured_by_strips(desk: &Desk, cards: &Cards, output: &WlOutput) {
    let probe = cards
        .extra_probes
        .iter()
        .find(|p| &p.output.handle == output)
        .expect("the display has a probe");
    let thin = Some(layout::PROBE_THICKNESS);
    let strips = probe
        .ids
        .map(|id| desk.open.get(&id).expect("its strips are open"));
    assert_eq!(strips[0].anchor, Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
    assert_eq!(
        strips[0].size,
        Some((Some(0), thin)),
        "as wide as the usable area"
    );
    assert_eq!(
        strips[1].anchor,
        Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM
    );
    assert_eq!(
        strips[1].size,
        Some((thin, Some(0))),
        "as tall as the usable area"
    );
    for strip in strips {
        assert_eq!(strip.namespace, "halcyon-display-probe");
        assert!(matches!(&strip.output, IcedOutput::Output(o) if o == output));
        assert_eq!(strip.layer, Layer::Bottom);
        assert_eq!(strip.keyboard_interactivity, KeyboardInteractivity::None);
        assert_eq!(strip.input_zone, Some(Vec::new()));
        // Placed inside what panels and docks leave free, never over them.
        assert_eq!(strip.exclusive_zone, 0);
    }
    // Nothing on the display covers it: every window there is a strip or a
    // widget.
    for surface in desk.on(output) {
        assert!(
            surface.anchor != Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            "{} covers the display",
            surface.namespace
        );
    }
}

#[test]
fn a_second_display_is_measured_by_two_strips_that_place_its_widgets() {
    if isolated(
        "app::surface_lifecycle_tests::a_second_display_is_measured_by_two_strips_that_place_its_widgets",
    ) {
        return;
    }
    let (mut cards, mut desk, _wire, external) = start_two();
    assert_measured_by_strips(&desk, &cards, &external);
    // Weather would draw nothing until its display is measured: no window.
    assert_eq!(desk.names_on(&external), ["display-probe", "display-probe"]);
    assert!(!cards.widget_available(Widget::Weather));

    desk.resize_probe(&mut cards, 0, DP2_USABLE[0], 1.);
    assert!(
        !cards.widget_available(Widget::Weather),
        "the width alone isn't the display"
    );
    assert!(desk.window(&cards, Widget::Weather).is_none());
    desk.resize_probe(&mut cards, 1, 1., DP2_USABLE[1]);
    assert!(cards.widget_available(Widget::Weather));
    assert_eq!(cards.widget_screen(Widget::Weather), DP2_USABLE);
    assert_eq!(cards.widget_screen(Widget::Clock), [1920., 1080.]);
    assert_eq!(cards.widget_size(Widget::Weather), [700., 500.]);

    // Weather opens on DP-2 at its saved fraction of the usable area, as
    // when a full-size window measured it.
    let place = crate::layout::decode([0.8, 0.3], [700., 500.], DP2_USABLE);
    assert_eq!(cards.placed_position(Widget::Weather), place);
    let weather = desk
        .window(&cards, Widget::Weather)
        .expect("Weather opens once DP-2 is measured");
    assert!(matches!(&weather.output, IcedOutput::Output(o) if *o == external));
    let (inset, _) = cards.shadow_px();
    assert_eq!(weather.margin.left, place[0].round() as i32 - inset);
    assert_eq!(weather.margin.top, place[1].round() as i32 - inset);
    assert_eq!(
        desk.names_on(&external),
        ["display-probe", "display-probe", "weather"]
    );
    assert_eq!(
        desk.names(),
        [
            "arrange",
            "clock",
            "display-probe",
            "display-probe",
            "edge",
            "focus",
            "weather"
        ]
    );
    assert_measured_by_strips(&desk, &cards, &external);

    // The left dock closes: the compositor widens the top strip, and Weather
    // moves into the room. The strips stay as they are.
    let strips = cards.extra_probes[0].ids;
    desk.destroyed.clear();
    let wider = [DP2_USABLE[0] + 64., DP2_USABLE[1]];
    desk.resize_probe(&mut cards, 0, wider[0], 1.);
    assert_eq!(cards.widget_screen(Widget::Weather), wider);
    let moved = crate::layout::decode([0.8, 0.3], [700., 500.], wider);
    assert_eq!(cards.placed_position(Widget::Weather), moved);
    assert!(strips.iter().all(|id| !desk.destroyed.contains(id)));
    assert_eq!(cards.extra_probes[0].ids, strips);
    // The bottom dock hides as well; a reconfigure of the same width changes
    // nothing.
    let taller = [wider[0], DP2_USABLE[1] + 48.];
    desk.resize_probe(&mut cards, 1, 1., taller[1]);
    assert_eq!(cards.widget_screen(Widget::Weather), taller);
    desk.destroyed.clear();
    desk.resize_probe(&mut cards, 0, taller[0], 1.);
    assert!(desk.destroyed.is_empty(), "the same size rebuilds nothing");
    // The saved place is a fraction and never changes with the panels.
    assert_eq!(cards.config.widget_positions["weather"], [0.8, 0.3]);
}

#[test]
fn arranging_a_second_display_covers_it_and_strips_measure_it_again_afterwards() {
    if isolated(
        "app::surface_lifecycle_tests::arranging_a_second_display_covers_it_and_strips_measure_it_again_afterwards",
    ) {
        return;
    }
    let (mut cards, mut desk, _wire, external) = start_two();
    measure_dp2(&mut cards, &mut desk, DP2_USABLE);
    let laptop = cards.selected_output.as_ref().unwrap().handle.clone();

    desk.send(&mut cards, Message::ArrangeDisplay("DP-2".into()));
    // DP-2 is now the display drawn on, measured by its own arrange and edge
    // strips; DP-1, which keeps Clock and Focus, is measured by a probe.
    assert_eq!(cards.selected_output.as_ref().unwrap().handle, external);
    assert!(!cards.output_known);
    desk.resize(&mut cards, 5, DP2_USABLE[0], 1.);
    desk.resize(&mut cards, 3, 2., DP2_USABLE[1]);
    assert!(cards.layout_editor.active);
    let overlay = desk.open.get(&cards.ids[5]).expect("the arrange surface");
    assert!(matches!(&overlay.output, IcedOutput::Output(o) if *o == external));
    assert_eq!(
        overlay.anchor,
        Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
    );
    assert_eq!(overlay.size, Some((Some(0), Some(0))), "covers DP-2");
    assert_eq!(
        overlay.keyboard_interactivity,
        KeyboardInteractivity::Exclusive
    );
    assert!(
        desk.on(&external)
            .iter()
            .all(|s| s.namespace != "halcyon-display-probe"),
        "DP-2 has no probe while it is arranged"
    );
    desk.resize(&mut cards, 5, DP2_USABLE[0], DP2_USABLE[1]);
    assert_eq!(cards.screen_size, DP2_USABLE);
    assert_measured_by_strips(&desk, &cards, &laptop);
    // Clock and Focus wait for DP-1's measurement, then open there.
    assert_eq!(desk.names_on(&laptop), ["display-probe", "display-probe"]);
    desk.resize_probe(&mut cards, 0, 1920., 1.);
    desk.resize_probe(&mut cards, 1, 1., 1080.);
    assert_eq!(cards.widget_screen(Widget::Clock), [1920., 1080.]);
    assert_eq!(
        desk.names_on(&laptop),
        ["clock", "display-probe", "display-probe", "focus"]
    );
    assert_measured_by_strips(&desk, &cards, &laptop);

    desk.send(&mut cards, Message::LayoutFinish(false));
    assert_eq!(cards.selected_output.as_ref().unwrap().handle, laptop);
    desk.resize(&mut cards, 5, 1920., 1.);
    desk.resize(&mut cards, 3, 2., 1080.);
    assert!(!cards.layout_editor.active);
    assert_measured_by_strips(&desk, &cards, &external);
    measure_dp2(&mut cards, &mut desk, DP2_USABLE);
    assert_eq!(cards.widget_screen(Widget::Weather), DP2_USABLE);
    assert_eq!(
        desk.names_on(&external),
        ["display-probe", "display-probe", "weather"]
    );
    assert_eq!(
        desk.names_on(&laptop),
        ["arrange", "clock", "edge", "focus"]
    );
    let strip = desk.open.get(&cards.ids[5]).unwrap();
    assert_eq!(strip.size, Some((Some(0), Some(layout::PROBE_THICKNESS))));
}

#[test]
fn unplugging_a_second_display_closes_its_strips_and_plugging_it_back_opens_new_ones() {
    if isolated(
        "app::surface_lifecycle_tests::unplugging_a_second_display_closes_its_strips_and_plugging_it_back_opens_new_ones",
    ) {
        return;
    }
    let (mut cards, mut desk, wire, external) = start_two();
    measure_dp2(&mut cards, &mut desk, DP2_USABLE);
    let place = cards.placed_position(Widget::Weather);
    let strips = cards.extra_probes[0].ids;

    desk.send(
        &mut cards,
        Message::OutputChanged(OutputEvent::Removed, external.clone()),
    );
    assert!(cards.extra_probes.is_empty());
    for id in strips {
        assert!(desk.destroyed.contains(&id) && !desk.open.contains_key(&id));
    }
    assert!(desk.on(&external).is_empty());
    desk.resize(&mut cards, 5, 1920., 1.);
    desk.resize(&mut cards, 3, 2., 1080.);
    // Weather waits on DP-1 while DP-2 is away.
    assert_eq!(
        desk.names(),
        ["arrange", "clock", "edge", "focus", "weather"]
    );
    assert_eq!(cards.widget_screen(Widget::Weather), [1920., 1080.]);

    let replacement = wire.output(3);
    desk.send(
        &mut cards,
        Message::OutputChanged(
            OutputEvent::Created(Some(info(Some("DP-2"), (2560, 1440)))),
            replacement.clone(),
        ),
    );
    desk.resize(&mut cards, 5, 1920., 1.);
    desk.resize(&mut cards, 3, 2., 1080.);
    assert_eq!(cards.extra_probes.len(), 1);
    let fresh = cards.extra_probes[0].ids;
    assert!(fresh.iter().all(|id| !strips.contains(id)), "new strips");
    assert_measured_by_strips(&desk, &cards, &replacement);
    assert_eq!(
        desk.names_on(&replacement),
        ["display-probe", "display-probe"]
    );
    measure_dp2(&mut cards, &mut desk, DP2_USABLE);
    assert_eq!(
        desk.names_on(&replacement),
        ["display-probe", "display-probe", "weather"]
    );
    assert_eq!(cards.widget_screen(Widget::Weather), DP2_USABLE);
    assert_eq!(
        cards.placed_position(Widget::Weather),
        place,
        "back in its place"
    );
    assert_eq!(
        desk.names()
            .iter()
            .filter(|n| *n == "display-probe")
            .count(),
        2,
        "the old strips are gone"
    );
}

/// The edge menu set to Off: no strip and no hover area at the edge, only an
/// invisible probe that still measures the display, and the menu still opens
/// from `halcyon --toggle drawer`. Turning it back on brings the strip live.
#[test]
fn edge_menu_off_leaves_nothing_at_the_edge_and_the_toggle_still_opens_it() {
    if isolated(
        "app::surface_lifecycle_tests::edge_menu_off_leaves_nothing_at_the_edge_and_the_toggle_still_opens_it",
    ) {
        return;
    }
    let mut config = clock_and_focus();
    config.sidebar_slide = false;
    config.edge_menu.enabled = false;
    let (mut cards, mut desk, _wire) = start(config);
    assert_eq!(desk.names(), ["arrange", "clock", "focus", "height-probe"]);
    let probe = &desk.open[&cards.ids[3]];
    assert_eq!(probe.input_zone, Some(vec![]), "takes no pointer input");
    assert_eq!(probe.layer, Layer::Bottom);
    assert!(!cards.is_edge_strip(cards.ids[3]));
    assert_eq!(cards.screen_size, [1920., 1080.], "and still measures");
    // `halcyon --toggle drawer` opens it all the same.
    desk.send(&mut cards, Message::DrawerToggle);
    assert!(cards.sidebar.visible);
    assert!(desk.open.contains_key(&cards.ids[2]));
    desk.send(&mut cards, Message::DrawerToggle);
    desk.send(&mut cards, Message::Animate);
    assert!(!desk.open.contains_key(&cards.ids[2]));
    // Back on, live: the strip replaces the probe on the right.
    let mut config = cards.config.clone();
    config.edge_menu.enabled = true;
    let task = cards.apply_config(config);
    desk.run(&mut cards, task);
    assert_eq!(desk.names(), ["arrange", "clock", "edge", "focus"]);
    let strip = &desk.open[&cards.ids[3]];
    assert_eq!(strip.anchor, Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM);
    assert_eq!(strip.input_zone, None);
    assert!(cards.is_edge_strip(cards.ids[3]));
}

/// Side, position and length move the strip and the menu, live.
#[test]
fn edge_menu_side_position_and_length_place_the_strip_and_the_menu() {
    if isolated(
        "app::surface_lifecycle_tests::edge_menu_side_position_and_length_place_the_strip_and_the_menu",
    ) {
        return;
    }
    let mut config = clock_and_focus();
    config.sidebar_slide = false;
    let (mut cards, mut desk, _wire) = start(config);
    let set = |cards: &mut Cards, desk: &mut Desk, change: &dyn Fn(&mut Config)| {
        let mut config = cards.config.clone();
        change(&mut config);
        let task = cards.apply_config(config);
        desk.run(cards, task);
    };
    // The left edge: the full strip and the menu both move there.
    set(&mut cards, &mut desk, &|c| c.edge_menu.side = Side::Left);
    let strip = &desk.open[&cards.ids[3]];
    assert_eq!(strip.namespace, "halcyon-edge");
    assert_eq!(strip.anchor, Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM);
    desk.send(&mut cards, Message::ShowControls);
    desk.send(&mut cards, Message::Animate);
    let menu = &desk.open[&cards.ids[2]];
    assert_eq!(menu.anchor, Anchor::LEFT, "centred on the left edge");
    assert_eq!((menu.margin.left, menu.margin.right), (0, 0));
    // An open menu follows a change at once.
    set(&mut cards, &mut desk, &|c| c.edge_menu.side = Side::Right);
    assert_eq!(desk.open[&cards.ids[2]].anchor, Anchor::RIGHT);
    desk.send(&mut cards, Message::HideControls);
    desk.send(&mut cards, Message::Animate);
    // Part of the edge, at the top: a strip of its own, held off the bottom.
    set(&mut cards, &mut desk, &|c| {
        c.edge_menu.length = 40;
        c.edge_menu.position = 0;
    });
    assert_eq!(
        desk.names(),
        ["arrange", "clock", "edge", "focus", "height-probe"]
    );
    let part = cards.edge.part.expect("a strip over part of the edge");
    let strip = &desk.open[&part.id];
    assert_eq!(strip.namespace, "halcyon-edge");
    assert_eq!(strip.anchor, Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM);
    assert_eq!((strip.margin.top, strip.margin.bottom), (0, 648));
    assert!(cards.is_edge_strip(part.id));
    assert_eq!(desk.open[&cards.ids[3]].input_zone, Some(vec![]));
    // The menu opens beside it, hanging from the top.
    desk.send(&mut cards, Message::ShowControls);
    desk.send(&mut cards, Message::Animate);
    let menu = &desk.open[&cards.ids[2]];
    assert_eq!(menu.anchor, Anchor::RIGHT | Anchor::TOP);
    assert_eq!(menu.margin.top, 0);
    desk.send(&mut cards, Message::HideControls);
    desk.send(&mut cards, Message::Animate);
    // At the bottom: the same strip moves without being replaced.
    set(&mut cards, &mut desk, &|c| c.edge_menu.position = 100);
    assert_eq!(cards.edge.part.map(|p| p.id), Some(part.id));
    let strip = &desk.open[&part.id];
    assert_eq!((strip.margin.top, strip.margin.bottom), (648, 0));
    desk.send(&mut cards, Message::ShowControls);
    desk.send(&mut cards, Message::Animate);
    let menu = &desk.open[&cards.ids[2]];
    assert_eq!(menu.anchor, Anchor::RIGHT | Anchor::TOP);
    assert_eq!(
        menu.margin.top,
        1080 - drawer::SIDEBAR_HEIGHT as i32,
        "kept on screen"
    );
    desk.send(&mut cards, Message::HideControls);
    desk.send(&mut cards, Message::Animate);
    // Full length again: the partial strip goes and the full one is back.
    set(&mut cards, &mut desk, &|c| c.edge_menu.length = 100);
    assert!(cards.edge.part.is_none());
    assert!(!desk.open.contains_key(&part.id));
    assert_eq!(desk.names(), ["arrange", "clock", "edge", "focus"]);
}
