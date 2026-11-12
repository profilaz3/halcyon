//! Read-only, bounded observation of the panel that owns this applet.
//! Islands mode is verified with the companion; every other top Panel,
//! including stock COSMIC, uses the attached-card shape.
use cosmic::iced::{Subscription, futures::SinkExt, stream};
use dock_islands_core::{MAX_WIRE_BYTES, Request, Response};
use notify::{EventKind, RecursiveMode, Watcher};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Applet extent and the panel's own outer padding/corners. These are read once
/// per host observation, never in the animation or view path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub height: f32,
    pub padding: f32,
    pub radius: f32,
    /// A Standard bar: the title stays in the bar and the details hang
    /// beneath it as an attached card, instead of the island growing.
    pub attached: bool,
    /// The bar's own opacity, so an attached card reads as part of it.
    pub opacity: f32,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            height: super::COMPACT_HEIGHT,
            padding: 0.,
            radius: 16.,
            attached: false,
            opacity: 1.,
        }
    }
}
impl Frame {
    pub fn inset(self) -> f32 {
        (self.height - super::COMPACT_HEIGHT).max(0.) / 2.
    }
    /// The bar's lower edge in popup coordinates, where an attached card starts.
    pub fn bar(self) -> f32 {
        self.height + 2. * self.padding
    }
    pub fn popup_width(self) -> f32 {
        if self.attached {
            super::surface::ATTACHED_WIDTH + 2. * super::surface::SHOULDER
        } else {
            super::WIDTH + 2. * self.padding
        }
    }
    pub fn popup_height(self) -> f32 {
        if self.attached {
            self.bar() + super::surface::CARD_ROOM
        } else {
            (super::HEIGHT + self.inset() + self.padding).max(self.bar())
        }
    }
    fn load(height: u32, attached: bool) -> Self {
        use crate::desktop::store::{ConfigKey, DesktopStore};
        use cosmic::applet::cosmic_panel_config::CosmicPanelConfig;
        let defaults = CosmicPanelConfig::default();
        let keys = ["padding", "border_radius", "autohide", "opacity"].map(|key| {
            ConfigKey::new("com.system76.CosmicPanel.Panel", 1, key).expect("fixed panel key")
        });
        let snapshot = DesktopStore::from_env()
            .ok()
            .and_then(|s| s.load(&keys).ok());
        let value = |i, default| {
            snapshot
                .as_ref()
                .and_then(|s| s.effective(&keys[i]))
                .and_then(|v| v.decode::<u32>().ok())
                .unwrap_or(default)
        };
        Self {
            height: (height as f32).clamp(super::COMPACT_HEIGHT, 512.),
            padding: value(0, defaults.padding).min(crate::desktop::panels::MAX_PADDING) as f32,
            radius: value(1, defaults.border_radius).min(crate::desktop::panels::MAX_RADIUS) as f32,
            attached,
            opacity: snapshot
                .as_ref()
                .and_then(|s| s.effective(&keys[3]))
                .and_then(|v| v.decode::<f32>().ok())
                .filter(|o| o.is_finite())
                .unwrap_or(defaults.opacity)
                .clamp(0., 1.),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observation {
    pub reveals_allowed: bool,
    pub enabled: bool,
    pub frame: Frame,
}

#[derive(Clone, Debug, Hash)]
struct Host {
    socket: Option<PathBuf>,
    config: Option<PathBuf>,
    panel: bool,
    output: String,
    pid: u32,
    height: u32,
}
impl Host {
    fn context(context: &cosmic::applet::Context) -> Self {
        Self {
            socket: std::env::var_os("XDG_RUNTIME_DIR")
                .map(|p| PathBuf::from(p).join("desktop-cards-islands-v1.sock")),
            config: dirs::config_dir().map(|p| {
                p.join("cosmic")
                    .join(dock_islands_core::NAMESPACE)
                    .join("v1/Panel")
            }),
            panel: matches!(context.panel_type, cosmic::applet::PanelType::Panel),
            output: context.output_name.clone(),
            pid: unsafe { libc::getppid() } as u32,
            height: context.suggested_window_size().1.get(),
        }
    }
    async fn enabled(&self) -> bool {
        if !self.panel || self.output.is_empty() {
            return false;
        }
        tokio::time::timeout(Duration::from_millis(300), async {
            let socket = self.socket.as_ref()?;
            let mut stream = tokio::net::UnixStream::connect(socket).await.ok()?;
            let peer = stream.peer_cred().ok()?;
            if peer.uid() != unsafe { libc::geteuid() }
                || peer.pid().map(|p| p as u32) != Some(self.pid)
            {
                return None;
            }
            let mut request = serde_json::to_vec(&Request::Inspect {
                panel: "Panel".into(),
                output: self.output.clone(),
            })
            .ok()?;
            request.push(b'\n');
            stream.write_all(&request).await.ok()?;
            let mut line = Vec::new();
            BufReader::new(stream)
                .take((MAX_WIRE_BYTES + 1) as u64)
                .read_until(b'\n', &mut line)
                .await
                .ok()?;
            if line.len() > MAX_WIRE_BYTES || !line.ends_with(b"\n") {
                return None;
            }
            match serde_json::from_slice::<Response>(&line).ok()? {
                Response::Draft { settings, view, .. } => Some(
                    settings.validate().is_ok() && settings.enabled && view.output == self.output,
                ),
                _ => None,
            }
        })
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
    }
}

impl Host {
    /// Islands grow the island in place. Any other top Panel, including stock
    /// COSMIC without the companion, gets the attached card.
    async fn observe(&self, islands: bool) -> Observation {
        let height = self.height;
        let frame = tokio::task::spawn_blocking(move || Frame::load(height, !islands))
            .await
            .unwrap_or(Frame {
                attached: !islands,
                ..Frame::default()
            });
        Observation {
            enabled: true,
            reveals_allowed: reveals_allowed(),
            frame,
        }
    }
}

pub(super) fn subscription(context: &cosmic::applet::Context) -> Subscription<Observation> {
    Subscription::run_with(Host::context(context), |host| observe(host.clone()))
}
fn observe(host: Host) -> impl cosmic::iced::futures::Stream<Item = Observation> {
    stream::channel(1, async move |mut out| {
        if !host.panel {
            let _ = out
                .send(Observation {
                    reveals_allowed: false,
                    enabled: false,
                    frame: Frame::default(),
                })
                .await;
            std::future::pending::<()>().await;
        }
        let mut paths = host
            .socket
            .iter()
            .chain(host.config.iter())
            .cloned()
            .collect::<Vec<_>>();
        if let Some(config) = dirs::config_dir() {
            paths.extend(["padding", "border_radius", "autohide", "opacity"].map(|key| {
                config
                    .join("cosmic/com.system76.CosmicPanel.Panel/v1")
                    .join(key)
            }));
        }
        let watched = paths.clone();
        let (send, mut receive) = tokio::sync::mpsc::channel(1);
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let relevant = match event {
                    Ok(e) => {
                        matches!(
                            e.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && e
                            .paths
                            .iter()
                            .any(|changed| watched.iter().any(|p| p.starts_with(changed)))
                    }
                    Err(_) => true,
                };
                if relevant {
                    let _ = send.try_send(());
                }
            })
            .ok();
        let mut registered = std::collections::BTreeSet::new();
        let mut last = None;
        let mut startup_retries = 8;
        loop {
            // Watch parents as well as namespaces so first creation and atomic
            // replacements work without creating any desktop-owned files.
            if let Some(w) = &mut watcher {
                for path in &paths {
                    for parent in path.ancestors().skip(1).take(3) {
                        if !registered.contains(parent)
                            && w.watch(parent, RecursiveMode::NonRecursive).is_ok()
                        {
                            registered.insert(parent.to_path_buf());
                        }
                    }
                }
            }
            let islands = host.enabled().await;
            // The companion can start after the applet. Give it a moment before
            // settling on the Standard bar, so Islands users never see the
            // attached card flash first.
            if startup_retries > 0 && !islands {
                startup_retries -= 1;
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            startup_retries = 0;
            let enabled = host.observe(islands).await;
            if last != Some(enabled) {
                last = Some(enabled);
                if out.send(enabled).await.is_err() {
                    break;
                }
            }
            tokio::select! {
                signal = receive.recv(), if watcher.is_some() => {
                    if signal.is_none() { watcher = None; }
                    // Native reload and config events are independent. Allow a
                    // short settle, then retry once so their ordering cannot
                    // leave the applet in the previous mode until the fallback.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    while receive.try_recv().is_ok() {}
                    let islands = host.enabled().await;
                    let enabled = host.observe(islands).await;
                    if last != Some(enabled) {
                        last = Some(enabled);
                        if out.send(enabled).await.is_err() { break; }
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    registered.clear();
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
        }
    })
}

#[cfg(test)]
mod tests;

fn reveals_allowed() -> bool {
    use crate::desktop::store::{ConfigKey, DesktopStore};
    use cosmic::applet::cosmic_panel_config::AutoHide;
    let Ok(key) = ConfigKey::new("com.system76.CosmicPanel.Panel", 1, "autohide") else {
        return false;
    };
    DesktopStore::from_env()
        .ok()
        .and_then(|s| s.load(std::slice::from_ref(&key)).ok())
        .is_some_and(|s| {
            s.effective(&key)
                .and_then(|v| v.decode::<AutoHide>().ok())
                .is_some_and(|v| matches!(v, AutoHide::Never))
        })
}
