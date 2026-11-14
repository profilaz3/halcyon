//! Arrange shows every widget exactly as the desktop draws it: the same
//! rectangle, the same content laid out the same way, with the same text
//! sizes. The editing chrome sits on the card's edges and never takes room
//! from its content or makes it scroll.
use super::*;
use crate::native_test::Native;
use cosmic::iced::{Rectangle, Size, Vector};

const SCREEN: [f32; 2] = [1920., 1080.];

/// What one view draws, on screen: its text and its scrolling areas.
#[derive(Default)]
struct Drawn {
    text: Vec<(String, Rectangle)>,
    scrolls: Vec<(Rectangle, Size)>,
}

/// The surface the desktop draws `widget` on, and that surface's rectangle
/// on screen, from the same settings the desktop creates it with.
fn desktop_surface(cards: &Cards, widget: Widget) -> (window::Id, Rectangle) {
    let (mi, mu) = cards.shadow_px();
    if widget.is_tool() {
        let at = cards.placed_position(widget);
        let size = cards.widget_size(widget).map(|v| v.ceil() + mu as f32);
        return (
            cards.widget_id(widget),
            Rectangle::new(
                Point::new(at[0].round() - mi as f32, at[1].round() - mi as f32),
                Size::new(size[0], size[1]),
            ),
        );
    }
    // The default right-hand column draws its cards on the Weather surface.
    let host = if !cards.custom_layout() && matches!(widget, Widget::Devbox | Widget::Projects) {
        Widget::Weather
    } else {
        widget
    };
    let surface = cards.card_surface(host);
    let (Some(width), Some(height)) = surface.size.unwrap() else {
        panic!("{widget:?}: a card surface has a size");
    };
    let (width, mut height) = (width as f32, height as f32);
    let m = surface.margin;
    let x = if surface.anchor.contains(Anchor::RIGHT) && !surface.anchor.contains(Anchor::LEFT) {
        SCREEN[0] - m.right as f32 - width
    } else {
        m.left as f32
    };
    if height == 0. && surface.anchor.contains(Anchor::BOTTOM) {
        height = SCREEN[1] - m.top as f32 - m.bottom as f32;
    }
    (
        surface.id,
        Rectangle::new(Point::new(x, m.top as f32), Size::new(width, height)),
    )
}

fn close(a: Rectangle, b: Rectangle) -> bool {
    (a.x - b.x).abs() < 0.01
        && (a.y - b.y).abs() < 0.01
        && (a.width - b.width).abs() < 0.01
        && (a.height - b.height).abs() < 0.01
}

fn overlap(a: Rectangle, b: Rectangle) -> f32 {
    let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    w.max(0.) * h.max(0.)
}

fn fixture(placed: bool, material: bool, glance: bool) -> Cards {
    let mut config = Config::default();
    config.location = Some(crate::config::Location {
        name: "Belgrade".into(),
        latitude: 44.8,
        longitude: 20.5,
        timezone: None,
    });
    config.enabled_widgets = ["clock", "weather", "calendar", "devbox", "projects"]
        .map(String::from)
        .into();
    if material {
        config.materials.shared = Some(crate::materials::Finish::new(
            crate::materials::Kind::ALL[0],
        ));
    }
    if placed {
        config.music.enabled = true;
        config.notes.enabled = true;
        config.focus.enabled = true;
        for (key, at) in [
            ("clock", [0.03, 0.05]),
            ("calendar", [0.03, 0.5]),
            ("weather", [0.97, 0.06]),
            ("devbox", [0.97, 0.62]),
            ("projects", [0.6, 0.6]),
            ("music", [0.45, 0.1]),
            ("notes", [0.3, 0.75]),
            ("focus", [0.6, 0.15]),
        ] {
            config.widget_positions.insert(key.into(), at);
        }
        // A saved size, and one smaller than the System monitor's content,
        // which the desktop grows to fit.
        config.widget_sizes.insert("weather".into(), [420., 240.]);
        config.widget_sizes.insert("devbox".into(), [380., 200.]);
    }
    if glance {
        config.glance.enabled = true;
        config.glance.initialized = true;
        config.glance.style = crate::config::GlanceStyle::Panel;
        config
            .widget_positions
            .insert("glance".into(), [0.35, 0.05]);
    }
    config.placement.snap = false;
    // Saved as the desktop reads it, so Arrange saves over these preferences.
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let config = Config::load();
    let (mut cards, _) = Cards::init(Core::default(), config);
    std::mem::forget(cards.test_output_ready());
    cards.screen_size = SCREEN;
    super::super::look_render_tests::sample_data(&mut cards);
    cards
}

/// Whether the desktop draws `widget` now. Arrange also shows a tool that
/// is waiting, such as Music with nothing playing, so it can be placed.
fn on_desktop(cards: &Cards, widget: Widget) -> bool {
    cards.arranging_widget(widget) && (!widget.is_tool() || cards.tool_shown(widget))
}

/// Everything the desktop draws for the visible widgets, on screen, and a
/// picture of it with every surface in its place.
fn desktop(cards: &Cards, theme: &cosmic::Theme, name: &str) -> Drawn {
    let mut surfaces: Vec<(window::Id, Rectangle)> = Vec::new();
    for widget in Widget::ALL {
        if !on_desktop(cards, widget) {
            continue;
        }
        let surface = desktop_surface(cards, widget);
        if !surfaces.iter().any(|(id, _)| *id == surface.0) {
            surfaces.push(surface);
        }
    }
    let mut drawn = Drawn::default();
    let mut layers: Vec<Element<'_, Message>> = vec![
        container(cosmic::iced::widget::space())
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(|_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.03, 0.03, 0.05))),
                    ..Default::default()
                }
            }))
            .into(),
    ];
    for (id, rect) in surfaces {
        let mut native = Native::new(rect.size());
        let offset = Vector::new(rect.x, rect.y);
        drawn.text.extend(
            native
                .placed_text(cards.surface_view(id))
                .into_iter()
                .map(|(text, bounds)| (text, bounds + offset)),
        );
        drawn.scrolls.extend(
            native
                .scrollables(cards.surface_view(id))
                .into_iter()
                .map(|(bounds, content)| (bounds + offset, content)),
        );
        layers.push(
            pin(container(cards.surface_view(id))
                .width(rect.width)
                .height(rect.height))
            .position(rect.position())
            .into(),
        );
    }
    let mut screen = Native::new(Size::new(SCREEN[0], SCREEN[1]));
    screen.capture(
        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        theme,
        &format!("{name}-desktop"),
    );
    drawn
}

fn check(name: &str, placed: bool, material: bool, glance: bool) {
    let theme = cosmic::Theme::dark();
    let mut cards = fixture(placed, material, glance);
    assert_eq!(cards.custom_layout(), placed, "{name}");
    let shown: Vec<Widget> = Widget::ALL
        .into_iter()
        .filter(|w| cards.arranging_widget(*w))
        .collect();
    // Daily overview takes the place of the Clock, Weather and Calendar.
    let expected = match (placed, glance) {
        (_, true) => 6,
        (true, false) => 8,
        (false, false) => 5,
    };
    assert_eq!(shown.len(), expected, "{name}: {shown:?}");
    let sizes: Vec<[f32; 2]> = shown.iter().map(|w| cards.widget_size(*w)).collect();
    let places: Vec<[f32; 2]> = shown.iter().map(|w| cards.placed_position(*w)).collect();
    let surfaces: Vec<Rectangle> = shown
        .iter()
        .map(|w| desktop_surface(&cards, *w).1)
        .collect();
    let drawn: Vec<bool> = shown.iter().map(|w| on_desktop(&cards, *w)).collect();
    let on_screen = desktop(&cards, &theme, name);
    assert!(
        on_screen.text.iter().any(|(t, _)| t == "System"),
        "{name}: the cards render"
    );

    let _ = cards.arrange();
    assert!(cards.layout_editor.active, "{name}");
    let mut native = Native::new(Size::new(SCREEN[0], SCREEN[1]));
    let arranged = Drawn {
        text: native.placed_text(cards.layout_view()),
        scrolls: native.scrollables(cards.layout_view()),
    };
    native.capture(cards.layout_view(), &theme, &format!("{name}-arrange"));

    let (mi, _) = cards.shadow_px();
    for (index, widget) in shown.iter().enumerate() {
        // Arranging changes neither the card's size nor its place.
        assert_eq!(
            cards.widget_size(*widget),
            sizes[index],
            "{name} {widget:?}"
        );
        let at = cards.arrange_origin(
            *widget,
            cards.position_from(*widget, &cards.layout_editor.draft),
        );
        let desktop_at = cards.arrange_origin(*widget, places[index]);
        assert!(
            !drawn[index]
                || (at[0] - desktop_at[0]).abs() < 0.01 && (at[1] - desktop_at[1]).abs() < 0.01,
            "{name} {widget:?}: {at:?} in Arrange, {desktop_at:?} on the desktop"
        );
        let card = Rectangle::new(
            Point::new(at[0], at[1]),
            Size::new(sizes[index][0], sizes[index][1]),
        );
        let outline = native.bounds(
            cards.layout_view(),
            &format!("arrange-card-{}", widget.key()),
        );
        assert!(
            close(outline, card.expand(3.)),
            "{name} {widget:?}: Arrange outlines {outline:?} around the card {card:?}"
        );
        // A placed card's surface is the card plus its shadow margin.
        if drawn[index] && (placed || widget.is_tool() || *widget == Widget::Glance) {
            let surface = surfaces[index];
            assert!(
                close(surface.shrink(mi as f32), card),
                "{name} {widget:?}: the desktop draws {:?}, Arrange {card:?}",
                surface.shrink(mi as f32)
            );
        }
        // The chrome never covers what the card shows.
        let content: Vec<&(String, Rectangle)> = if drawn[index] {
            &on_screen.text
        } else {
            &arranged.text
        }
        .iter()
        // Not the chrome's own labels.
        .filter(|(text, bounds)| {
            !text.starts_with('⠿') && text != "◢" && overlap(*bounds, card) > 0.5 * bounds.area()
        })
        .collect();
        assert!(
            !content.is_empty(),
            "{name} {widget:?}: the card draws text"
        );
        for part in ["name", "settings", "remove", "grip"] {
            let id = format!("arrange-{part}-{}", widget.key());
            let chrome = native.bounds(cards.layout_view(), &id);
            for (text, bounds) in &content {
                // Where the letters are: a line's box has room above and
                // below them.
                let ink = Rectangle {
                    y: bounds.y + bounds.height * 0.18,
                    height: bounds.height * 0.64,
                    ..*bounds
                };
                assert!(
                    overlap(chrome, ink) < 0.5,
                    "{name} {widget:?}: {id} {chrome:?} covers {text:?} at {bounds:?}"
                );
            }
        }
    }
    // Neighbors' chrome never meets, so every handle stays reachable.
    let chrome: Vec<(Widget, String, Rectangle)> = shown
        .iter()
        .flat_map(|widget| {
            ["name", "settings", "remove", "grip"].map(|part| {
                let id = format!("arrange-{part}-{}", widget.key());
                (*widget, id.clone(), native.bounds(cards.layout_view(), &id))
            })
        })
        .collect();
    for (a, a_id, a_rect) in &chrome {
        for (b, b_id, b_rect) in &chrome {
            assert!(
                a == b || overlap(*a_rect, *b_rect) == 0.,
                "{name}: {a_id} {a_rect:?} meets {b_id} {b_rect:?}"
            );
        }
    }
    // The same text, at the same place and size.
    for (text, bounds) in &on_screen.text {
        assert!(
            arranged
                .text
                .iter()
                .any(|(t, b)| t == text && close(*b, *bounds)),
            "{name}: {text:?} is at {bounds:?} on the desktop, in Arrange at {:?}",
            arranged
                .text
                .iter()
                .filter(|(t, _)| t == text)
                .map(|(_, b)| *b)
                .collect::<Vec<_>>()
        );
    }
    // The same scrolling areas, holding the same content: nothing that fits
    // on the desktop scrolls in Arrange.
    for (bounds, content) in &on_screen.scrolls {
        assert!(
            arranged
                .scrolls
                .iter()
                .any(|(b, c)| close(*b, *bounds) && (c.height - content.height).abs() < 0.01),
            "{name}: a scrolling area {bounds:?} holding {content:?} on the desktop, in Arrange {:?}",
            arranged.scrolls
        );
    }
    if placed {
        let first = if glance {
            Widget::Glance
        } else {
            Widget::Weather
        };
        resize_round_trip(&mut cards, &mut native, first);
        resize_round_trip(&mut cards, &mut native, Widget::Devbox);
    }
    let overflowing = |drawn: &Drawn| {
        drawn
            .scrolls
            .iter()
            .filter(|(b, c)| c.height > b.height + 0.5)
            .count()
    };
    assert_eq!(
        overflowing(&arranged),
        overflowing(&on_screen),
        "{name}: Arrange adds no scrollbars"
    );
}

/// A pointer event on the Arrange overlay, as the compositor sends it.
fn pointer(cards: &mut Cards, native: &mut Native, point: Point, press: Option<bool>) {
    let event = match press {
        Some(true) => mouse::Event::ButtonPressed(mouse::Button::Left),
        Some(false) => mouse::Event::ButtonReleased(mouse::Button::Left),
        None => mouse::Event::CursorMoved { position: point },
    };
    let (messages, _) = native.event(
        cards.layout_view(),
        cosmic::iced::Event::Mouse(event),
        point,
    );
    for message in messages {
        let _ = cards.update(message);
    }
    let _ = cards.layout_pointer(event);
}

/// Resizing with the corner handle gives the card exactly the rectangle the
/// desktop draws once Arrange is done.
fn resize_round_trip(cards: &mut Cards, native: &mut Native, widget: Widget) {
    if !cards.layout_editor.active {
        let _ = cards.arrange();
    }
    let before = native
        .bounds(
            cards.layout_view(),
            &format!("arrange-card-{}", widget.key()),
        )
        .shrink(3.);
    let grip = native
        .bounds(
            cards.layout_view(),
            &format!("arrange-grip-{}", widget.key()),
        )
        .center();
    pointer(cards, native, grip, None);
    pointer(cards, native, grip, Some(true));
    assert!(
        cards.layout_editor.drag.as_ref().is_some_and(|d| d.resize),
        "{widget:?}: the corner handle resizes"
    );
    let target = Point::new(grip.x + 30., grip.y + 52.);
    pointer(cards, native, target, None);
    pointer(cards, native, target, Some(false));
    let resized = native
        .bounds(
            cards.layout_view(),
            &format!("arrange-card-{}", widget.key()),
        )
        .shrink(3.);
    assert!(
        (resized.width - before.width - 30.).abs() < 0.01
            && (resized.height - before.height - 52.).abs() < 0.01,
        "{widget:?}: {before:?} resized to {resized:?}"
    );
    let _ = cards.finish_arrange(true);
    assert!(!cards.layout_editor.active);
    let (mi, _) = cards.shadow_px();
    let (_, surface) = desktop_surface(cards, widget);
    assert!(
        close(surface.shrink(mi as f32), resized),
        "{widget:?}: resized to {resized:?} in Arrange, the desktop draws {:?}",
        surface.shrink(mi as f32)
    );
}

#[test]
fn arrange_draws_every_card_exactly_as_the_desktop_does() {
    if crate::displays::test_support::isolated(
        "app::layout::arrange_match_tests::arrange_draws_every_card_exactly_as_the_desktop_does",
    ) {
        return;
    }
    for (name, placed, material, glance) in [
        ("arrange-match-default-material", false, true, false),
        ("arrange-match-default-plain", false, false, false),
        ("arrange-match-placed-material", true, true, false),
        ("arrange-match-placed-plain", true, false, false),
        ("arrange-match-glance-material", true, true, true),
    ] {
        check(name, placed, material, glance);
    }
}
