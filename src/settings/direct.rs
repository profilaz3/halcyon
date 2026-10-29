//! Completion boundaries for ordinary controls. Builders stage only the
//! current gesture or form; every completed action finishes in its inspector.
use super::*;

pub(super) fn completion(message: &Message) -> Option<Message> {
    use desktop::Message as Dock;
    use system_appearance::Message as Appearance;
    use wallpaper::Message as Wallpaper;
    use windows::Message as Windows;
    match message {
        Message::DesktopEditor(edit)
            if matches!(
                edit,
                Dock::RadiusChoice(_)
                    | Dock::Anchor(_)
                    | Dock::Size(_)
                    | Dock::AutoHide(_)
                    | Dock::Floating(_)
                    | Dock::Extend(_)
                    | Dock::Output(_)
                    | Dock::Background(_)
                    | Dock::KeepStyle(_)
                    | Dock::MoveTo(..)
                    | Dock::UseStarter
                    | Dock::PanelStyle(_)
                    | Dock::Add(..)
                    | Dock::AddToBar(_)
                    | Dock::Remove(_)
                    | Dock::Step(..)
                    | Dock::Drop
                    | Dock::DropAt(_)
                    | Dock::Islands(
                        desktop::islands::Edit::Enabled(_)
                            | desktop::islands::Edit::Placement(_)
                            | desktop::islands::Edit::Done
                            | desktop::islands::Edit::End(_)
                            | desktop::islands::Edit::Move(_)
                            | desktop::islands::Edit::GapFinished
                    )
            ) =>
        {
            Some(Message::DesktopEditor(Dock::Apply))
        }
        Message::SystemAppearance(edit)
            if matches!(
                edit,
                Appearance::Mode(_)
                    | Appearance::Color(..)
                    | Appearance::HexSubmit(_)
                    | Appearance::Roundness(_)
                    | Appearance::Density(_)
                    | Appearance::Frosted(..)
                    | Appearance::Window(..)
                    | Appearance::FontFamily(..)
                    | Appearance::FontWeight(..)
                    | Appearance::FontStyle(..)
                    | Appearance::IconTheme(_)
            ) =>
        {
            Some(Message::SystemAppearance(Appearance::Apply))
        }
        Message::Windows(edit)
            if matches!(
                edit,
                Windows::Autotile(_) | Windows::Scope(_) | Windows::Flag(..) | Windows::EdgeSnap(_)
            ) =>
        {
            Some(Message::Windows(Windows::Apply))
        }
        Message::Workspaces(edit)
            if matches!(
                edit,
                workspaces::Message::Mode(_)
                    | workspaces::Message::Layout(_)
                    | workspaces::Message::Typing(_)
                    | workspaces::Message::Wraparound(_)
            ) =>
        {
            Some(Message::Workspaces(workspaces::Message::Apply))
        }
        Message::DesktopIcons(desktop_icons::Message::Show(..)) => {
            Some(Message::DesktopIcons(desktop_icons::Message::Apply))
        }
        Message::WindowRules(edit)
            if matches!(
                edit,
                window_rules::Message::Save
                    | window_rules::Message::Toggle(..)
                    | window_rules::Message::Remove(..)
            ) =>
        {
            Some(Message::WindowRules(window_rules::Message::Apply))
        }
        Message::Shortcuts(edit)
            if matches!(
                edit,
                shortcuts::Message::Save
                    | shortcuts::Message::Answer(_)
                    | shortcuts::Message::Running(..)
                    | shortcuts::Message::Default(..)
                    | shortcuts::Message::Remove(_)
                    | shortcuts::Message::RemoveCopy(_)
                    | shortcuts::Message::Restore(_)
            ) =>
        {
            Some(Message::Shortcuts(shortcuts::Message::Apply))
        }
        Message::Wallpaper(edit)
            if matches!(
                edit,
                Wallpaper::ThemeColor
                    | Wallpaper::Accent(_)
                    | Wallpaper::AccentSubmit
                    | Wallpaper::Palette(_)
                    | Wallpaper::SyncColors(_)
                    | Wallpaper::AutomaticColors(_)
                    | Wallpaper::LinkOutline(_)
                    | Wallpaper::SuggestAccent
                    | Wallpaper::KeepColors
            ) =>
        {
            Some(Message::Wallpaper(Wallpaper::ApplyColors))
        }
        Message::Theme(_) => Some(Message::Wallpaper(Wallpaper::ApplyColors)),
        Message::Placement(edit)
            if matches!(
                edit,
                placement::Message::Release
                    | placement::Message::Nudge(..)
                    | placement::Message::Resize(..)
                    | placement::Message::Automatic
                    | placement::Message::Center(_)
                    | placement::Message::Snap(_)
                    | placement::Message::Reset
            ) =>
        {
            Some(Message::Placement(placement::Message::Save))
        }
        _ => None,
    }
}
pub(super) fn label(message: &Message) -> String {
    match message {
        Message::Moment(_) => "Moment",
        Message::WidgetDisplay(_) | Message::WidgetMonitor(..) | Message::WidgetMonitorHide(..) => {
            "Widget display"
        }
        Message::DesktopEditor(edit) => match edit {
            desktop::Message::Opacity(_) => "Dock opacity",
            desktop::Message::Anchor(_) => "Dock position",
            _ => "Dock & panel",
        },
        Message::SystemAppearance(edit) => match edit {
            system_appearance::Message::Mode(_) => "Mode",
            system_appearance::Message::FontFamily(..)
            | system_appearance::Message::FontWeight(..)
            | system_appearance::Message::FontStyle(..) => "Font",
            _ => "Appearance",
        },
        Message::MaterialChoose(_) | Message::MaterialSave => "Glass material",
        Message::MaterialMotion(_) => "Liquid widget motion",
        Message::Widget(_, _) => "Widget visibility",
        Message::Placement(_) => "Widget arrangement",
        Message::ChooseCity(_) => "Weather city",
        Message::AppearanceValue(_, _) | Message::SaveAppearance => "Widget style",
        Message::ClockScale(_)
        | Message::ClockAlignment(_)
        | Message::ClockFormat(_)
        | Message::ClockDate(_)
        | Message::ClockVertical(_)
        | Message::ClockDepth(_) => "Clock",
        edit if moments::clock_style_edit(edit).is_some() => "Clock",
        Message::GlanceOpacity(_) | Message::GlanceStyle(_) | Message::GlanceDensity(_) => {
            "Daily overview"
        }
        Message::Desktop(_) => "Show widgets",
        _ => "Settings",
    }
    .into()
}

impl Settings {
    pub(super) fn completed_control(&mut self, message: Option<Message>) -> Task<Message> {
        let Some(message) = message else {
            return Task::none();
        };
        self.handle(message)
    }
}

/// Explicit equal-valued choices still customize that property. The mask is
/// committed with the value; a no-op choice records only its intent metadata.
pub(super) fn property(app: &Settings, message: &Message) -> u64 {
    match message {
        Message::DesktopEditor(desktop::Message::Apply | desktop::Message::RadiusChoice(_)) => {
            app.native_property
        }
        Message::DesktopEditor(desktop::Message::Background(_)) => {
            native_bar_property(app.desktop.selected, "background")
        }
        Message::SystemAppearance(system_appearance::Message::Color(role, Some(_))) => {
            crate::desktop::actions::override_for_native(
                &role.config_key(app.system_appearance.variant),
            )
        }
        Message::SystemAppearance(system_appearance::Message::HexSubmit(role))
            if app
                .system_appearance
                .hex_text(*role)
                .and_then(|text| crate::desktop::appearance::parse_hex(text, role.alpha()))
                .is_some() =>
        {
            crate::desktop::actions::override_for_native(
                &role.config_key(app.system_appearance.variant),
            )
        }
        Message::SaveAppearance | Message::FlushEdits | Message::Page(_) => app
            .appearance_pending
            .iter()
            .map(|control| match control {
                AppearanceControl::Opacity => widget_property("opacity"),
                AppearanceControl::Corners => widget_property("corner_radius"),
                AppearanceControl::Border => widget_property("border_opacity"),
            })
            .fold(0, |a, b| a | b),
        Message::WidgetSurface(Some(_)) => widget_property("surface"),
        Message::ContrastAssist(_) => widget_property("contrast_assist"),
        Message::AppearancePreset(..) => ["opacity", "corner_radius", "border_opacity"]
            .into_iter()
            .map(widget_property)
            .fold(0, |a, b| a | b),
        _ => 0,
    }
}
fn widget_property(name: &str) -> u64 {
    crate::desktop::actions::override_for_widget(&format!("/appearance/{name}"))
}
fn native_bar_property(kind: crate::desktop::panels::PanelKind, name: &str) -> u64 {
    let key =
        crate::desktop::ConfigKey::new(kind.namespace(), crate::desktop::panels::VERSION, name)
            .expect("known panel property");
    crate::desktop::actions::override_for_native(&key)
}
/// Shared by synchronous and worker admission; completion still owns the commit.
pub(super) fn bar_property(
    kind: crate::desktop::panels::PanelKind,
    edit: &desktop::Message,
) -> u64 {
    let name = match edit {
        desktop::Message::Opacity(_) => "opacity",
        desktop::Message::Radius(_) | desktop::Message::RadiusChoice(_) => "border_radius",
        desktop::Message::Padding(_) => "padding",
        desktop::Message::Spacing(_) => "spacing",
        _ => return 0,
    };
    native_bar_property(kind, name)
}
