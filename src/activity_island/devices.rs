//! Devices in the island: a short banner when one connects, a one-time
//! reveal when its battery falls to 20% and to 10%, and a card listing
//! every connected device with its battery. Nothing stays in the pill.
use super::*;
use crate::devices::Device;
use live::Ring;

/// An event older than this is not news: a restart never replays it.
const FRESH_MS: i64 = 20_000;
/// Events already shown, at most.
const SEEN: usize = 32;
/// A banner retracts after this long unless the pointer holds it.
const SHOWN: Duration = Duration::from_secs(4);
const ROW: f32 = 34.;
const ROW_GAP: f32 = 2.;
/// The room the chooser layout gives below the title line.
const ROOM: f32 = HEIGHT - 50.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    Connected,
    /// The battery fell to this level (one of `crate::devices::LOW`).
    Low(u8),
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Banner {
    pub device: Device,
    pub event: Event,
}
#[derive(Default)]
pub(super) struct Island {
    /// Events already seen: device id, when, and the level (0 for a connect).
    seen: Vec<(String, i64, u8)>,
    /// The first snapshot only marks what is there.
    primed: bool,
    /// What the open island announces.
    pub banner: Option<Banner>,
    /// The open island lists every device.
    pub card: bool,
}
impl Island {
    pub fn close(&mut self) {
        self.banner = None;
        self.card = false;
    }
}
#[derive(Debug, Clone)]
pub enum Msg {
    /// Open the island on the list of devices.
    Open,
}

/// The newest event in `list` not seen before, if the person wants it.
/// Every fresh event is marked seen, shown or not, so none comes back.
pub(super) fn pick(
    island: &mut Island,
    list: &[Device],
    prefs: &preferences::Preferences,
    now_ms: i64,
) -> Option<Banner> {
    let mut found: Vec<Banner> = Vec::new();
    for d in list {
        let fresh = |at: Option<i64>| at.filter(|at| now_ms.saturating_sub(*at) < FRESH_MS);
        let mut events = Vec::new();
        if let Some(at) = fresh(d.connected_ms) {
            events.push((at, 0, Event::Connected));
        }
        if let (Some(level), Some(at)) = (d.alert, fresh(d.alert_ms)) {
            events.push((at, level, Event::Low(level)));
        }
        for (at, level, event) in events {
            let key = (d.id.clone(), at, level);
            if island.seen.contains(&key) {
                continue;
            }
            island.seen.push(key);
            found.push(Banner {
                device: d.clone(),
                event,
            });
        }
    }
    let overflow = island.seen.len().saturating_sub(SEEN);
    island.seen.drain(..overflow);
    if !std::mem::replace(&mut island.primed, true) || !prefs.system {
        return None;
    }
    let wanted = |b: &&Banner| match b.event {
        Event::Connected => prefs.device_banners,
        Event::Low(_) => prefs.low_battery,
    };
    // A battery running low says more than a device arriving.
    found
        .iter()
        .filter(wanted)
        .filter(|b| matches!(b.event, Event::Low(_)))
        .min_by_key(|b| match b.event {
            Event::Low(level) => level,
            Event::Connected => u8::MAX,
        })
        .or_else(|| found.iter().find(|b| wanted(b)))
        .cloned()
}

impl Applet {
    /// The device the open island shows, as the desktop last reported it.
    fn banner_device(&self) -> Option<(&Device, Event)> {
        let b = self.devices.banner.as_ref()?;
        Some((
            self.snapshot
                .devices
                .iter()
                .find(|d| d.id == b.device.id)
                .unwrap_or(&b.device),
            b.event,
        ))
    }
    pub(super) fn devices_eligible(&self) -> bool {
        self.devices.banner.is_some() || (self.devices.card && self.kind == Kind::Devices)
    }
    /// A battery at 10% asks for the island like an urgent activity.
    pub(super) fn devices_urgent(&self) -> bool {
        matches!(
            self.devices.banner.as_ref().map(|b| b.event),
            Some(Event::Low(level)) if level <= crate::devices::LOW[1]
        )
    }
    /// A low battery is shown even right after another reveal.
    pub(super) fn devices_pressing(&self) -> bool {
        matches!(
            self.devices.banner.as_ref().map(|b| b.event),
            Some(Event::Low(_))
        )
    }
    /// Reads the snapshot's devices for news. A device connecting usually
    /// also arrives as a Bluetooth announcement; the banner takes its place.
    pub(super) fn device_reveal(&mut self, system: Option<Kind>, now: Instant) -> Option<Kind> {
        let banner = pick(
            &mut self.devices,
            &self.snapshot.devices,
            &self.snapshot.preferences,
            self.now_ms,
        );
        let bluetooth = Some(Kind::System(system::Source::Bluetooth));
        let Some(banner) = banner else {
            // A Bluetooth announcement while a banner shows adds nothing.
            return system.filter(|k| {
                !(Some(*k) == bluetooth && self.popup.is_some() && self.kind == Kind::Devices)
            });
        };
        // Already announcing this device's arrival: become the banner in place.
        if self.popup.is_some()
            && self.revealed
            && (Some(self.kind) == bluetooth || self.kind == Kind::Devices)
            && !self.devices.card
        {
            self.devices.banner = Some(banner);
            self.kind = Kind::Devices;
            self.reveal_deadline = Some(now + SHOWN);
            return system.filter(|k| Some(*k) != bluetooth);
        }
        match system {
            None => {
                self.devices.banner = Some(banner);
                Some(Kind::Devices)
            }
            Some(k) if Some(k) == bluetooth => {
                self.devices.banner = Some(banner);
                Some(Kind::Devices)
            }
            other => other,
        }
    }
    /// After a reveal was considered: a banner that could not show is
    /// dropped, and one that shows stays a little longer than others.
    pub(super) fn settle_device_banner(&mut self, now: Instant) {
        if self.kind != Kind::Devices || self.popup.is_none() {
            self.devices.banner = None;
        } else if self.revealed && self.devices.banner.is_some() && !self.devices.card {
            self.reveal_deadline = Some(now + SHOWN);
        }
    }
    pub(super) fn devices_update(&mut self, msg: Msg, now: Instant) -> Task<Message> {
        match msg {
            Msg::Open => {
                if !self.live() {
                    return Task::none();
                }
                self.devices.card = true;
                self.kind = Kind::Devices;
                self.ui.view = live::View::Card;
                self.batch.clear();
                self.selected_source = None;
                self.regrow(now)
            }
        }
    }
    pub(super) fn devices_content(&self) -> Content {
        if self.devices.card {
            return Content {
                title: "Devices".into(),
                ..Default::default()
            };
        }
        match self.banner_device() {
            Some((d, event)) => Content {
                // The name leads, so a narrow bar still shows whose it is.
                title: d.name.clone(),
                meta: banner_meta(d, event),
                ..Default::default()
            },
            None => Content {
                title: "Devices".into(),
                meta: "No devices connected".into(),
                ..Default::default()
            },
        }
    }
    /// The banner's colour: amber at 20%, the warning colour at 10%.
    pub(super) fn devices_accent(&self, palette: Palette) -> Option<Color> {
        if self.devices.card {
            return None;
        }
        match self.banner_device()?.1 {
            Event::Low(_) if self.devices_urgent() => Some(palette.warning()),
            Event::Low(_) => Some(palette.readable(accent::TIMER)),
            Event::Connected => None,
        }
    }
    /// The banner leads with the device's own icon.
    pub(super) fn devices_glyph<'a>(&self, accent: Color) -> Option<Element<'a, Message>> {
        if self.devices.card {
            return None;
        }
        let (d, _) = self.banner_device()?;
        Some(tinted(d.kind.icon(), 16, accent))
    }
    /// After the banner's title: the battery as a ring and a percent.
    pub(super) fn devices_trailing<'a>(
        &self,
        palette: Palette,
        accent: Color,
    ) -> Option<Element<'a, Message>> {
        if self.devices.card {
            return None;
        }
        let (d, _) = self.banner_device()?;
        let level = d.battery?;
        Some(
            Ring {
                size: 14.,
                stroke: 2.5,
                fraction: f32::from(level) / 100.,
                color: accent,
                track: palette.track,
            }
            .into(),
        )
    }
    /// The width the banner's ring takes from the title, with its spacing.
    pub(super) fn devices_trailing_room(&self) -> f32 {
        let shows = self.kind == Kind::Devices
            && !self.chooser
            && !self.devices.card
            && self
                .banner_device()
                .is_some_and(|(d, _)| d.battery.is_some());
        if shows { 14. + 8. } else { 0. }
    }
    pub(super) fn devices_extra(&self) -> f32 {
        let rows = self.snapshot.devices.len().max(1) as f32;
        (rows * (ROW + ROW_GAP) - ROW_GAP - ROOM).max(0.)
    }
    /// The open card: a row per device, with its battery.
    pub(super) fn devices_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        if self.snapshot.devices.is_empty() {
            return widget::container(self.text(
                "No devices connected",
                13.,
                alpha(palette.quiet, reveal),
            ))
            .width(Length::Fill)
            .height(ROOM)
            .align_x(cosmic::iced::Alignment::Center)
            .align_y(cosmic::iced::Alignment::Center)
            .padding([6, 10])
            .into();
        }
        let mut list = widget::column([]).spacing(ROW_GAP);
        for d in &self.snapshot.devices {
            let tone = ring_tone(d, palette, accent);
            let mut row = widget::row([
                tinted(d.kind.icon(), 16, alpha(palette.quiet, reveal)),
                widget::container(self.text(d.name.clone(), 13., alpha(palette.ink, reveal)))
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center);
            // A bolt, not a word, so a long name keeps its room.
            if let Some(level) = d.battery.filter(|_| d.charging()) {
                row = row.push(tinted(
                    &crate::tools::cards::charging_icon(level),
                    14,
                    alpha(palette.quiet, reveal),
                ));
            }
            row = row.push(battery(d, palette, alpha(tone, reveal), 18., 13.));
            list = list.push(
                widget::container(row)
                    .id(cosmic::widget::Id::new(format!("activity-device-{}", d.id)))
                    .width(Length::Fill)
                    .height(ROW)
                    .padding([6, 10]),
            );
        }
        list.width(Length::Fill).into()
    }
    /// The Idle card's way to the devices, while any is connected.
    pub(super) fn devices_button<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Option<Element<'a, Message>> {
        if self.snapshot.devices.is_empty() {
            return None;
        }
        Some(
            self.tip(
                palette,
                widget::button::icon(widget::icon::from_name("bluetooth-symbolic").symbolic(true))
                    .icon_size(16)
                    .padding([8, 14])
                    .width(44)
                    .height(32)
                    .id("activity-devices".into())
                    .class(capsule_button(palette, tint, reveal, Role::Quiet))
                    .on_press(Message::Devices(Msg::Open)),
                "Devices",
                widget::tooltip::Position::Top,
            ),
        )
    }
    /// The banner's and the Bluetooth card's buttons.
    pub(super) fn devices_actions<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        settings: bool,
    ) -> Element<'a, Message> {
        let mut row = widget::row([self.label_button(
            palette,
            "activity-devices-all",
            "All devices",
            tint,
            reveal,
            Role::Primary,
            Some(Message::Devices(Msg::Open)),
        )])
        .spacing(4)
        .align_y(cosmic::iced::Alignment::Center);
        if settings {
            row = row.push(self.label_button(
                palette,
                "activity-system-settings",
                // Under a Standard bar the card is narrower.
                if self.frame().attached {
                    "Settings"
                } else {
                    "Bluetooth settings"
                },
                tint,
                reveal,
                Role::Quiet,
                Some(Message::SystemSettings(system::Source::Bluetooth)),
            ));
        }
        row.into()
    }
}

/// A symbolic icon in one colour.
fn tinted<'a>(name: &str, size: u16, color: Color) -> Element<'a, Message> {
    widget::icon::from_name(name)
        .symbolic(true)
        .size(size)
        .icon()
        .class(cosmic::theme::Svg::custom(move |_| {
            cosmic::iced::widget::svg::Style { color: Some(color) }
        }))
        .into()
}
/// A ring's colour: the accent, amber at 20% and the warning colour at 10%
/// while it runs down.
fn ring_tone(d: &Device, palette: Palette, accent: Color) -> Color {
    match d.low() {
        Some(level) if level <= crate::devices::LOW[1] => palette.warning(),
        Some(_) => palette.readable(accent::TIMER),
        None => accent,
    }
}
/// “Connected · 80%” or “Battery low · 15%”, with “Charging” after it
/// while it charges.
fn banner_meta(d: &Device, event: Event) -> String {
    let mut parts = vec![match event {
        Event::Connected => "Connected".to_owned(),
        Event::Low(_) => "Battery low".to_owned(),
    }];
    parts.extend(d.percent());
    if d.charging() {
        parts.push("Charging".into());
    }
    parts.join(" · ")
}
/// A battery ring and its percent in tabular figures.

fn battery<'a>(
    d: &Device,
    palette: Palette,
    color: Color,
    ring: f32,
    size: f32,
) -> Element<'a, Message> {
    let mut row = widget::row([])
        .spacing(6)
        .align_y(cosmic::iced::Alignment::Center);
    if let Some(level) = d.battery {
        row = row
            .push(Ring {
                size: ring,
                stroke: if ring < 16. { 2.5 } else { 3. },
                fraction: f32::from(level) / 100.,
                color,
                track: palette.track,
            })
            .push(live::tabular(
                &format!("{level}%"),
                size,
                palette.ink,
                false,
            ));
    }
    row.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::Kind as DeviceKind;

    fn device(id: &str, battery: u8, charging: bool) -> Device {
        Device {
            id: id.into(),
            name: id.into(),
            kind: DeviceKind::Headphones,
            battery: Some(battery),
            charging: Some(charging),
            ..Default::default()
        }
    }
    fn prefs() -> preferences::Preferences {
        preferences::Preferences::default()
    }

    #[test]
    fn the_first_snapshot_is_silent_and_each_event_shows_once() {
        let mut island = Island::default();
        let mut d = device("bt:1", 80, false);
        d.connected_ms = Some(1_000);
        // What was there at the start is never announced.
        assert_eq!(pick(&mut island, &[d.clone()], &prefs(), 2_000), None);
        let mut e = device("bt:2", 60, false);
        e.connected_ms = Some(5_000);
        let b = pick(&mut island, &[d.clone(), e.clone()], &prefs(), 6_000).unwrap();
        assert_eq!((b.device.id.as_str(), b.event), ("bt:2", Event::Connected));
        assert_eq!(
            pick(&mut island, &[d.clone(), e.clone()], &prefs(), 7_000),
            None
        );
        // Low battery, once at 20% and once more at 10%.
        e.battery = Some(20);
        e.alert = Some(20);
        e.alert_ms = Some(8_000);
        let b = pick(&mut island, &[d.clone(), e.clone()], &prefs(), 8_000).unwrap();
        assert_eq!(b.event, Event::Low(20));
        assert_eq!(
            pick(&mut island, &[d.clone(), e.clone()], &prefs(), 9_000),
            None
        );
        e.battery = Some(10);
        e.alert = Some(10);
        e.alert_ms = Some(10_000);
        let b = pick(&mut island, &[d, e], &prefs(), 10_000).unwrap();
        assert_eq!(b.event, Event::Low(10));
    }

    #[test]
    fn old_events_and_switched_off_kinds_stay_quiet() {
        let mut island = Island::default();
        assert_eq!(pick(&mut island, &[], &prefs(), 0), None);
        let mut d = device("bt:1", 80, false);
        d.connected_ms = Some(1_000);
        // Older than the window: a restart does not replay it.
        assert_eq!(
            pick(&mut island, &[d.clone()], &prefs(), 1_000 + FRESH_MS),
            None
        );
        let mut off = prefs();
        off.device_banners = false;
        d.id = "bt:2".into();
        d.connected_ms = Some(30_000);
        assert_eq!(pick(&mut island, &[d.clone()], &off, 31_000), None);
        // Low battery has its own switch.
        d.id = "bt:3".into();
        d.connected_ms = None;
        d.alert = Some(20);
        d.alert_ms = Some(40_000);
        let mut quiet = prefs();
        quiet.low_battery = false;
        assert_eq!(pick(&mut island, &[d.clone()], &quiet, 41_000), None);
        d.alert_ms = Some(42_000);
        let mut system = prefs();
        system.system = false;
        assert_eq!(pick(&mut island, &[d], &system, 42_000), None);
    }

    #[test]
    fn low_battery_wins_over_a_device_arriving() {
        let mut island = Island::default();
        pick(&mut island, &[], &prefs(), 0);
        let mut a = device("bt:a", 90, false);
        a.connected_ms = Some(1_000);
        let mut b = device("bt:b", 9, false);
        b.alert = Some(10);
        b.alert_ms = Some(1_000);
        let banner = pick(&mut island, &[a, b], &prefs(), 1_500).unwrap();
        assert_eq!(
            (banner.device.id.as_str(), banner.event),
            ("bt:b", Event::Low(10))
        );
    }

    #[test]
    fn rings_turn_amber_then_urgent_while_discharging() {
        let p = Palette::dark();
        let accent = Color::WHITE;
        assert_eq!(ring_tone(&device("x", 50, false), p, accent), accent);
        assert_eq!(
            ring_tone(&device("x", 20, false), p, accent),
            p.readable(accent::TIMER)
        );
        assert_eq!(ring_tone(&device("x", 8, false), p, accent), p.warning());
        assert_eq!(ring_tone(&device("x", 8, true), p, accent), accent);
    }

    fn live() -> Applet {
        let (mut app, _) = Applet::init(Core::default(), ());
        app.commands = Some(tokio::sync::mpsc::channel(4).0);
        app.active = true;
        app.host_reveals = true;
        app.format_24h = false;
        app.snapshot.focus.key = "ready".into();
        app.snapshot.quiet = Some(false);
        app
    }
    fn feed(app: &mut Applet, devices: &[Device], tune: impl Fn(&mut bridge::Snapshot)) {
        let mut snapshot = bridge::Snapshot {
            quiet: Some(false),
            devices: devices.to_vec(),
            ..Default::default()
        };
        snapshot.focus.key = "ready".into();
        tune(&mut snapshot);
        let _ = app.update(Message::Backend(bridge::ClientEvent::Snapshot(snapshot)));
    }
    fn gone(app: &mut Applet) {
        if let Some(id) = app.popup {
            let _ = app.update(Message::PopupGone(id));
        }
        // Past the cooldown between routine reveals.
        app.last_reveal = None;
    }
    fn arrived(id: &str, name: &str, kind: DeviceKind, battery: u8) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            kind,
            battery: Some(battery),
            charging: Some(false),
            connected_ms: Some(chrono::Utc::now().timestamp_millis()),
            ..Default::default()
        }
    }
    fn bluetooth(
        title: &str,
        cue: &str,
        announce: bool,
        expires_ms: Option<i64>,
    ) -> system::Activity {
        system::Activity {
            source: system::Source::Bluetooth,
            title: title.into(),
            detail: "Connected".into(),
            cue: cue.into(),
            progress: None,
            urgent: false,
            announce,
            expires_ms,
        }
    }
    fn now() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[test]
    fn a_device_connecting_shows_a_banner_once() {
        let mut app = live();
        let mouse = device("bt:mouse", 60, false);
        feed(&mut app, &[mouse.clone()], |_| {});
        assert!(
            app.popup.is_none(),
            "what is there at the start stays quiet"
        );
        let phones = arrived("bt:phones", "WH-1000XM5", DeviceKind::Headphones, 80);
        feed(&mut app, &[mouse.clone(), phones.clone()], |_| {});
        assert!(app.popup.is_some() && app.revealed);
        assert_eq!(app.kind, Kind::Devices);
        assert!(app.reveal_deadline.is_some(), "a banner retracts by itself");
        let content = app.content();
        assert_eq!(content.title, "WH-1000XM5");
        assert_eq!(content.meta, "Connected · 80%");
        // The same snapshot again changes nothing; closing ends the banner.
        feed(&mut app, &[mouse.clone(), phones.clone()], |_| {});
        assert_eq!(app.kind, Kind::Devices);
        gone(&mut app);
        assert!(app.devices.banner.is_none() && app.kind != Kind::Devices);
        feed(&mut app, &[mouse, phones], |_| {});
        assert!(app.popup.is_none(), "never announced twice");
    }

    #[test]
    fn quiet_do_not_disturb_and_the_setting_keep_banners_away() {
        let gates: [(&str, fn(&mut bridge::Snapshot)); 5] = [
            ("fullscreen", |s| s.quiet = Some(true)),
            ("dnd", |s| s.dnd = true),
            ("banners off", |s| s.preferences.device_banners = false),
            ("system off", |s| s.preferences.system = false),
            ("reveals off", |s| s.preferences.reveals = false),
        ];
        for (name, gate) in gates {
            let mut app = live();
            feed(&mut app, &[], |_| {});
            let phones = arrived("bt:phones", "Buds", DeviceKind::Headphones, 70);
            feed(&mut app, &[phones.clone()], gate);
            assert!(app.popup.is_none(), "{name}");
            assert!(app.devices.banner.is_none(), "{name}: dropped, not queued");
            // Back to normal, the moment has passed: no replay.
            feed(&mut app, &[phones], |_| {});
            assert!(app.popup.is_none(), "{name}: not replayed");
        }
        // Low battery has its own switch.
        let mut app = live();
        feed(&mut app, &[], |_| {});
        let mut mouse = device("bt:mouse", 18, false);
        mouse.alert = Some(20);
        mouse.alert_ms = Some(now());
        feed(&mut app, &[mouse], |s| s.preferences.low_battery = false);
        assert!(app.popup.is_none());
    }

    #[test]
    fn low_battery_reveals_past_the_cooldown_and_ten_percent_is_urgent() {
        let mut app = live();
        let mut mouse = device("bt:mouse", 30, false);
        mouse.name = "MX Anywhere 3".into();
        mouse.kind = DeviceKind::Mouse;
        feed(&mut app, &[mouse.clone()], |_| {});
        // Something was just revealed: a routine reveal would wait.
        app.last_reveal = Some(Instant::now());
        mouse.battery = Some(20);
        mouse.alert = Some(20);
        mouse.alert_ms = Some(now());
        feed(&mut app, &[mouse.clone()], |_| {});
        assert!(app.popup.is_some(), "20% shows despite the cooldown");
        assert_eq!(app.content().title, "MX Anywhere 3");
        assert_eq!(app.content().meta, "Battery low · 20%");
        assert!(!app.devices_urgent());
        let p = app.palette();
        assert_eq!(app.devices_accent(p), Some(p.readable(accent::TIMER)));
        gone(&mut app);
        mouse.battery = Some(10);
        mouse.alert = Some(10);
        mouse.alert_ms = Some(now() + 1);
        feed(&mut app, &[mouse.clone()], |_| {});
        assert!(app.popup.is_some());
        assert!(app.devices_urgent());
        assert_eq!(app.devices_accent(p), Some(p.warning()));
        gone(&mut app);
        // Charging back above 20% and down again is a new discharge, but
        // the same alert stamp is never shown twice.
        feed(&mut app, &[mouse], |_| {});
        assert!(app.popup.is_none());
    }

    #[test]
    fn a_bluetooth_announcement_turns_into_the_banner() {
        let mut app = live();
        feed(&mut app, &[], |_| {});
        // The Bluetooth source announced the arrival first.
        let announced = bluetooth("Buds", "", true, Some(now() + 4_000));
        feed(&mut app, &[], |s| s.system = vec![announced.clone()]);
        assert_eq!(app.kind, Kind::System(system::Source::Bluetooth));
        assert!(app.popup.is_some() && app.revealed);
        let buds = arrived("bt:buds", "Buds", DeviceKind::Headphones, 70);
        feed(&mut app, &[buds], |s| s.system = vec![announced.clone()]);
        assert_eq!(app.kind, Kind::Devices, "one banner, not two");
        assert_eq!(app.content().title, "Buds");
        assert_eq!(app.content().meta, "Connected · 70%");
    }

    #[test]
    fn all_devices_opens_the_list_from_bluetooth_and_idle() {
        use cosmic::iced::Size;
        let mut app = live();
        app.snapshot.devices = vec![
            device("bt:1", 80, true),
            device("bt:2", 15, false),
            device("kdeconnect:3", 64, false),
        ];
        app.snapshot.system = vec![bluetooth("Buds", "", false, None)];
        app.kind = Kind::System(system::Source::Bluetooth);
        app.popup = Some(window::Id::unique());
        app.held = true;
        app.motion.retarget(1., false, Instant::now());
        let size = Size::new(app.frame().popup_width(), app.popup_height());
        let mut ui = crate::native_test::Native::new(size);
        assert!(ui.bounds(app.capsule(true), "activity-devices-all").width > 0.);
        assert!(
            ui.bounds(app.capsule(true), "activity-system-settings")
                .width
                > 0.
        );
        let _ = app.update(Message::Devices(Msg::Open));
        assert!(app.devices.card && app.kind == Kind::Devices);
        assert!(app.live_panel());
        assert_eq!(app.live_heading(1.).as_deref(), Some("Devices"));
        assert!(app.live_extra() > 0., "three rows need more room");
        assert!(app.eligible_kinds().contains(&Kind::Devices));
        // The taller list reopens the popup and keeps showing.
        let grown = app.promote_popup.expect("the popup grows for the list");
        let _ = app.update(Message::PopupGone(grown));
        assert!(app.devices.card && app.kind == Kind::Devices);
        // Closing forgets the list.
        let id = app.popup.unwrap_or_else(window::Id::unique);
        app.popup = Some(id);
        let _ = app.update(Message::PopupGone(id));
        assert!(!app.devices.card);
        assert!(!app.eligible_kinds().contains(&Kind::Devices));
        // The Idle card offers the list only while something is connected.
        app.kind = Kind::Idle;
        app.popup = Some(window::Id::unique());
        app.motion.retarget(1., false, Instant::now());
        assert!(
            ui.try_bounds(app.capsule(true), "activity-devices")
                .is_some()
        );
        app.snapshot.devices.clear();
        assert!(
            ui.try_bounds(app.capsule(true), "activity-devices")
                .is_none()
        );
    }

    /// Banners and the list in each style, floating and on a Standard bar.
    #[test]
    fn device_states_render_for_review() {
        use super::super::tests::save_scene;
        use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
        use cosmic::iced::Size;
        use preferences::IslandStyle;
        let Some(root) =
            std::env::var_os("HALCYON_ACTIVITY_EVIDENCE").map(std::path::PathBuf::from)
        else {
            return;
        };
        let root = root.join("island-devices");
        let scale = 2f32;
        let three = || {
            let named = |id: &str, name: &str, kind, battery, charging| Device {
                id: id.into(),
                name: name.into(),
                kind,
                battery: Some(battery),
                charging: Some(charging),
                ..Default::default()
            };
            vec![
                named(
                    "bt:AA:01",
                    "Sony WH-1000XM5",
                    DeviceKind::Headphones,
                    80,
                    true,
                ),
                named("bt:AA:02", "MX Anywhere 3", DeviceKind::Mouse, 15, false),
                named("kdeconnect:pixel", "Pixel 8", DeviceKind::Phone, 64, false),
            ]
        };
        type Setup = fn(&mut Applet, Vec<Device>);
        let states: &[(&str, &[&str], Setup)] = &[
            ("connected", &["activity-devices-all"], |app, list| {
                let mut d = list[0].clone();
                d.charging = Some(false);
                app.snapshot.devices = vec![d.clone()];
                app.devices.banner = Some(Banner {
                    device: d,
                    event: Event::Connected,
                });
                app.kind = Kind::Devices;
            }),
            ("low-20", &["activity-devices-all"], |app, list| {
                let d = list[1].clone();
                app.snapshot.devices = list;
                app.devices.banner = Some(Banner {
                    device: d,
                    event: Event::Low(20),
                });
                app.kind = Kind::Devices;
            }),
            ("low-10", &["activity-devices-all"], |app, mut list| {
                list[1].battery = Some(9);
                let d = list[1].clone();
                app.snapshot.devices = list;
                app.devices.banner = Some(Banner {
                    device: d,
                    event: Event::Low(10),
                });
                app.kind = Kind::Devices;
            }),
            (
                "card",
                &[
                    "activity-device-bt:AA:01",
                    "activity-device-kdeconnect:pixel",
                ],
                |app, list| {
                    app.snapshot.devices = list;
                    app.devices.card = true;
                    app.kind = Kind::Devices;
                },
            ),
            ("card-empty", &[], |app, _| {
                app.devices.card = true;
                app.kind = Kind::Devices;
            }),
            (
                "bluetooth",
                &["activity-devices-all", "activity-system-settings"],
                |app, list| {
                    app.snapshot.devices = list;
                    app.snapshot.system = vec![bluetooth("Sony WH-1000XM5", "80%", false, None)];
                    app.kind = Kind::System(system::Source::Bluetooth);
                },
            ),
            (
                "idle",
                &["activity-devices", "activity-preferences"],
                |app, list| {
                    app.snapshot.devices = list;
                    app.kind = Kind::Idle;
                },
            ),
        ];
        let styles = [
            (
                "theme-light",
                IslandStyle::Theme,
                cosmic::Theme::light(),
                CosmicPanelBackground::Light,
                ([214., 196., 170.], [168., 186., 204.]),
            ),
            (
                "theme-dark",
                IslandStyle::Theme,
                cosmic::Theme::dark(),
                CosmicPanelBackground::Dark,
                ([46., 58., 74.], [96., 84., 110.]),
            ),
            (
                "dark",
                IslandStyle::Dark,
                cosmic::Theme::light(),
                CosmicPanelBackground::Light,
                ([214., 196., 170.], [168., 186., 204.]),
            ),
        ];
        for attached in [false, true] {
            for (style_name, style, theme, background, wall) in &styles {
                for (name, ids, setup) in states {
                    let mut app = live();
                    app.now_ms = now();
                    app.snapshot.preferences.style = *style;
                    app.core.applet.background = background.clone();
                    if attached {
                        app.supported = true;
                        app.panel_frame = host::Frame {
                            height: 32.,
                            padding: 4.,
                            radius: 8.,
                            attached: true,
                            opacity: 0.85,
                        };
                    }
                    setup(&mut app, three());
                    app.popup = Some(window::Id::unique());
                    app.held = true;
                    app.popup_extra = app.live_extra();
                    app.motion.retarget(1., false, Instant::now());
                    let frame = app.frame();
                    let (w, h) = (frame.popup_width(), app.popup_height());
                    let mut ui = crate::native_test::Native::new(Size::new(w, h));
                    let label = format!("{name} {style_name} attached={attached}");
                    assert!(ui.duplicate_ids(app.capsule(true)).is_empty(), "{label}");
                    for id in *ids {
                        let b = ui.bounds(app.capsule(true), id);
                        assert!(
                            b.width > 0.
                                && b.x >= 0.
                                && b.y >= 0.
                                && b.x + b.width <= w + 0.5
                                && b.y + b.height <= h + 0.5,
                            "{label}: {id} {b:?} outside {w}x{h}"
                        );
                    }
                    let pixels = ui.render_scaled(app.capsule(true), theme, scale as f64);
                    let panel = Palette::new(&app.core.applet, IslandStyle::Theme).surface;
                    save_scene(
                        &root.join(format!(
                            "{}{name}-{style_name}.png",
                            if attached { "standard-" } else { "" }
                        )),
                        &pixels,
                        ((w * scale) as usize, (h * scale) as usize),
                        &frame,
                        panel,
                        *wall,
                        scale,
                    );
                }
            }
        }
    }
}
