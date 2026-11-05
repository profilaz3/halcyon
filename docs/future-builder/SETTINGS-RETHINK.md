# Settings rethink: research and screen-by-screen plan

Started 2026-11-04 after hands-on review of the installed build. Verdict: the
current Settings app is not acceptable. Every screen is redone, one at a time,
each agreed with a mockup before code. This supersedes the interaction model in
[APP-UX-REDESIGN.md](APP-UX-REDESIGN.md) where they conflict.

## What is wrong (from the 2026-11-04 screenshots)

| Problem | Where it shows |
| --- | --- |
| **The transaction model is the UI.** Preview / not applied / Review changes / Review conflicts / Discard / Apply / Undo all compete for attention. "Review changes" appears three times on one screen. | Colors |
| **Warnings stack up.** Orange banners for repair, review, install state sit above and below the content. Status reads as "something is broken". | Colors, Panel |
| **Internal vocabulary leaks.** Islands, Looks, Setups, Mix, Penumbra, "Changes: Dock only", "Preview color source", "Window outline · Keep its color". | All |
| **Help bubbles replace clarity.** `?` buttons next to headings explain controls that should explain themselves. | All |
| **Navigation layers.** Back link + sidebar + tab strip + sub-tabs + "Fonts & icons ›" rows + "Customize an element…". You can be four levels deep without knowing where. | Colors, Panel |
| **One concept, several controls.** Wallpaper colour source chips, a Neon card, "Wallpaper: Yours / Neon gradient", and "Color applies to" all express the same choice. Applets reorder by drag handle *and* arrows. | Colors, Panel |
| **Layout without hierarchy.** Two uneven columns, large empty regions, equal visual weight on everything; the preview is small. | Colors, Panel |
| **Browse cards repeat noise.** "Explore setup ›" on every card, duplicated taglines, a status line ("Standard panel · Islands unavailable") above the gallery. | Browse |

## What good products in this space do

| Product | Lesson for Halcyon |
| --- | --- |
| **GNOME Settings / libadwaita** | Instant apply by default. An explicit Apply only for slow or risky changes, grouped with just those controls. Boxed rows, one setting per row, one level of subpage. |
| **macOS System Settings (Appearance, Wallpaper)** | Few choices, large visual swatches, results visible immediately on the real desktop. "Auto" handles light/dark from a schedule instead of a separate concept. |
| **Android "Wallpaper & style" (Material You)** | One screen: wallpaper on top, 4 generated palettes underneath, dark toggle, live preview in a phone mock. Colour derives from wallpaper by default; a "Basic colors" tab is the fallback. |
| **iOS Lock Screen editor** | Direct manipulation: tap the thing on the preview to edit it (clock → font/colour, + → widgets). Options depend on the selected item; no global forms. |
| **Windows 11 Personalization** | Themes as one-click bundles at the top, individual parts (background, colors, taskbar) below as plain rows. Reviewers credit it for feeling designed "all at once". |
| **KDE Plasma 6.6 global themes** | Apply a bundle everywhere or only to chosen parts; save the current desktop as a new theme in one step. |
| **Wallpaper Engine / Lively / Rainmeter** | Gallery-first, big thumbnails, the property panel only for the selected item. Rainmeter is the cautionary tale: power, poor ease of use. |

## Principles for the redo

1. **Changes apply live; Undo is the safety net.** No Apply / Review / Conflicts
   on ordinary screens. A single toast offers Undo after each change. Keep an
   explicit confirm only where a change is slow or could break the session
   (installing Islands, switching panel host). This is the GNOME rule.
2. **One concept, one control.** If two controls express the same choice, delete one.
3. **Plain words.** Name things for what the user sees: "Dock style", "Panel
   widgets", "Desktop look". No internal feature names in UI text.
4. **No `?` buttons and at most one status message per screen**, only when the
   user must act. Healthy state is silent.
5. **Preview is the hero.** Big preview at the top; click an element in it to
   jump to its controls (iOS editor pattern).
6. **Two navigation levels max:** sidebar → page (optionally one subpage).
   No tab strips inside pages unless a page truly has independent halves.
7. **Bundles first, parts second.** Looks/themes as a visual gallery at the top
   of their page; individual parts as rows below (Windows 11, Plasma).

## Screen inventory and order

The sidebar now has 7 roots, one page each: Looks, Appearance, Wallpaper,
Widgets, Dock & panel, Windows, Settings. Every other page sits one level
below its root and has a single back link to it.

| # | Screen | Was | Now (2026-11-04) |
| --- | --- | --- | --- |
| 1 | Appearance | Colors / Surfaces / Fonts & windows tabs | one page: preview, styles, mode and color, rows to Glass and Fonts & icons |
| 2 | Wallpaper | DesktopWallpaper | preview beside fit, displays and moving effect; gallery below; applies instantly |
| 3 | Looks | Setups: Browse / Saved / Mix / Schedules | one gallery, Your looks, then Time of day, Mix and Open a shared look |
| 4 | Dock & panel | separate Dock and Panel pages with tabs | one page with a Dock/Panel switch and sections |
| 5 | Widgets | Overview, WidgetLayout, per-widget tabs | picture, switches, one template for every widget page |
| 6 | Tools (Music, Notes, Focus) | tabbed Tools | same widget template; Notes saves as you type |
| 7 | Windows | Windows, Workspaces, Rules, Shortcuts | one column: tiling, gaps, title bar, focus, workspaces, then rows |
| 8 | Change with time of day | Schedules, Moments, Adaptive looks | Day, Evening and Night each pick a look; other rules folded under More rules |
| 9 | Settings | General, Updates, Recovery, Pending changes | one list with Accessibility first |
| 10 | First-run setup | wizard | four steps with a live preview: look, dock and panel, widgets, review |

Reviews by an independent agent on native captures: 4.5/10 before the redo,
6.9/10 after round 4.

Latest independent scores, against the main Settings pages as the 9/10 bar:

| Surface | Score |
|---|---|
| Main Settings pages | 9 |
| Secondary Settings pages | 9 |
| First-run setup | 8.8 to 9 |
| Widgets rail (desktop drawer) | 9 |
| Desktop widget cards | 9 |
| Activity island | about 9 (last review 8.5, its three items fixed since) |

The same rules carried to the desktop: one filled main action in the accent
colour, quiet text buttons for the rest, sentence case, plain words, 12px
minimum for secondary text, and muted colours tinted by the look's accent.

Each screen goes through: written critique, mockup where the layout changes,
implementation, native screenshots, independent review.

## Separate defect found during the review

The 0.5 build reports "Islands not installed" on a source install (`~/.local`)
because it only probes `/usr/lib/cosmic-ext-halcyon/islands-status.py`, which only
the system package provides. The per-user companion is running but invisible to
Settings.

## Sources

- [GNOME HIG planning: instant vs explicit apply](https://wiki.gnome.org/Design/HIG/Planning/Configuration)
- [Apple: change Appearance settings on Mac](https://support.apple.com/guide/mac-help/change-appearance-settings-mchlp1225/26/mac)
- [Android 12 Material You colour picker](https://tomsguide.com/how-to/how-to-change-the-material-you-color-in-android-12)
- [iOS Lock Screen editor](https://tomsguide.com/how-to/how-to-customize-your-iphones-lock-screen)
- [Windows 11 personalization review (SlashGear)](https://www.slashgear.com/windows-11-first-impressions-4-features-i-love-plus-1-i-hate-04693690)
- [KDE Plasma 6.6 custom global themes](https://linuxiac.com/kde-plasma-6-6-lets-users-create-custom-global-themes/)
- [Plasma 5.25 per-part global theme apply](https://itsfoss.com/news/plasma-5-25-features/)
- [Rainmeter alternatives comparison](https://www.itopvpn.com/desktop-tips/rainmeter-alternative-8080)
- [Progressive disclosure (NN/g)](https://www.nngroup.com/videos/progressive-disclosure/)
