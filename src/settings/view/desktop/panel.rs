//! The panel's shape. Its items use the editor both bars share (applets.rs).
use super::*;
use crate::settings::desktop::PanelStyle;

impl Settings {
    /// Shape: compact, edge to edge, or islands (once they can run here).
    pub(super) fn panel_style(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let islands = draft.islands().known().is_some_and(|s| s.enabled);
        let extend = draft.extend().known().copied();
        let horizontal = draft.anchor().known().is_none_or(|a| a.horizontal());
        let mut options = vec![
            (
                "Compact".into(),
                !islands && extend == Some(false),
                Some(edit(Edit::PanelStyle(PanelStyle::Compact))),
            ),
            (
                if horizontal { "Full width" } else { "Full height" }.into(),
                !islands && extend == Some(true),
                Some(edit(Edit::PanelStyle(PanelStyle::FullWidth))),
            ),
        ];
        if draft.islands().known().is_some() {
            if self.desktop.islands.installation.installed() {
                options.push((
                    "Islands".into(),
                    islands,
                    Some(edit(Edit::PanelStyle(PanelStyle::Islands))),
                ));
            } else if islands {
                options.push(("Islands · not installed".into(), true, None));
            }
        }
        let offered = options.len() > 2;
        let mut shape = vec![segmented(options, self.settings_accent())];
        // A width this editor can't read is kept until a shape is chosen.
        if let Setting::Unsupported(raw) = draft.extend()
            && !islands
        {
            shape.push(kept(&raw));
        }
        if offered {
            shape.push(label(
                "Islands splits the panel into floating pieces.",
                12.,
                MUTED,
            ));
        }
        match shape.len() {
            1 => field("Shape", shape.remove(0)),
            _ => field(
                "Shape",
                cosmic::widget::column::with_children(shape)
                    .spacing(6)
                    .into(),
            ),
        }
    }
}
