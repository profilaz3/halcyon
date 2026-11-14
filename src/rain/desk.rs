//! What hides the desktop on each output, so the wallpaper effect there can
//! hold still: a maximized or fullscreen window on the output's active
//! workspace, an idle session, or an output that is turned off. A separate
//! event-driven Wayland connection, followed only while an effect is
//! wanted; it sends a new `Desk` only when that changes.
//!
//! cosmic-comp keeps sending frame callbacks to a covered background
//! surface about once a second, and none at all to an output that is off,
//! so without this the effect keeps drawing whole frames nobody sees, and
//! the frame watchdog mistakes a dark output for a stalled one.
use cosmic::cctk::{
    cosmic_protocols::toplevel_info::v1::client::{
        zcosmic_toplevel_handle_v1 as handle, zcosmic_toplevel_info_v1 as info,
    },
    wayland_client::{
        self, Connection, Dispatch, Proxy, QueueHandle, WEnum,
        backend::ObjectId,
        protocol::{wl_output, wl_registry, wl_seat},
    },
    wayland_protocols::ext::{
        foreign_toplevel_list::v1::client::{
            ext_foreign_toplevel_handle_v1 as foreign, ext_foreign_toplevel_list_v1 as list,
        },
        idle_notify::v1::client::{
            ext_idle_notification_v1 as idle, ext_idle_notifier_v1 as notifier,
        },
        workspace::v1::client::{
            ext_workspace_group_handle_v1 as group, ext_workspace_handle_v1 as workspace,
            ext_workspace_manager_v1 as workspaces,
        },
    },
};
use cosmic::iced::{
    Subscription,
    futures::{SinkExt, Stream},
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::{os::fd::AsFd, time::Duration};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1 as power_manager, zwlr_output_power_v1 as power,
};

/// What hides the desktop now, by output name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Desk {
    /// Nobody has touched the keyboard or pointer for `idle_after`.
    pub idle: bool,
    pub off: BTreeSet<String>,
    pub covered: BTreeSet<String>,
}
impl Desk {
    /// Whether nothing of the desktop on `output` can be seen.
    pub fn hides(&self, output: &str) -> bool {
        self.idle || self.off.contains(output) || self.covered.contains(output)
    }
}

/// `zcosmic_toplevel_handle_v1.state` values.
const MAXIMIZED: u32 = 0;
const MINIMIZED: u32 = 1;
const FULLSCREEN: u32 = 3;
/// `ext_workspace_handle_v1.state`: the workspace is shown.
const ACTIVE: u32 = 1;

#[derive(Debug, Clone)]
struct Window<K> {
    outputs: HashSet<K>,
    workspaces: HashSet<K>,
    states: Vec<u32>,
}
impl<K> Default for Window<K> {
    fn default() -> Self {
        Self {
            outputs: HashSet::new(),
            workspaces: HashSet::new(),
            states: Vec::new(),
        }
    }
}
impl<K> Window<K> {
    fn covers(&self) -> bool {
        !self.states.contains(&MINIMIZED)
            && (self.states.contains(&MAXIMIZED) || self.states.contains(&FULLSCREEN))
    }
}

/// The desktop as the connection has heard it, keyed by protocol object.
#[derive(Debug)]
pub(crate) struct Model<K> {
    outputs: HashMap<K, String>,
    off: HashSet<K>,
    active: HashSet<K>,
    windows: HashMap<K, Window<K>>,
    idle: bool,
}
impl<K> Default for Model<K> {
    fn default() -> Self {
        Self {
            outputs: HashMap::new(),
            off: HashSet::new(),
            active: HashSet::new(),
            windows: HashMap::new(),
            idle: false,
        }
    }
}
impl<K: Hash + Eq + Clone> Model<K> {
    pub(crate) fn output(&mut self, output: K, name: String) {
        self.outputs.insert(output, name);
    }
    pub(crate) fn output_gone(&mut self, output: &K) {
        self.outputs.remove(output);
        self.off.remove(output);
        for window in self.windows.values_mut() {
            window.outputs.remove(output);
        }
    }
    pub(crate) fn power(&mut self, output: K, on: bool) {
        if on {
            self.off.remove(&output);
        } else {
            self.off.insert(output);
        }
    }
    pub(crate) fn workspace(&mut self, workspace: K, state: u32) {
        if state & ACTIVE != 0 {
            self.active.insert(workspace);
        } else {
            self.active.remove(&workspace);
        }
    }
    pub(crate) fn workspace_gone(&mut self, workspace: &K) {
        self.active.remove(workspace);
        for window in self.windows.values_mut() {
            window.workspaces.remove(workspace);
        }
    }
    pub(crate) fn window_state(&mut self, window: K, states: Vec<u32>) {
        self.windows.entry(window).or_default().states = states;
    }
    pub(crate) fn window_output(&mut self, window: K, output: K, enter: bool) {
        let outputs = &mut self.windows.entry(window).or_default().outputs;
        if enter {
            outputs.insert(output);
        } else {
            outputs.remove(&output);
        }
    }
    pub(crate) fn window_workspace(&mut self, window: K, workspace: K, enter: bool) {
        let workspaces = &mut self.windows.entry(window).or_default().workspaces;
        if enter {
            workspaces.insert(workspace);
        } else {
            workspaces.remove(&workspace);
        }
    }
    pub(crate) fn window_gone(&mut self, window: &K) {
        self.windows.remove(window);
    }
    pub(crate) fn idle(&mut self, idle: bool) {
        self.idle = idle;
    }
    pub(crate) fn desk(&self) -> Desk {
        let name = |output: &K| self.outputs.get(output).cloned();
        Desk {
            idle: self.idle,
            off: self.off.iter().filter_map(name).collect(),
            covered: self
                .windows
                .values()
                .filter(|w| {
                    w.covers()
                        && (w.workspaces.is_empty()
                            || w.workspaces.iter().any(|ws| self.active.contains(ws)))
                })
                .flat_map(|w| w.outputs.iter().filter_map(name))
                .collect(),
        }
    }
}

/// How long without input counts as idle: COSMIC's screen-off time when
/// one is set, otherwise ten minutes. Idle inhibitors (a playing video)
/// keep the session awake.
fn idle_after() -> Duration {
    const DEFAULT: Duration = Duration::from_secs(600);
    let key =
        crate::desktop::store::ConfigKey::new("com.system76.CosmicIdle", 1, "screen_off_time");
    let off = crate::desktop::DesktopStore::from_env()
        .ok()
        .zip(key.ok())
        .and_then(|(store, key)| store.read_user(&key).ok().flatten())
        .and_then(|value| value.decode::<Option<u32>>().ok())
        .flatten();
    off.map_or(DEFAULT, |ms| Duration::from_millis(u64::from(ms)))
        .max(Duration::from_secs(60))
}

#[derive(Default)]
struct State {
    model: Model<ObjectId>,
    seat: Option<wl_seat::WlSeat>,
    outputs: Vec<(u32, wl_output::WlOutput, Option<power::ZwlrOutputPowerV1>)>,
    notifier: Option<notifier::ExtIdleNotifierV1>,
    notification: Option<idle::ExtIdleNotificationV1>,
    power: Option<power_manager::ZwlrOutputPowerManagerV1>,
    info: Option<info::ZcosmicToplevelInfoV1>,
    list: Option<list::ExtForeignToplevelListV1>,
    toplevels: HashMap<
        ObjectId,
        (
            foreign::ExtForeignToplevelHandleV1,
            handle::ZcosmicToplevelHandleV1,
        ),
    >,
    pending: Vec<foreign::ExtForeignToplevelHandleV1>,
    workspaces: Option<workspaces::ExtWorkspaceManagerV1>,
    idle_after: Duration,
}
impl State {
    /// Make the per-object requests that need two globals, once both are
    /// bound, whatever order the registry named them in.
    fn wire(&mut self, q: &QueueHandle<Self>) {
        if let Some(manager) = &self.power {
            for (_, output, power) in &mut self.outputs {
                if power.is_none() {
                    *power = Some(manager.get_output_power(output, q, output.id()));
                }
            }
        }
        if self.notification.is_none()
            && let (Some(notifier), Some(seat)) = (&self.notifier, &self.seat)
        {
            let ms = u32::try_from(self.idle_after.as_millis()).unwrap_or(u32::MAX);
            self.notification = Some(notifier.get_idle_notification(ms, seat, q, ()));
        }
        if let Some(info) = &self.info {
            for toplevel in self.pending.drain(..) {
                let cosmic = info.get_cosmic_toplevel(&toplevel, q, Some(toplevel.id()));
                self.toplevels.insert(toplevel.id(), (toplevel, cosmic));
            }
        }
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        match e {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_output" if version >= 4 => {
                    let output = r.bind::<wl_output::WlOutput, _, _>(name, 4, q, ());
                    s.outputs.push((name, output, None));
                }
                "wl_seat" if s.seat.is_none() => {
                    s.seat = Some(r.bind(name, 1, q, ()));
                }
                "ext_idle_notifier_v1" => s.notifier = Some(r.bind(name, 1, q, ())),
                "zwlr_output_power_manager_v1" => s.power = Some(r.bind(name, 1, q, ())),
                // Version 2 hands out the cosmic handle for each toplevel of
                // the ext list; version 3 adds its ext workspaces.
                "zcosmic_toplevel_info_v1" if version >= 3 => {
                    s.info = Some(r.bind(name, 3, q, ()));
                }
                "ext_foreign_toplevel_list_v1" => s.list = Some(r.bind(name, 1, q, ())),
                "ext_workspace_manager_v1" => s.workspaces = Some(r.bind(name, 1, q, ())),
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(at) = s.outputs.iter().position(|(n, ..)| *n == name) {
                    let (_, output, power) = s.outputs.remove(at);
                    s.model.output_gone(&output.id());
                    if let Some(power) = power {
                        power.destroy();
                    }
                    output.release();
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        s: &mut Self,
        o: &wl_output::WlOutput,
        e: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = e {
            s.model.output(o.id(), name);
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<notifier::ExtIdleNotifierV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &notifier::ExtIdleNotifierV1,
        _: notifier::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<idle::ExtIdleNotificationV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &idle::ExtIdleNotificationV1,
        e: idle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            idle::Event::Idled => s.model.idle(true),
            idle::Event::Resumed => s.model.idle(false),
            _ => {}
        }
    }
}
impl Dispatch<power_manager::ZwlrOutputPowerManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &power_manager::ZwlrOutputPowerManagerV1,
        _: power_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<power::ZwlrOutputPowerV1, ObjectId> for State {
    fn event(
        s: &mut Self,
        _: &power::ZwlrOutputPowerV1,
        e: power::Event,
        output: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            power::Event::Mode { mode } => {
                s.model
                    .power(output.clone(), mode != WEnum::Value(power::Mode::Off));
            }
            // Another client controls it, or the output went away.
            power::Event::Failed => s.model.power(output.clone(), true),
            _ => {}
        }
    }
}
impl Dispatch<list::ExtForeignToplevelListV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &list::ExtForeignToplevelListV1,
        e: list::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let list::Event::Toplevel { toplevel } = e {
            s.pending.push(toplevel);
        }
    }
    wayland_client::event_created_child!(State, list::ExtForeignToplevelListV1, [
        list::EVT_TOPLEVEL_OPCODE => (foreign::ExtForeignToplevelHandleV1, ())
    ]);
}
impl Dispatch<foreign::ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        h: &foreign::ExtForeignToplevelHandleV1,
        e: foreign::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let foreign::Event::Closed = e {
            s.model.window_gone(&h.id());
            s.pending.retain(|pending| pending != h);
            if let Some((toplevel, cosmic)) = s.toplevels.remove(&h.id()) {
                cosmic.destroy();
                toplevel.destroy();
            } else {
                h.destroy();
            }
        }
    }
}
impl Dispatch<info::ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &info::ZcosmicToplevelInfoV1,
        _: info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    // Only version 1 announces toplevels this way; this binds version 3.
    wayland_client::event_created_child!(State, info::ZcosmicToplevelInfoV1, [
        info::EVT_TOPLEVEL_OPCODE => (handle::ZcosmicToplevelHandleV1, None)
    ]);
}
impl Dispatch<handle::ZcosmicToplevelHandleV1, Option<ObjectId>> for State {
    fn event(
        s: &mut Self,
        _: &handle::ZcosmicToplevelHandleV1,
        e: handle::Event,
        window: &Option<ObjectId>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(window) = window.clone() else {
            return;
        };
        match e {
            handle::Event::State { state } => {
                let states = state
                    .chunks_exact(4)
                    .map(|c| u32::from_ne_bytes(c.try_into().expect("four bytes")))
                    .collect();
                s.model.window_state(window, states);
            }
            handle::Event::OutputEnter { output } => {
                s.model.window_output(window, output.id(), true)
            }
            handle::Event::OutputLeave { output } => {
                s.model.window_output(window, output.id(), false);
            }
            handle::Event::ExtWorkspaceEnter { workspace } => {
                s.model.window_workspace(window, workspace.id(), true);
            }
            handle::Event::ExtWorkspaceLeave { workspace } => {
                s.model.window_workspace(window, workspace.id(), false);
            }
            _ => {}
        }
    }
}
impl Dispatch<workspaces::ExtWorkspaceManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &workspaces::ExtWorkspaceManagerV1,
        _: workspaces::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    wayland_client::event_created_child!(State, workspaces::ExtWorkspaceManagerV1, [
        workspaces::EVT_WORKSPACE_GROUP_OPCODE => (group::ExtWorkspaceGroupHandleV1, ()),
        workspaces::EVT_WORKSPACE_OPCODE => (workspace::ExtWorkspaceHandleV1, ())
    ]);
}
impl Dispatch<group::ExtWorkspaceGroupHandleV1, ()> for State {
    fn event(
        _: &mut Self,
        g: &group::ExtWorkspaceGroupHandleV1,
        e: group::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let group::Event::Removed = e {
            g.destroy();
        }
    }
}
impl Dispatch<workspace::ExtWorkspaceHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        w: &workspace::ExtWorkspaceHandleV1,
        e: workspace::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            workspace::Event::State { state } => {
                let bits = match state {
                    WEnum::Value(state) => state.bits(),
                    WEnum::Unknown(bits) => bits,
                };
                s.model.workspace(w.id(), bits);
            }
            workspace::Event::Removed => {
                s.model.workspace_gone(&w.id());
                w.destroy();
            }
            _ => {}
        }
    }
}

async fn run(out: &mut cosmic::iced::futures::channel::mpsc::Sender<Desk>) -> Option<()> {
    let display = std::env::var_os("WAYLAND_DISPLAY")?;
    let path = std::path::PathBuf::from(display);
    let path = if path.is_absolute() {
        path
    } else {
        std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(path)
    };
    let socket = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::UnixStream::connect(path),
    )
    .await
    .ok()?
    .ok()?
    .into_std()
    .ok()?;
    let c = Connection::from_socket(socket).ok()?;
    let fd = tokio::io::unix::AsyncFd::new(c.as_fd().try_clone_to_owned().ok()?).ok()?;
    let mut queue = c.new_event_queue::<State>();
    let q = queue.handle();
    let _registry = c.display().get_registry(&q, ());
    let mut s = State {
        idle_after: idle_after(),
        ..Default::default()
    };
    let mut last = None;
    loop {
        queue.dispatch_pending(&mut s).ok()?;
        s.wire(&q);
        c.flush().ok()?;
        if s.toplevels.len() > 1024 {
            return None;
        }
        let desk = s.model.desk();
        if last.as_ref() != Some(&desk) {
            out.send(desk.clone()).await.ok()?;
            last = Some(desk);
        }
        let mut ready = fd.readable().await.ok()?;
        // The guard is thread-bound; never hold it across an await. This
        // connection has one reader, and dispatch_pending handles queued events.
        let Some(reader) = queue.prepare_read() else {
            continue;
        };
        match reader.read() {
            Ok(_) => {}
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return None,
        }
        ready.clear_ready();
    }
}

/// The desktop's `Desk`, sent again only when it changes. Without a
/// connection nothing counts as hidden, so the effect plays.
pub fn watch() -> Subscription<Desk> {
    Subscription::run(feed)
}
fn feed() -> impl Stream<Item = Desk> {
    cosmic::iced::stream::channel(1, async move |mut out| {
        loop {
            let _ = run(&mut out).await;
            if out.send(Desk::default()).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rain_desk_hides_outputs_under_covering_windows_on_active_workspaces() {
        let mut m = Model::<u32>::default();
        m.output(1, "DP-1".into());
        m.output(2, "HDMI-A-1".into());
        m.workspace(10, ACTIVE);
        m.workspace(11, 0);
        assert_eq!(m.desk(), Desk::default());
        // A plain window covers nothing.
        m.window_output(100, 1, true);
        m.window_workspace(100, 10, true);
        m.window_state(100, vec![2]);
        assert!(!m.desk().hides("DP-1"));
        // Maximized on the shown workspace: DP-1 is covered, HDMI is not.
        m.window_state(100, vec![MAXIMIZED, 2]);
        assert!(m.desk().hides("DP-1") && !m.desk().hides("HDMI-A-1"));
        // Minimized, or on a workspace that is not shown: nothing covered.
        m.window_state(100, vec![MAXIMIZED, MINIMIZED]);
        assert!(!m.desk().hides("DP-1"));
        m.window_state(100, vec![MAXIMIZED]);
        m.window_workspace(100, 10, false);
        m.window_workspace(100, 11, true);
        assert!(!m.desk().hides("DP-1"), "maximized on a hidden workspace");
        m.workspace(11, ACTIVE);
        assert!(m.desk().hides("DP-1"), "its workspace is shown again");
        // Fullscreen on the other output, then closed.
        m.window_output(200, 2, true);
        m.window_state(200, vec![FULLSCREEN]);
        assert!(m.desk().hides("HDMI-A-1"), "no workspace known: shown");
        m.window_gone(&200);
        assert!(!m.desk().hides("HDMI-A-1"));
        // Moved to another output.
        m.window_output(100, 1, false);
        m.window_output(100, 2, true);
        assert!(!m.desk().hides("DP-1") && m.desk().hides("HDMI-A-1"));
        m.workspace_gone(&11);
        assert!(m.desk().hides("HDMI-A-1"), "no workspace left: shown");
    }
    #[test]
    fn rain_desk_hides_outputs_that_are_off_and_everything_while_idle() {
        let mut m = Model::<u32>::default();
        m.output(1, "DP-1".into());
        m.output(2, "HDMI-A-1".into());
        m.power(1, false);
        let desk = m.desk();
        assert!(desk.hides("DP-1") && !desk.hides("HDMI-A-1"));
        m.power(1, true);
        assert_eq!(m.desk(), Desk::default());
        m.idle(true);
        assert!(m.desk().hides("DP-1") && m.desk().hides("HDMI-A-1"));
        m.idle(false);
        m.power(2, false);
        m.output_gone(&2);
        assert_eq!(m.desk(), Desk::default(), "a removed output is forgotten");
    }
    /// Watches the running desktop for a few seconds, read-only: it binds
    /// the globals and listens, and never sets a mode. Only with
    /// `HALCYON_DESK_LIVE` set to a file for what it saw.
    #[test]
    fn rain_desk_live_smoke() {
        let Some(out) = std::env::var_os("HALCYON_DESK_LIVE") else {
            return;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut seen = runtime.block_on(async {
            use cosmic::iced::futures::StreamExt;
            let stream = feed();
            tokio::pin!(stream);
            let mut seen = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(4), async {
                while let Some(desk) = stream.next().await {
                    seen.push(format!("{desk:?}"));
                }
            })
            .await;
            seen
        });
        // The same objects, read in round trips, to show what it heard.
        let c = Connection::connect_to_env().unwrap();
        let mut queue = c.new_event_queue::<State>();
        let q = queue.handle();
        let _registry = c.display().get_registry(&q, ());
        let mut s = State {
            idle_after: idle_after(),
            ..Default::default()
        };
        for _ in 0..10 {
            queue.roundtrip(&mut s).unwrap();
            s.wire(&q);
            std::thread::sleep(Duration::from_millis(200));
        }
        let windows: Vec<_> = s
            .model
            .windows
            .values()
            .map(|w| (w.states.clone(), w.outputs.len(), w.workspaces.len()))
            .collect();
        seen.push(format!(
            "outputs {:?} off {} active workspaces {} power {} idle {} after {:?} info {:?} list {} workspaces {} toplevels {} pending {} windows {windows:?} desk {:?}",
            s.model.outputs.values().collect::<Vec<_>>(),
            s.model.off.len(),
            s.model.active.len(),
            s.power.is_some(),
            s.notification.is_some(),
            s.idle_after,
            s.info.as_ref().map(|i| i.version()),
            s.list.is_some(),
            s.workspaces.is_some(),
            s.toplevels.len(),
            s.pending.len(),
            s.model.desk()
        ));
        std::fs::write(out, seen.join("\n")).unwrap();
    }
}
