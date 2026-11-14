//! Restore ownership outlives history pruning and Undo. A conservative barrier
//! is durable before the first write; acknowledgement lives in the journaled
//! history record, so a killed worker cannot acknowledge an uncommitted result.
use super::*;
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Epoch {
    pub(super) generation: u64,
    pub(super) manual_at: Option<String>,
}
const EPOCH: &str = "restore-epoch.json";
impl DesktopStore {
    pub(super) fn epoch(&self) -> Result<Epoch, DesktopError> {
        super::super::recovery::read_path(&self.state_dir.join(EPOCH), 4096)?
            .map(|(bytes, _)| {
                serde_json::from_slice(&bytes)
                    .map_err(|e| DesktopError::Io(format!("Invalid restore ownership: {e}")))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }
    pub fn restore_epoch(&self) -> Result<u64, DesktopError> {
        Ok(self.epoch()?.generation)
    }
    pub(super) fn advance_restore_epoch(&self) -> Result<(), DesktopError> {
        let generation = self
            .restore_epoch()?
            .checked_add(1)
            .ok_or_else(|| DesktopError::Io("Restore generation exhausted".into()))?;
        let next = Epoch {
            generation,
            manual_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        super::super::recovery::write_scoped(
            &self.state_dir.join(EPOCH),
            Some(&serde_json::to_vec(&next).map_err(|e| DesktopError::Io(e.to_string()))?),
        )
        .map_err(|e| io_error("Could not retain restore ownership", e))
    }
    pub fn palette_seen(&self, epoch: u64, stamp: u64) -> Result<bool, DesktopError> {
        if let Some(ack) = self.history_file()?.and_then(|f| f.palette_ack) {
            return Ok(ack == (epoch, stamp));
        }
        // Read-only compatibility with pre-epoch releases, only before the
        // first restore. New writers never update this unjournaled legacy file.
        if epoch == 0 {
            return Ok(super::super::recovery::read_path(
                &self.state_dir.join("wallpaper-colors-seen.json"),
                64,
            )?
            .and_then(|(b, _)| String::from_utf8(b).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
                == Some(stamp));
        }
        Ok(false)
    }
    /// Stamp zero means the wallpaper is opaque/unavailable. Real source stamps
    /// are nonzero, so a later readable wallpaper is never suppressed by this.
    pub(super) fn restore_palette_ack(&self) -> Result<(u64, u64), DesktopError> {
        let epoch = self.restore_epoch()?;
        let stamp = super::super::wallpaper::WallpaperDraft::load(self)
            .ok()
            .map(|draft| super::super::wallpaper::automatic::source(self, &draft).stamp)
            .unwrap_or(0);
        Ok((epoch, stamp))
    }
    pub(super) fn acknowledge_recovered_restore(&self, id: &str) -> Result<(), DesktopError> {
        let (epoch, stamp) = self.restore_palette_ack()?;
        let mut file = self.history_file()?.unwrap_or_else(|| {
            let mut file = LatestChange {
                id: id.into(),
                owner: Owner::Wallpaper,
                label: "Recovered desktop".into(),
                created: chrono::Utc::now().to_rfc3339(),
                change: AppliedChange {
                    id: id.into(),
                    entries: Vec::new(),
                },
                widget: None,
                item: None,
            }
            .to_file();
            file.source = Source::Derived;
            file.transaction_id = Some(id.into());
            file
        });
        file.palette_ack = Some((epoch, stamp));
        let bytes = serde_json::to_vec(&file).map_err(|e| DesktopError::Io(e.to_string()))?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(DesktopError::Io("Recovered history is too large".into()));
        }
        self.update_record(
            Record::Set(Owner::Wallpaper, "Recovered desktop", Source::Derived),
            Some(&bytes),
        )
        .map_err(DesktopError::Io)
    }
    pub fn apply_palette(
        &self,
        patch: &DesktopPatch,
        widgets: &WidgetPatch,
        epoch: u64,
        stamp: u64,
    ) -> Result<Option<LatestChange>, DesktopError> {
        let mut patch = patch.clone();
        patch.epoch_guard = Some(epoch);
        patch.palette_ack = Some(stamp);
        let c = self.transact(
            &patch,
            Some(widgets),
            Operation::Apply,
            Record::Set(
                Owner::Wallpaper,
                "Automatic wallpaper colors",
                Source::Derived,
            ),
        )?;
        if c.change.is_empty() && c.widget.is_none() {
            return Ok(None);
        }
        Ok(Some(LatestChange {
            id: c.change.id.clone(),
            owner: Owner::Wallpaper,
            label: "Automatic wallpaper colors".into(),
            created: c.created,
            change: c.change,
            widget: c.widget,
            item: None,
        }))
    }
}
