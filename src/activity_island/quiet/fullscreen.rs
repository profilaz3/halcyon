//! A separate event-driven Wayland connection; store only fullscreen bits.
use super::Part;
use cosmic::cctk::{
    cosmic_protocols::toplevel_info::v1::client::{
        zcosmic_toplevel_handle_v1 as handle, zcosmic_toplevel_info_v1 as info,
    },
    wayland_client::{
        self, Connection, Dispatch, Proxy, QueueHandle,
        protocol::{wl_callback, wl_registry},
    },
};
use std::{collections::HashMap, os::fd::AsFd, time::Duration};
#[derive(Default)]
struct State {
    manager: Option<info::ZcosmicToplevelInfoV1>,
    global: Option<u32>,
    ready: bool,
    windows: HashMap<wayland_client::backend::ObjectId, (bool, bool)>,
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        c: &Connection,
        q: &QueueHandle<Self>,
    ) {
        match e {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "zcosmic_toplevel_info_v1" && version >= 1 && s.manager.is_none() => {
                // The v1 contract includes state and close notifications without
                // collecting titles or depending on optional workspace protocols.
                s.manager = Some(r.bind::<info::ZcosmicToplevelInfoV1, _, _>(name, 1, q, ()));
                s.global = Some(name);
                c.display().sync(q, ());
            }
            wl_registry::Event::GlobalRemove { name } if s.global == Some(name) => {
                s.ready = false;
                s.manager = None;
                s.windows.clear();
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        s: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        s.ready = s.manager.is_some();
    }
}
impl Dispatch<info::ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &info::ZcosmicToplevelInfoV1,
        e: info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            info::Event::Toplevel { toplevel } => {
                s.windows.insert(toplevel.id(), (false, false));
            }
            info::Event::Finished => {
                s.ready = false;
                s.manager = None;
            }
            _ => {}
        }
    }
    wayland_client::event_created_child!(State,info::ZcosmicToplevelInfoV1,[0=>(handle::ZcosmicToplevelHandleV1,())]);
}
impl Dispatch<handle::ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        h: &handle::ZcosmicToplevelHandleV1,
        e: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            handle::Event::State { state } => {
                let full = state
                    .chunks_exact(4)
                    .any(|c| u32::from_ne_bytes(c.try_into().expect("four bytes")) == 3);
                s.windows.entry(h.id()).or_default().0 = full;
            }
            handle::Event::Done => {
                if let Some((pending, current)) = s.windows.get_mut(&h.id()) {
                    *current = *pending;
                }
            }
            handle::Event::Closed => {
                s.windows.remove(&h.id());
                h.destroy();
            }
            _ => {}
        }
    }
}
async fn run(tx: &tokio::sync::mpsc::Sender<Part>) -> Option<()> {
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
    let mut s = State::default();
    let mut last = None;
    loop {
        queue.dispatch_pending(&mut s).ok()?;
        c.flush().ok()?;
        if s.windows.len() > 512 {
            return None;
        }
        let quiet = !s.ready || s.windows.values().any(|(_, full)| *full);
        if last != Some(quiet) {
            tx.send(Part::Fullscreen(quiet)).await.ok()?;
            last = Some(quiet);
        }
        let mut ready = tokio::select! {_=tx.closed()=>return None,r=fd.readable()=>r.ok()?};
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
pub(super) async fn observe(tx: tokio::sync::mpsc::Sender<Part>) {
    loop {
        let _ = run(&tx).await;
        if tx.send(Part::Fullscreen(true)).await.is_err() {
            return;
        }
        tokio::select! {_=tx.closed()=>return,_=tokio::time::sleep(Duration::from_secs(10))=>{}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires an explicitly isolated COSMIC compositor"]
    async fn reactive_private_fullscreen_connection_initializes_and_cancels() {
        assert!(crate::tools::preview());
        assert_eq!(
            std::env::var("HALCYON_ACTIVITY_PRIVATE_COMPOSITOR").as_deref(),
            Ok("1")
        );
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let task = tokio::spawn(observe(tx));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !matches!(rx.recv().await, Some(Part::Fullscreen(false))) {}
        })
        .await
        .unwrap();
        drop(rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
}
