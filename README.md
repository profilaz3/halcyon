# Halcyon

Desktop widgets, themes and layouts for **COSMIC on Pop!_OS 24.04**.

**[Download](https://github.com/profilaz3/halcyon/releases/latest)** · [Release notes](https://github.com/profilaz3/halcyon/releases) · [Report an issue](https://github.com/profilaz3/halcyon/issues)

<!-- Keep this overview stable between releases. See .github/README-MAINTENANCE.md. -->

[![Tidal setup in native Halcyon Settings](docs/screenshots/040/setup-preview.png)](docs/screenshots/040/README.md)

- **Widgets:** clock, weather, calendar, system monitor, music, Git projects, notes and focus timer.
- **Layout:** move, resize and order widgets; choose which display they use.
- **Appearance:** wallpapers, colors, fonts and surface finishes. Settings and setup use GPU rendering.
- **Setups:** save, combine, share and schedule desktop configurations.

## Install

1. Open the [latest release](https://github.com/profilaz3/halcyon/releases/latest) and download `cosmic-ext-halcyon_<version>_amd64.deb`. Islands is optional.
2. Open the file with **COSMIC Store**, choose **Install**, and approve the system password prompt if asked.
3. Open **Halcyon** from the application launcher and finish setup.

Packages support 64-bit Intel and AMD PCs. An internet connection is needed to download any missing dependencies.

Fresh installations enable the signed update source. Install future updates through **COSMIC Store → Updates** or APT. Automatic installation follows your OS settings; existing disabled or modified sources are preserved.

[Terminal installation, updates and removal](docs/DISTRIBUTION.md).

## Optional Islands

Islands splits the dock and panel into floating applet groups and adds bar surface finishes. Widgets and themes work without it.

Open **App settings → Updates → Get Islands…**, install the add-on, then choose **Enable for next login**. Log out and back in.

[Compatibility and manual installation](packaging/islands/README.md).

## Optional Penumbra

Penumbra adds a theme-aware gradient that fades the panel or dock into your wallpaper. Its independent switch is under Desktop → Dock & panel → Style → Penumbra. Floating and Penumbra turn each other off. It requires a panel component built with Penumbra support; enabling Islands is optional. See [Penumbra setup and compatibility](companion/PENUMBRA.md).

## Calendar

Add read-only calendar subscription links from Google Calendar, Outlook or other providers. [Calendar setup guide](docs/CALENDAR.md).

## Privacy

Usage sharing is enabled by default. Turn it off during setup or in **App settings → Privacy**. Reports include a random installation ID, app version, approximate country and first desktop start; they exclude widget content. [Privacy details](docs/PRIVACY.md).

## Development

Built with Rust and libcosmic. [Build and contribute](docs/DEVELOPMENT.md) · [Package details](packaging/README.md) · [GPL-3.0-or-later](LICENSE) · [Third-party notices](THIRD_PARTY.md).
