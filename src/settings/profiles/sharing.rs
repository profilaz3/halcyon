use super::*;
impl ProfilesEditor {
    pub fn sharing(&self) -> bool {
        self.share_pending.is_some()
    }
    pub fn take_share_jobs(&mut self) -> Vec<crate::settings::sharing::Job> {
        std::mem::take(&mut self.share_jobs)
    }
    pub(super) fn cancel_share(&mut self) {
        if let Some(cancel) = self.share_cancel.take() {
            let _ = cancel.send(true);
        }
        if self.share_pending.is_some()
            && matches!(&self.notice,Some(Notice::Done(text)) if text=="Preparing sample preview…")
        {
            self.notice = None;
        }
        self.share_pending = None;
        self.share_source = None;
        self.share_jobs.clear();
    }
    pub(super) fn share_image(&mut self) {
        if self.busy() || self.theme_loading() || self.theme_problem().is_some() {
            return;
        }
        if !self.store().is_some_and(|s| s.writable()) {
            self.notice = Some(Notice::Error("Export is off in preview mode".into()));
            return;
        }
        let Some(profile) = self.profile() else {
            return;
        };
        let name = profile.name.clone();
        // Render the selected record, not a snapshot of the user's desktop.
        let scene = match crate::settings::sharing::Scene::new(profile) {
            Ok(scene) => scene,
            Err(error) => {
                self.notice = Some(Notice::Error(error));
                return;
            }
        };
        self.next_request += 1;
        let id = self.next_request;
        let (sender, cancel) = tokio::sync::watch::channel(false);
        self.share_source = Some(scene.clone());
        self.share_pending = Some((id, name, scene.omitted_picture));
        self.share_cancel = Some(sender);
        self.share_jobs
            .push(crate::settings::sharing::Job { id, scene, cancel });
        self.notice = Some(Notice::Done("Preparing sample preview…".into()));
    }
    pub(super) fn share_ready(&mut self, id: u64, result: Result<Vec<u8>, String>) -> Outcome {
        if !self
            .share_pending
            .as_ref()
            .is_some_and(|(wanted, _, _)| *wanted == id)
        {
            return Outcome::None;
        }
        let fresh = self
            .profile()
            .and_then(|profile| crate::settings::sharing::Scene::new(profile).ok());
        let same_name = self.profile().is_some_and(|profile| {
            self.share_pending
                .as_ref()
                .is_some_and(|(_, name, _)| name == &profile.name)
        });
        if fresh != self.share_source || !same_name {
            self.cancel_share();
            self.notice = Some(Notice::Error(
                "The look changed. Save its preview again.".into(),
            ));
            return Outcome::None;
        }
        self.share_source = None;
        let (_, name, omitted) = self.share_pending.take().unwrap();
        self.share_cancel = None;
        match result {
            Ok(bytes)
                if bytes.len() <= crate::settings::sharing::MAX_PNG
                    && bytes.starts_with(b"\x89PNG\r\n\x1a\n") =>
            {
                let confine = self
                    .store()
                    .and_then(|s| s.fixture_root())
                    .map(Path::to_path_buf);
                let file_name =
                    file_name(&name).trim_end_matches(".json").to_owned() + "-preview.png";
                let id = self.request(Pending::Image(name, omitted));
                self.notice = None;
                Outcome::Dialog(Request::ExportImage {
                    id,
                    file_name,
                    bytes,
                    confine,
                })
            }
            Ok(_) => {
                self.notice = Some(Notice::Error(
                    "Could not render the preview. Try again.".into(),
                ));
                Outcome::None
            }
            Err(error) => {
                self.notice = Some(Notice::Error(error));
                Outcome::None
            }
        }
    }
}
