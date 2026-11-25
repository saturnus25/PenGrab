mod app;
mod catalog;
mod operations;
mod packages;
mod search;
mod selfupdate;
mod terminal;
#[cfg(test)]
mod tests;
mod ui;

use app::{App, UiOptions};
use catalog::{AppResult, Language};
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    io::{self, IsTerminal},
    path::PathBuf,
    process::ExitCode,
    sync::atomic::AtomicBool,
    time::Duration,
};

#[derive(Default)]
struct Options {
    data: Option<PathBuf>,
    language: Option<Language>,
    no_color: bool,
    no_mouse: bool,
    catalog: bool,
    catalog_json: bool,
    apply: Option<operations::Operation>,
    tools: Vec<String>,
    categories: Vec<String>,
    location: Option<PathBuf>,
    non_interactive: bool,
    dry_run: bool,
    json: bool,
    diagnostics: bool,
    help: bool,
}

impl Options {
    fn parse() -> AppResult<Self> {
        let mut options = Self::default();
        let mut arguments = env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--data-dir") => {
                    options.data = Some(arguments.next().ok_or("--data-dir needs a path")?.into());
                }
                Some("--lang") => {
                    options.language =
                        match arguments.next().as_deref().and_then(|code| code.to_str()) {
                            Some("en") => Some(Language::En),
                            Some("es") => Some(Language::Es),
                            _ => return Err("--lang needs en or es".into()),
                        }
                }
                Some("--no-color") => options.no_color = true,
                Some("--no-mouse") => options.no_mouse = true,
                Some("--catalog") => options.catalog = true,
                Some("--catalog-json") => options.catalog_json = true,
                Some("--apply") => {
                    options.apply = match arguments.next().as_deref().and_then(|verb| verb.to_str())
                    {
                        Some("install") => Some(operations::Operation::Install),
                        Some("upgrade") => Some(operations::Operation::Upgrade),
                        _ => return Err("--apply needs install or upgrade".into()),
                    };
                }
                Some("--tool" | "--category") => {
                    let value = arguments
                        .next()
                        .ok_or("A catalog ID is required")?
                        .into_string()
                        .map_err(|_| "Catalog IDs must be text")?;
                    if argument == "--tool" {
                        options.tools.push(value);
                    } else {
                        options.categories.push(value);
                    }
                }
                Some("--location") => {
                    let path = arguments.next().ok_or("--location needs a path")?;
                    options.location = Some(operations::Settings::location(path.into())?);
                }
                Some("--non-interactive") => options.non_interactive = true,
                Some("--dry-run") => options.dry_run = true,
                Some("--json") => options.json = true,
                Some("--diagnostics") => options.diagnostics = true,
                Some("--help" | "-h") => options.help = true,
                _ => return Err(format!("Unknown option: {}", argument.to_string_lossy()).into()),
            }
        }
        let modes = [
            options.catalog,
            options.catalog_json,
            options.diagnostics,
            options.apply.is_some(),
        ];
        if modes.iter().filter(|mode| **mode).count() > 1 {
            return Err("Choose one mode.".into());
        }
        if options.apply.is_none()
            && (!options.tools.is_empty()
                || !options.categories.is_empty()
                || options.location.is_some()
                || options.non_interactive
                || options.dry_run
                || options.json)
        {
            return Err("Package operation options require --apply.".into());
        }
        Ok(options)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let message = terminal::clean_text(&error.to_string());
            if env::args_os().any(|argument| argument == "--json" || argument == "--catalog-json") {
                println!(
                    "{}",
                    serde_json::json!({"schemaVersion":1,"outcome":"error","message":message})
                );
            } else {
                eprintln!("PenGrab: {message}");
            }
            ExitCode::from(2)
        }
    }
}

fn run() -> AppResult<u8> {
    let options = Options::parse()?;
    if options.help {
        println!(
            "PenGrab {}\n\n\
             Usage: pengrab [--catalog | --diagnostics] [options]\n\n\
             --catalog       Print the tool catalog.\n\
             --catalog-json  Print category and tool IDs as JSON.\n\
             --apply install|upgrade --tool ID or --category ID\n\
             --dry-run       Show a plan without running package operations.\n\
             --non-interactive Use user-scope portable packages only.\n\
             --location PATH Request an absolute installer destination.\n\
             --json          Print the package report as JSON.\n\
             --diagnostics   Check WinGet and print the result.\n\
             --lang en|es    Choose the interface language.\n\
             --no-color      Use the terminal's default colors.\n\
             --no-mouse      Keep the terminal's mouse behavior.\n\
             --data-dir PATH Read the catalog and translations from PATH.",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(0);
    }

    let directory = match options.data.as_ref() {
        Some(directory) => directory.clone(),
        None => find_data()?,
    };
    let no_color = options.no_color || env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut app = App::new(
        directory,
        UiOptions {
            language: options.language,
            no_color,
            no_mouse: options.no_mouse,
        },
    )?;
    if let Some(operation) = options.apply {
        if !options.dry_run && !options.non_interactive {
            return Err("Use --dry-run to review the plan, --non-interactive to apply it, or the TUI for an interactive review.".into());
        }
        let settings = operations::Settings {
            operation,
            unattended: options.non_interactive,
            location: options.location,
        };
        let mut entries = operations::selected(&app.catalog, &options.tools, &options.categories)?;
        if !options.dry_run {
            operations::run(&settings, &mut entries, &AtomicBool::new(false), |_, _| {});
        }
        let complete = entries.iter().all(|entry| entry.status.complete());
        let outcome = if options.dry_run {
            "planned"
        } else if complete {
            "complete"
        } else {
            "incomplete"
        };
        if options.json {
            println!(
                "{}",
                serde_json::json!({
                    "schemaVersion":1,"outcome":outcome,"operation":operation,
                    "unattended":settings.unattended,"location":settings.location,"items":entries,
                })
            );
        } else {
            for entry in entries {
                println!(
                    "{} | {} | {}",
                    terminal::clean_text(&entry.name),
                    app.t(entry.status.label()),
                    entry.version.or(entry.detail).as_deref().unwrap_or("")
                );
            }
        }
        return Ok(if options.dry_run || complete { 0 } else { 6 });
    } else if options.catalog_json {
        let categories: Vec<_> = app
            .catalog
            .categories
            .iter()
            .map(|category| {
                serde_json::json!({
                    "id":category.id,"name":category.name.get(app.language),
                    "parent":category.parent.map(|parent| &app.catalog.categories[parent].id)
                })
            })
            .collect();
        let tools: Vec<_> = app
            .catalog
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "id":tool.id,"name":tool.name,"category":tool.category,
                    "wingetId":tool.winget_id,"publisherUrl":tool.publisher_url()
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"categories":categories,"tools":tools})
        );
    } else if options.catalog {
        for (index, tool) in app.catalog.tools.iter().enumerate() {
            let package = tool
                .winget_id
                .as_deref()
                .or_else(|| tool.publisher_url())
                .unwrap_or_else(|| app.t("NoWinGetPackage"));
            println!(
                "{} | {} | {}",
                terminal::clean_text(&tool.name),
                terminal::clean_text(&app.catalog.category_line(index, app.language)),
                terminal::clean_text(package)
            );
        }
    } else if options.diagnostics {
        app.winget = Some(packages::version(&std::sync::atomic::AtomicBool::new(
            false,
        )));
        println!("{}: {}", app.t("Architecture"), env::consts::ARCH);
        println!("{}", ui::winget_status(&app));
    } else if !io::stdin().is_terminal()
        || !io::stdout().is_terminal()
        || env::var_os("TERM").is_some_and(|term| term == "dumb")
    {
        println!("{}", app.t("RustRedirected"));
    } else {
        run_tui(&mut app)?;
    }
    Ok(0)
}

fn find_data() -> AppResult<PathBuf> {
    let executable = env::current_exe()?;
    let data = executable
        .parent()
        .into_iter()
        .flat_map(|folder| folder.ancestors().take(4))
        .map(|folder| folder.join("data"))
        .find(|data| data.join("categories.json").is_file());
    data.ok_or_else(|| {
        "data/categories.json was not found next to the application. Use --data-dir to locate it."
            .into()
    })
}

fn run_tui(app: &mut App) -> io::Result<()> {
    let mut mouse = app.mouse_enabled();
    let mut dark = !app.monochrome();
    let mut console = terminal::Session::enter(mouse, dark)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    app.start_probe();
    app.release_update = selfupdate::ReleaseUpdate::start();
    let mut hit = ui::HitMap::default();
    let mut dirty = true;
    let mut size = terminal.size()?;
    while !app.quit {
        dirty |= app.poll_probe();
        dirty |= app.poll_highlight();
        dirty |= app.poll_queue();
        dirty |= app
            .release_update
            .as_mut()
            .is_some_and(selfupdate::ReleaseUpdate::poll);
        let current_size = terminal.size()?;
        if current_size != size {
            size = current_size;
            dirty = true;
        }
        if mouse != app.mouse_enabled() {
            mouse = app.mouse_enabled();
            if console.set_mouse(mouse).is_err() {
                app.notice = Some(app.t("SettingsMouseUnavailable").to_owned());
            }
        }
        if dark == app.monochrome() {
            dark = !app.monochrome();
            console.set_background(dark)?;
            terminal.clear()?;
        }
        if dirty {
            terminal.draw(|frame| {
                hit = ui::draw(frame, app);
            })?;
            dirty = false;
        }
        if !event::poll(Duration::from_millis(75))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                app.key(key);
                dirty = true;
            }
            Event::Mouse(event) if mouse => dirty |= hit.event(app, event),
            Event::Resize(_, _) => dirty = true,
            _ => {}
        }
    }
    Ok(())
}
