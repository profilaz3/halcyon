//! Solar schedules deliberately project a setup onto visual keys only.
//! A whole WidgetStyle domain also contains content, recipes and Clock size.
use crate::desktop::profiles::{AppearanceLook, BackgroundLook, PanelLook, Profile, WallpaperLook};
use crate::desktop::store::widget;
use serde_json::json;

pub fn project(input: &Profile) -> Result<Profile, String> {
    let mut output = Profile::new(&input.name).ok_or("Invalid appearance name")?;
    output.version = input.version;
    // These are integrity and scope contracts, not presentation choices.
    // In particular, keep the digest used to recheck imported pictures.
    output.package = input.package.clone();
    output.composition = input.composition.clone();
    output.appearance_only = input.appearance_only;
    // Preserve the current light/dark policy, including COSMIC's automatic
    // mode. Each solar look supplies both palettes; it never fights that policy.
    output.appearance = input.appearance.as_ref().map(|a| AppearanceLook {
        interface_font: a.interface_font.clone(),
        monospace_font: a.monospace_font.clone(),
        icon_theme: a.icon_theme.clone(),
        light: a.light.as_ref().map(colors),
        dark: a.dark.as_ref().map(colors),
        ..Default::default()
    });
    output.dock = input.dock.as_ref().map(bar);
    output.panel = input.panel.as_ref().map(bar);
    output.wallpaper = input
        .wallpaper
        .as_ref()
        .and_then(|w| w.background.as_ref())
        .map(|bg| WallpaperLook {
            background: Some(BackgroundLook {
                source: bg.source.clone(),
                ..Default::default()
            }),
            ..Default::default()
        });
    if let Some(original) = &input.widget_style {
        let mut style = json!({});
        for pointer in crate::desktop::profiles::themes::STYLE
            .iter()
            .copied()
            .chain([
                "/widget_font",
                "/clock_font",
                "/glance/clock_font",
                "/materials/shared",
                "/materials/widgets",
                "/materials/dock",
                "/materials/panel",
            ])
        {
            if let Some(value) = original.pointer(pointer) {
                widget::put(&mut style, pointer, Some(value.clone()))?;
            }
        }
        output.widget_style = Some(style);
    }
    Ok(output)
}

fn colors(v: &crate::desktop::profiles::VariantLook) -> crate::desktop::profiles::VariantLook {
    crate::desktop::profiles::VariantLook {
        accent: v.accent.clone(),
        background: v.background.clone(),
        primary: v.primary.clone(),
        secondary: v.secondary.clone(),
        outline: v.outline.clone(),
        ..Default::default()
    }
}
fn bar(p: &PanelLook) -> PanelLook {
    // Spacing, gaps, floating, margins and size are geometry. Applets, output
    // assignments and the Islands enabled state also stay exactly as they are.
    PanelLook {
        opacity: p.opacity,
        border_radius: p.border_radius,
        background: p.background.clone(),
        ..Default::default()
    }
}
