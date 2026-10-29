//! Arrange holds a companion lease. Only Apply writes the sidecar.
use super::*;
use crate::desktop::islands::Client;
use dock_islands_core::{Handle, Layout, Placement, Request, Response, Section, Settings, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Enabled(bool),
    Placement(Placement),
    Gap(u16),
    GapText(String),
    GapFinished,
    Select(Section),
    Arrange,
    Done,
    Cancel,
    Check,
    Sync,
    Move(i32),
    End(bool),
    Group(bool),
}
#[derive(Default)]
pub struct State {
    pub installation: crate::desktop::islands::Installation,
    pub compatible: bool,
    pub status: Option<String>,
    pub selected: Option<Section>,
    pub group: bool,
    pub gap_text: Option<String>,
    pub lease: Option<Lease>,
    pub geometry: Option<Layout>,
    pub measured_output: Option<(PanelKind, Output, String)>,
}
pub struct Lease {
    client: Client,
    token: u64,
    revision: u64,
    pub(super) before: Settings,
    pub(super) kind: PanelKind,
}
impl DesktopEditor {
    pub fn arranging(&self) -> bool {
        self.islands.lease.is_some()
    }
    pub fn requested_island_output(&self) -> String {
        match self.current().map(PanelDraft::output) {
            Some(Setting::Known(Output::Name(name))) => name,
            _ => String::new(),
        }
    }
    pub fn island_output(&self) -> String {
        if let Some((kind, configured, actual)) = &self.islands.measured_output
            && *kind == self.selected
            && self
                .current()
                .and_then(|d| d.output().known().cloned())
                .as_ref()
                == Some(configured)
        {
            return actual.clone();
        }
        self.requested_island_output()
    }
    fn remember_island_output(&mut self, actual: String) {
        self.islands.measured_output = self
            .current()
            .and_then(|d| d.output().known().cloned())
            .map(|configured| (self.selected, configured, actual));
    }
    pub fn check_islands(&mut self) {
        if self.store().is_some_and(|s| {
            crate::desktop::islands::Installation::on_native_thread()
                && crate::desktop::islands::Installation::needs_worker(s)
        }) {
            // The event handler only consumes the structured background result.
            if !self.islands.installation.installed() {
                self.penumbra_supported = false;
                self.islands.lease = None;
                self.islands.geometry = None;
                self.islands.measured_output = None;
                self.islands.compatible = false;
                self.islands.status = None;
            }
            return;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        if let Some(store) = self.store() {
            self.islands.installation =
                crate::desktop::islands::Installation::discover_until(store, deadline);
        }
        if !self.islands.installation.installed() {
            self.penumbra_supported = false;
            self.islands.lease = None;
            self.islands.geometry = None;
            self.islands.measured_output = None;
            self.islands.compatible = false;
            self.islands.status = None;
            return;
        }
        let mut result = self
            .store()
            .ok_or_else(|| "Desktop settings are unavailable".to_string())
            .and_then(|store| {
                if crate::desktop::islands::Installation::on_native_thread() {
                    // Isolated fixtures do not run a production helper.
                    Client::connect(store)
                } else {
                    Client::connect_observed(store, &self.islands.installation, deadline)
                }
            })
            .and_then(|mut client| {
                client.capabilities()?;
                Ok(client)
            });
        self.islands.compatible = result.is_ok();
        self.penumbra_supported = result
            .as_mut()
            .is_ok_and(|client| client.penumbra_supported());
        self.islands.status = result.as_ref().err().cloned();
        if !self.arranging() {
            self.islands.geometry = None;
            self.islands.measured_output = None;
            if let Ok(mut client) = result
                && let Ok(Response::Draft { layout, view, .. }) =
                    client.request(&Request::Inspect {
                        panel: self.selected.entry().into(),
                        output: self.requested_island_output(),
                    })
            {
                self.islands.geometry = Some(layout);
                self.remember_island_output(view.output);
            }
        }
    }
    pub fn islands_can_apply(&self) -> bool {
        !self.arranging()
            && self.drafts.values().all(|d| {
                !d.edited().any(|name| name == "islands") || self.islands.installation.installed()
            })
    }
    fn sync_island_lease(&mut self) -> bool {
        let Some(lease) = &mut self.islands.lease else {
            return true;
        };
        let result = lease.client.request(&Request::Read { token: lease.token });
        match result {
            Ok(Response::Draft {
                revision,
                settings,
                layout,
                view,
                ..
            }) => {
                lease.revision = revision;
                if let Some(handle) = view.selected {
                    self.islands.group = handle == Handle::Group;
                    self.islands.selected = match handle {
                        Handle::Island(id) => Some(id),
                        Handle::Group => None,
                    };
                }
                self.islands.geometry = Some(layout);
                if let Some(draft) = self.drafts.get_mut(&lease.kind) {
                    draft.set_islands(settings);
                }
                true
            }
            response => {
                let lease = self.islands.lease.take().unwrap();
                if let Some(draft) = self.drafts.get_mut(&lease.kind) {
                    draft.set_islands(lease.before);
                }
                let error = response
                    .err()
                    .unwrap_or("The arrangement session ended".into());
                self.notice = Some(if error == "The arrangement session ended" {
                    Notice::Done("Arrangement cancelled".into())
                } else {
                    Notice::Error(error)
                });
                self.check_islands();
                false
            }
        }
    }
    fn select_island_handle(&mut self, handle: Handle) {
        if let Some(lease) = &mut self.islands.lease {
            if let Err(error) = lease.client.request(&Request::Select {
                token: lease.token,
                handle,
            }) {
                self.notice = Some(Notice::Error(error));
            }
        }
    }
    fn set_island_draft(&mut self, change: impl FnOnce(&mut Settings)) {
        if !self.sync_island_lease() {
            return;
        }
        let Some(Setting::Known(mut s)) = self.current().map(PanelDraft::islands) else {
            return;
        };
        change(&mut s);
        if let Err(e) = s.validate() {
            self.notice = Some(Notice::Error(e));
            return;
        }
        if let Some(lease) = &mut self.islands.lease {
            let response = lease.client.request(&Request::Update {
                token: lease.token,
                revision: lease.revision,
                draft: s.clone(),
            });
            match response {
                Ok(Response::Draft {
                    revision, layout, ..
                }) => {
                    lease.revision = revision;
                    self.islands.geometry = Some(layout);
                }
                Ok(_) => {
                    self.notice = Some(Notice::Error("Unexpected panel reply".into()));
                    return;
                }
                Err(e) => {
                    self.notice = Some(Notice::Error(e));
                    return;
                }
            }
        }
        self.edit(|d| d.set_islands(s));
    }
    pub fn island_edit(&mut self, edit: Edit) {
        if !matches!(edit, Edit::Cancel | Edit::Check) {
            let observed = self
                .store()
                .map(crate::desktop::islands::Installation::discover)
                .unwrap_or_default();
            // Keep the observation used to admit this edit. A separate status
            // query must not be required just to make the draft manageable.
            self.islands.installation = observed;
            if !self.islands.installation.installed() {
                self.check_islands();
                return;
            }
        }
        match edit {
            Edit::Enabled(enabled) => {
                if self.arranging() {
                    self.island_edit(Edit::Cancel);
                }
                self.set_island_draft(|s| s.enabled = enabled);
                // Apply validates the change; don't repeat discovery and IPC
                // between drafting this value and immediately applying it.
            }
            Edit::Placement(mode) => {
                self.set_island_draft(|s| s.placement = mode);
                self.islands.group = false;
            }
            Edit::Gap(gap) => {
                self.set_island_draft(|s| s.gap = gap.min(64));
                self.islands.gap_text = None;
            }
            Edit::GapText(text) => {
                let text: String = text.chars().filter(char::is_ascii_digit).take(2).collect();
                if let Ok(gap) = text.parse::<u16>()
                    && gap <= 64
                {
                    self.set_island_draft(|s| s.gap = gap);
                }
                self.islands.gap_text = Some(text);
            }
            Edit::GapFinished => {}
            Edit::Select(s) => {
                self.select_island_handle(Handle::Island(s));
                self.islands.selected = Some(s);
                self.islands.group = false;
                self.focus = Some(Slot {
                    region: match s {
                        Section::Start => Region::Start,
                        Section::Center => Region::Center,
                        Section::End => Region::End,
                    },
                    index: 0,
                });
            }
            Edit::Group(group) => {
                self.islands.group = group;
                if group {
                    self.islands.selected = None;
                    self.select_island_handle(Handle::Group);
                }
            }
            Edit::Check => self.check_islands(),
            Edit::Sync => {
                self.sync_island_lease();
            }
            Edit::Arrange => {
                if self.arranging() {
                    return;
                }
                let result = (|| {
                    let draft = self.current().ok_or("No panel selected")?;
                    if draft.edited().any(|key| key != "islands") {
                        return Err(
                            "Apply screen edge and applet changes before arranging on screen"
                                .into(),
                        );
                    }
                    let settings = draft
                        .islands()
                        .known()
                        .cloned()
                        .ok_or("Island settings cannot be read")?;
                    let base = draft
                        .islands_base()
                        .ok_or("Island settings cannot be read")?;
                    let mut client = Client::connect(self.store().ok_or("Desktop unavailable")?)?;
                    client.capabilities()?;
                    let response = client.request(&Request::Begin {
                        panel: self.selected.entry().into(),
                        output: self.requested_island_output(),
                        base,
                        draft: settings.clone(),
                    })?;
                    match response {
                        Response::Draft {
                            token,
                            revision,
                            layout,
                            view,
                            ..
                        } => Ok((
                            Lease {
                                client,
                                token,
                                revision,
                                before: settings,
                                kind: self.selected,
                            },
                            layout,
                            view.output,
                        )),
                        _ => Err("The panel did not start Arrange".into()),
                    }
                })();
                match result {
                    Ok((lease, layout, output)) => {
                        self.remember_island_output(output);
                        self.islands.lease = Some(lease);
                        self.islands.geometry = Some(layout);
                        self.notice = None;
                    }
                    Err(e) => self.notice = Some(Notice::Error(e)),
                }
            }
            Edit::Done | Edit::Cancel => {
                let Some(mut lease) = self.islands.lease.take() else {
                    return;
                };
                let cancel = edit == Edit::Cancel;
                let response = lease.client.request(&if cancel {
                    Request::Cancel { token: lease.token }
                } else {
                    Request::Done { token: lease.token }
                });
                match response {
                    Ok(Response::Draft { settings, .. }) if !cancel => {
                        if let Some(d) = self.drafts.get_mut(&lease.kind) {
                            d.set_islands(settings);
                        }
                        self.notice = None;
                    }
                    Ok(Response::Cancelled) if cancel => {
                        if let Some(d) = self.drafts.get_mut(&lease.kind) {
                            d.set_islands(lease.before);
                        }
                        self.notice = None;
                    }
                    result => {
                        if let Some(d) = self.drafts.get_mut(&lease.kind) {
                            d.set_islands(lease.before);
                        }
                        self.notice = Some(Notice::Error(
                            result
                                .err()
                                .unwrap_or("Arrange ended without a result".into()),
                        ));
                    }
                }
                self.check_islands();
            }
            Edit::Move(_) | Edit::End(_) => {
                if !self.sync_island_lease() {
                    return;
                }
                let output = self.island_output();
                let handle = if self.islands.group {
                    Handle::Group
                } else {
                    Handle::Island(self.islands.selected.unwrap_or(Section::Center))
                };
                let Some(d) = self.current() else {
                    return;
                };
                let Some(mut s) = d.islands().known().cloned() else {
                    return;
                };
                let mode = s.placement;
                if mode == Placement::Together
                    && let Handle::Island(id) = handle
                {
                    let present = preview_lengths(d).map(|length| length > 0.);
                    let steps = match edit {
                        Edit::End(false) => -3,
                        Edit::End(true) => 3,
                        Edit::Move(n) => n.signum() as i8,
                        _ => unreachable!(),
                    };
                    s.layout_mut(&output)
                        .reorder_visible(mode, id, present, steps);
                } else {
                    let measured = self.islands.geometry.as_ref();
                    let natural = measured.map_or_else(
                        || preview_lengths(d),
                        |l| l.spans.map(|s| s.map_or(0., |s| s.length)),
                    );
                    let layout = dock_islands_core::resolve(
                        &s,
                        &output,
                        measured.map_or(
                            Span {
                                start: 0.,
                                length: 1024.,
                            },
                            |l| l.bounds,
                        ),
                        natural,
                        32.,
                    );
                    let span = match handle {
                        Handle::Group => Some(layout.group),
                        Handle::Island(id) => layout.spans[id.index()],
                    };
                    if let Some(span) = span {
                        let wanted = match edit {
                            Edit::End(false) => layout.bounds.start,
                            Edit::End(true) => layout.bounds.end() - span.length,
                            Edit::Move(n) => span.start + 8. * n as f64,
                            _ => unreachable!(),
                        };
                        dock_islands_core::move_handle(
                            &mut s, &output, &layout, handle, wanted, false,
                        );
                    }
                }
                self.set_island_draft(|current| *current = s);
            }
        }
    }
    pub fn acknowledge_islands(&mut self) -> Result<(), String> {
        let store = self.store().ok_or("Desktop unavailable")?;
        let mut client = Client::connect(store)?;
        for d in self.drafts.values() {
            let Some(settings) = d.islands().known().cloned() else {
                continue;
            };
            if !d.listed() {
                continue;
            }
            if !matches!(client.request(&Request::Activate{panel:d.kind().entry().into(),expected:settings.clone()})?,Response::Applied{settings:ack} if ack==settings)
            {
                return Err("The panel did not acknowledge the saved layout".into());
            }
        }
        Ok(())
    }
}
pub fn preview_lengths(draft: &PanelDraft) -> [f64; 3] {
    let regions = draft.applets().known().cloned().unwrap_or_default();
    let size = draft
        .size()
        .known()
        .cloned()
        .unwrap_or(PanelSize::M)
        .thickness() as f64;
    let padding = draft.padding().known().copied().unwrap_or(4) as f64;
    let spacing = draft.spacing().known().copied().unwrap_or(4) as f64;
    Region::ALL.map(|r| {
        let n = regions.list(r).len();
        if n == 0 {
            0.
        } else {
            n as f64 * size + (n - 1) as f64 * spacing + padding * 2.
        }
    })
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeStatus {
    pub panel: Option<PanelKind>,
    pub requested_output: String,
    pub compatible: bool,
    pub material: bool,
    pub penumbra: bool,
    pub status: Option<String>,
    pub geometry: Option<Layout>,
    pub output: Option<String>,
}
impl RuntimeStatus {
    pub fn read(
        store: &DesktopStore,
        installation: &crate::desktop::islands::Installation,
        panel: PanelKind,
        output: String,
        deadline: std::time::Instant,
    ) -> Self {
        if !installation.installed() || installation.needs_recheck() {
            return Self::default();
        }
        let mut result = Self {
            panel: Some(panel),
            requested_output: output.clone(),
            ..Self::default()
        };
        match Client::connect_observed(store, installation, deadline).and_then(|mut c| {
            c.capabilities()?;
            Ok(c)
        }) {
            Ok(mut client) => {
                result.compatible = true;
                result.penumbra = client.penumbra_supported();
                result.material = matches!(
                    client.request(&Request::MaterialSupport),
                    Ok(Response::MaterialSupport { version: 1 })
                );
                if let Ok(Response::Draft { layout, view, .. }) =
                    client.request(&Request::Inspect {
                        panel: panel.entry().into(),
                        output,
                    })
                {
                    result.geometry = Some(layout);
                    result.output = Some(view.output);
                }
            }
            Err(error) => result.status = Some(error),
        }
        result
    }
}
impl DesktopEditor {
    pub fn accept_runtime(&mut self, status: RuntimeStatus) {
        self.penumbra_supported = status.penumbra;
        self.islands.compatible = status.compatible;
        self.islands.status = status.status;
        if !self.arranging()
            && status.panel == Some(self.selected)
            && status.requested_output == self.requested_island_output()
        {
            self.islands.geometry = status.geometry;
            if let Some(output) = status.output {
                self.remember_island_output(output);
            } else {
                self.islands.measured_output = None;
            }
        }
    }
}
impl State {
    /// Installation is the background worker's observation, independent of a
    /// per-user generation or live socket. Prefer it to legacy IPC diagnostics
    /// that can mistake an installed-but-disabled add-on for a missing one.
    pub fn availability_message(&self) -> Option<&str> {
        use crate::desktop::islands::{CompatibilityStatus, Installation};
        match &self.installation {
            Installation::Pending
            | Installation::Installed {
                status: Some(CompatibilityStatus::Checking),
                ..
            } => Some(
                "Checking Islands compatibility. The standard dock and panel remain available.",
            ),
            Installation::Absent => Some(
                "Install the Islands add-on to use this style. The standard dock and panel remain available.",
            ),
            Installation::Installed {
                compatible: Some(false),
                ..
            } => Some(
                "Islands is installed but unavailable for this desktop. The standard dock and panel remain available. Check diagnostics in App settings.",
            ),
            Installation::Installed { enabled: false, .. } => Some(
                "Islands is installed but disabled. Manage it in App settings. The standard dock and panel remain available.",
            ),
            Installation::Installed {
                status: Some(CompatibilityStatus::LogoutRequired),
                ..
            } => Some("Islands is enabled. Log out and back in to activate it."),
            Installation::Installed { .. } => self.status.as_deref(),
        }
    }

    /// Display snapshot while the sole lease owner is executing on a worker.
    pub fn display_snapshot(&self) -> Self {
        Self {
            installation: self.installation.clone(),
            compatible: self.compatible,
            status: self.status.clone(),
            selected: self.selected,
            group: self.group,
            gap_text: self.gap_text.clone(),
            lease: None,
            geometry: self.geometry.clone(),
            measured_output: self.measured_output.clone(),
        }
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    fn warning_observation(
        enabled: bool,
        compatible: Option<bool>,
        status: crate::desktop::islands::CompatibilityStatus,
    ) -> crate::desktop::islands::Installation {
        let fixture = crate::desktop::store::tests::Fixture::new("islands-warning-observation");
        std::fs::write(
            fixture.0.join("islands-installation.json"),
            serde_json::to_vec(&serde_json::json!({
                "format": "halcyon-islands-installation", "version": 1,
                "installation": "installed", "provenance": "package",
                "enabled": enabled, "compatible": compatible,
                "status_version": 2, "status": status.label(),
            }))
            .unwrap(),
        )
        .unwrap();
        crate::desktop::islands::Installation::discover(&fixture.store())
    }

    #[test]
    fn installed_islands_warning_uses_observed_state_instead_of_legacy_socket_error() {
        use crate::desktop::islands::CompatibilityStatus;
        let installed = warning_observation;
        let mut state = State {
            // This is the contradictory diagnostic seen in the r3 screenshot:
            // the package exists, but no per-user generation was ever enabled.
            status: Some("Install and enable the Islands add-on to use this style.".into()),
            ..Default::default()
        };
        for (observation, expected) in [
            (
                installed(false, Some(false), CompatibilityStatus::Disabled),
                "installed but unavailable",
            ),
            (
                installed(false, Some(true), CompatibilityStatus::Disabled),
                "installed but disabled",
            ),
            (
                installed(false, None, CompatibilityStatus::Checking),
                "Checking Islands compatibility",
            ),
            (
                installed(true, Some(true), CompatibilityStatus::LogoutRequired),
                "Log out and back in",
            ),
        ] {
            state.installation = observation.clone();
            let warning = state.availability_message().unwrap();
            assert!(warning.contains(expected), "{warning}");
            assert!(!warning.contains("Install and enable"));
            assert_eq!(
                state.installation, observation,
                "copy does not change admission"
            );
            assert!(!state.compatible, "copy does not enable Arrange");
        }
    }

    #[test]
    fn islands_warning_preserves_absence_pending_and_live_host_errors() {
        use crate::desktop::islands::{CompatibilityStatus, Installation};
        let mut state = State::default();
        assert!(
            state
                .availability_message()
                .unwrap()
                .starts_with("Checking")
        );
        state.installation = Installation::Absent;
        assert!(
            state
                .availability_message()
                .unwrap()
                .starts_with("Install the Islands")
        );
        state.installation = warning_observation(true, Some(true), CompatibilityStatus::Standard);
        state.status = Some("The panel socket is not owned by this user".into());
        assert_eq!(state.availability_message(), state.status.as_deref());
        state.status = None;
        state.compatible = true;
        assert_eq!(state.availability_message(), None);
    }

    #[test]
    fn task2r1_runtime_result_is_bound_to_the_requested_panel() {
        let mut editor = DesktopEditor::default();
        editor.selected = PanelKind::Panel;
        let status = RuntimeStatus {
            panel: Some(PanelKind::Dock),
            geometry: Some(Layout::default()),
            output: Some("dock-output".into()),
            ..Default::default()
        };
        editor.accept_runtime(status.clone());
        assert!(
            editor.islands.geometry.is_none(),
            "late Dock geometry cannot become Panel geometry"
        );
        editor.accept_runtime(RuntimeStatus {
            panel: Some(PanelKind::Panel),
            requested_output: "old-output".into(),
            ..status.clone()
        });
        assert!(
            editor.islands.geometry.is_none(),
            "late output geometry cannot replace the selected output"
        );
        editor.accept_runtime(RuntimeStatus {
            panel: Some(PanelKind::Panel),
            ..status
        });
        assert!(editor.islands.geometry.is_some());
    }
}
