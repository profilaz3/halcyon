mod activity_island;
mod app;
mod automation;
mod calendar;
mod config;
mod data;
mod depth;
mod design;
mod desktop;
mod desktop_theme;
mod devices;
mod displays;
mod drawer;
mod folder;
mod installation;
mod instance;
mod layout;
mod layout_state;
mod materials;
mod messages;
mod markets;
mod moments;
#[cfg(test)]
mod native_test;
mod paths;
mod providers;
mod rain;
mod recipes;
mod schedule;
mod settings;
mod settings_lifecycle;
mod system_theme;
#[cfg(test)]
mod test_env;
mod tiles;
mod tools;
mod upgrade;
mod usage;
mod watch;
mod widget_presentation;
mod widget_style;

fn main() -> cosmic::iced::Result {
    // Help, version and shortcut commands never start a window.
    let mut early = std::env::args().skip(1);
    if let Some(code) = instance::early_command(early.next().as_deref(), early.next().as_deref()) {
        std::process::exit(code);
    }
    if std::env::args().nth(1).as_deref() == Some("--activity-island") {
        activity_island::choose_renderer();
        return cosmic::applet::run::<activity_island::Applet>(());
    }
    if std::env::args().nth(1).as_deref() == Some("--package-image-worker") {
        desktop::profiles::packages::assets::main();
    }
    if std::env::args().nth(1).as_deref() == Some("--calendar-parser") {
        calendar::parser_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--backdrop-worker") {
        desktop::wallpaper::media::worker::main();
    }
    if std::env::args().nth(1).as_deref() == Some("--share-worker") {
        settings::sharing::main();
    }
    if std::env::args().nth(1).as_deref() == Some("--depth-worker") {
        depth::main();
    }
    if std::env::args().nth(1).as_deref() == Some("--depth-scene-worker") {
        depth::scene_main();
    }
    // Settings from before the rename move to the new folders, once.
    paths::migrate();
    // A panic in the desktop or Settings counts as a problem (sharing on only).
    usage::counts::install_panic_hook();
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if matches!(
        command.as_deref(),
        Some(
            "--read-widget-config"
                | "--edit-widget-config"
                | "--migrate-design"
                | "--recover-design"
                | "--keep-widget-design"
                | "--export-design"
                | "--export-legacy-design"
        )
    ) {
        if let Err(error) = design::commands::run(command.as_deref().unwrap(), args) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if command.as_deref() == Some("--migrate-config") {
        return Ok(());
    }
    if matches!(
        command.as_deref(),
        Some("--select-palette" | "--set-cosmic-colors")
    ) {
        let value = args.next().unwrap_or_default();
        let request = match (command.as_deref(), value.as_str()) {
            (Some("--select-palette"), name) => Ok(system_theme::ControllerRequest::Palette(name)),
            (_, "on") => Ok(system_theme::ControllerRequest::CosmicColors(true)),
            (_, "off") => Ok(system_theme::ControllerRequest::CosmicColors(false)),
            _ => Err("Choose on or off".to_string()),
        };
        let result = request.and_then(system_theme::controller);
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if command.as_deref() == Some("--apply-cosmic-colors") {
        let result = args
            .next()
            .ok_or_else(|| "Choose a Halcyon palette".into())
            .and_then(|name| system_theme::apply(&name));
        match result {
            Ok(()) => println!("COSMIC accents matched"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if command.as_deref() == Some("--apply-palette-links") {
        let result = args
            .next()
            .ok_or_else(|| "Choose a Halcyon palette".to_string())
            .and_then(|name| {
                let mut links = system_theme::Links::default();
                for flag in args.by_ref() {
                    match flag.as_str() {
                        "--wallpaper" => links.wallpaper = true,
                        "--colors" => links.colors = true,
                        other => return Err(format!("Unknown option {other}")),
                    }
                }
                if !links.wallpaper && !links.colors {
                    return Err("Choose --wallpaper, --colors or both".into());
                }
                system_theme::apply_links(&name, links)
            });
        match result {
            Ok(()) => println!("Palette matched"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if command.as_deref() == Some("--styles") {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        for (_, name) in desktop::profiles::themes::style_choices() {
            if writeln!(out, "{name}").is_err() {
                break;
            }
        }
        return Ok(());
    }
    if command.as_deref() == Some("--style") {
        // A style as Settings › Appearance applies it: written to the
        // desktop, which the running widgets follow.
        let wanted = args.next().unwrap_or_default();
        let own_bars = args.any(|arg| arg == "--own-bars");
        let result = desktop::profiles::themes::find_style(&wanted).and_then(|id| {
            let store = desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
            let installed = desktop::profiles::Installed::discover(&store);
            let held = automation::read_hold(&store);
            Ok(automation::run::run_style(
                &store, &installed, &id, own_bars, &held,
            ))
        });
        use automation::run::Ran;
        match result {
            Ok(Ran::Applied { .. }) => println!("Style applied"),
            Ok(Ran::AlreadySet) => println!("That style is already applied"),
            Ok(Ran::Waiting(parts)) => {
                let names: Vec<&str> = parts.iter().map(|part| part.title()).collect();
                eprintln!(
                    "Settings has changes to {} that aren’t applied yet, so nothing was changed. \
                     Apply or discard them, then try again.",
                    names.join(", ")
                );
                std::process::exit(1);
            }
            Ok(Ran::Failed(error)) | Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
            Ok(Ran::KeptManual(_)) => println!("Style applied"),
        }
        return Ok(());
    }
    if command.as_deref() == Some("--restore-desktop") {
        // Stop using Halcyon's restore: the desktop from before Halcyon.
        let result = desktop::DesktopStore::from_env()
            .map_err(|e| e.to_string())
            .and_then(|store| {
                desktop::recovery::restore_everything(&store).map_err(|e| e.to_string())
            });
        match result {
            Ok(_) => println!("Desktop from before Halcyon restored"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if command.as_deref() == Some("--restore-background") {
        let result = args
            .next()
            .ok_or_else(|| "Choose the saved wallpaper file".to_string())
            .and_then(|path| system_theme::restore_background(std::path::Path::new(&path)));
        match result {
            Ok(()) => println!("Previous wallpaper restored"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    // Recover a recorded migration before any default desktop can start.
    let design_ready = design::commands::startup();
    if let Err(error) = &design_ready {
        eprintln!("Widget design: {error}");
    }
    if command.as_deref() == Some("--settings") {
        let page = args.next().unwrap_or_default();
        return run_settings(&page);
    }
    if let Some(other) = command.as_deref() {
        eprintln!("Unknown option {other}\nSee: halcyon --help");
        std::process::exit(2);
    }
    if config::Config::load().design_problem.is_some() {
        return run_settings("general");
    }
    // One widgets process per session. Kept open until the process ends.
    let _instance = match instance::acquire() {
        instance::Lock::Busy => {
            println!("{}", instance::ALREADY_RUNNING);
            return Ok(());
        }
        instance::Lock::Held(file) => Some(file),
        instance::Lock::Unavailable(error) => {
            eprintln!("Starting without the single-copy check: {error}");
            None
        }
    };
    usage::start(paths::state_dir().map(|dir| dir.join("usage.json")));
    let settings = cosmic::app::Settings::default()
        .no_main_window(true)
        .client_decorations(false)
        .transparent(true);
    cosmic::app::run::<app::Cards>(settings, config::Config::load())
}

fn run_settings(page: &str) -> cosmic::iced::Result {
    let launch = (|| -> std::io::Result<Option<std::fs::File>> {
        if let Some(status) = settings_lifecycle::independent_scope()? {
            if !status.success() {
                return Err(std::io::Error::other(format!(
                    "Settings scope exited with {status}"
                )));
            }
            return Ok(None);
        }
        let config = config::Config::path()
            .ok_or_else(|| std::io::Error::other("Configuration directory unavailable"))?;
        let request = serde_json::to_vec(&serde_json::json!({"page": page}))?;
        settings_lifecycle::prepare(&config, &request)
    })();
    match launch {
        Ok(Some(owner)) => {
            let result = settings::run(page);
            if settings::restart_requested() {
                // Let the new window take ownership, then run what's installed now.
                drop(owner);
                let error = settings_lifecycle::restart(page);
                eprintln!("Could not restart Settings: {error}");
                std::process::exit(1);
            }
            result
        }
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("Could not open Settings: {error}");
            std::process::exit(1);
        }
    }
}
