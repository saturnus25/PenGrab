use crate::{
    app::{Action, App, Check, Focus, Page, Setting, UiOptions},
    catalog::{Catalog, Language},
    search::{self, Target},
    ui,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};
use std::fs;

#[test]
fn package_plan_expands_categories_and_keeps_manual_sources() {
    let (_directory, app) = fixture();
    let entries =
        crate::operations::selected(&app.catalog, &["zaproxy".to_owned()], &["Web".to_owned()])
            .unwrap();
    let ids: Vec<_> = entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry.status))
        .collect();
    assert_eq!(
        ids,
        [
            ("zaproxy", crate::operations::Status::Planned),
            ("wfuzz", crate::operations::Status::Manual)
        ]
    );
    assert!(crate::operations::selected(&app.catalog, &[], &[]).is_err());
    assert!(crate::operations::selected(&app.catalog, &["unknown".to_owned()], &[]).is_err());
}

#[test]
fn cancelling_package_review_never_starts_the_queue() {
    let (_directory, mut app) = fixture();
    let web = app.catalog.category_index("Web");
    app.apply(Action::ToggleBranch(web));
    let selected = app.selected.clone();
    app.apply(Action::Review(crate::operations::Operation::Install));
    assert!(!app.queue.as_ref().unwrap().confirm);
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.queue.is_none());
    assert_eq!(app.selected, selected);
}

const CATEGORIES: &str = include_str!("../tests/fixtures/categories.json");
const TOOLS: &str = include_str!("../tests/fixtures/tools.json");

fn fixture() -> (tempfile::TempDir, App) {
    let project = tempfile::tempdir().unwrap();
    let data = project.path().join("data");
    fs::create_dir_all(data.join("locales")).unwrap();
    let config = project.path().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("portable"), "").unwrap();
    fs::write(data.join("categories.json"), CATEGORIES).unwrap();
    fs::write(data.join("tools.json"), TOOLS).unwrap();
    fs::write(
        data.join("locales/en.json"),
        include_str!("../data/locales/en.json"),
    )
    .unwrap();
    fs::write(
        data.join("locales/es.json"),
        include_str!("../data/locales/es.json"),
    )
    .unwrap();
    (
        project,
        App::new(
            data,
            UiOptions {
                language: Some(Language::En),
                no_color: true,
                ..UiOptions::default()
            },
        )
        .unwrap(),
    )
}

#[test]
fn shipped_catalog_offers_both_nmap_sources_in_one_tool() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let mut app = App::new(
        data,
        UiOptions {
            language: Some(Language::Es),
            no_color: true,
            ..UiOptions::default()
        },
    )
    .unwrap();
    app.query = "nmap".to_owned();
    let matches = app.tools();
    assert_eq!(matches.len(), 1);
    let nmap = matches[0];
    let tool = &app.catalog.tools[nmap];
    assert_eq!(tool.id, "nmap");
    assert_eq!(tool.category, "Networking.Inventory");
    assert_eq!(
        tool.winget_install_command().as_deref(),
        Some("winget install --id Insecure.Nmap --exact --source winget")
    );
    assert_eq!(
        tool.publisher_download.as_deref(),
        Some("https://nmap.org/download.html#windows")
    );

    let networks = app.catalog.category_index("Networking");
    app.apply(Action::ToggleBranch(networks));
    assert_eq!(app.check(networks), Check::Full);
    assert_eq!(app.tools(), [nmap]);
    assert_eq!(app.selected.len(), app.catalog.branch(networks).len());
    assert!(app.selected.len() > 1);
    assert!(app.selected.contains(&nmap));

    app.apply(Action::Details(nmap));
    let mut terminal = Terminal::new(TestBackend::new(32, 16)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let display = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    for label in ["[W] Copiar comando WinGet", "[O] Descarga oficial"] {
        assert!(display.contains(label));
    }
}

#[test]
fn catalog_filters_invalid_download_urls_and_copy_commands() {
    let tool = r#"[{"id":"tool","name":"Tool","category":"Web.Http","description":{"en":"Tool"},"wingetId":"Example.Tool & extra","publisherDownload":"https://example.org/download"}]"#;
    let catalog = Catalog::from_json(CATEGORIES, tool).unwrap();
    assert!(catalog.tools[0].winget_install_command().is_none());
    for url in ["file:///C:/tool.exe", "https://", "https://example.org/a b"] {
        let invalid = tool.replace("https://example.org/download", url);
        assert!(Catalog::from_json(CATEGORIES, &invalid).is_err());
    }
}

#[test]
fn reverse_engineering_covers_the_static_suite_with_one_entry_per_tool() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let catalog = Catalog::load(&data).unwrap();
    let root = catalog.category_index("ReverseEngineering");
    for id in [
        "detect-it-easy",
        "pe-bear",
        "ghidra",
        "cutter",
        "ilspy",
        "dnspyex",
        "recaf",
        "vineflower",
        "cfr",
        "jadx",
        "pycdc",
        "pycdas",
    ] {
        let entries: Vec<_> = catalog
            .branch(root)
            .iter()
            .filter(|&&index| catalog.tools[index].id == id)
            .collect();
        assert_eq!(entries.len(), 1, "missing or duplicated tool {id}");
        assert!(catalog.tools[*entries[0]].publisher_url().is_some());
    }
}

#[test]
fn global_search_ranks_aliases_typos_and_accent_insensitive_categories() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let catalog = Catalog::load(&data).unwrap();
    for (query, title) in [
        ("die", "Detect It Easy"),
        ("binja", "Binary Ninja Free"),
        ("gihdra", "Ghidra"),
        ("re", "Reverse engineering"),
        ("C#", ".NET assemblies"),
        ("C++", "Native binaries"),
        ("Skylot.jadx", "JADX"),
        ("packet capture", "Wireshark"),
    ] {
        let hits = search::find(&catalog, Language::En, query);
        assert_eq!(
            hits.first().map(|hit| hit.title.as_str()),
            Some(title),
            "{query}"
        );
    }
    let results = search::find(&catalog, Language::Es, "ingenieria inversa");
    let result = &results[0];
    assert_eq!(result.title, "Ingeniería inversa");
    assert_eq!(
        result
            .highlights
            .iter()
            .map(|range| &result.title[range.clone()])
            .collect::<Vec<_>>(),
        ["Ingeniería", "inversa"]
    );
}

#[test]
fn clicking_search_result_reveals_collapsed_branch_and_keeps_selections() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let mut app = App::new(
        data,
        UiOptions {
            language: Some(Language::En),
            ..UiOptions::default()
        },
    )
    .unwrap();
    let root = app.catalog.category_index("ReverseEngineering").unwrap();
    let native = app
        .catalog
        .category_index("ReverseEngineering.Native")
        .unwrap();
    app.apply(Action::Expand(root));
    app.apply(Action::ToggleBranch(
        app.catalog.category_index("Networking"),
    ));
    let old_selection = app.selected.clone();
    app.apply(Action::SelectedOnly);
    app.apply(Action::Search);
    for letter in "gihdra".chars() {
        app.key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
    }
    let index = app
        .catalog
        .tools
        .iter()
        .position(|tool| tool.id == "ghidra")
        .unwrap();
    assert_eq!(app.search_matches[0].target, Target::Tool(index));

    let mut terminal = Terminal::new(TestBackend::new(32, 16)).unwrap();
    let mut hit = None;
    terminal
        .draw(|frame| hit = Some(ui::draw(frame, &mut app)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let locate = |needle: &str| {
        (0..buffer.area.height)
            .find_map(|row| {
                let line: String = (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect();
                line.find(needle)
                    .map(|offset| (line[..offset].chars().count() as u16, row))
            })
            .unwrap()
    };
    let typed = locate("gihdra");
    let (column, row) = locate("Ghidra");
    let query_color = buffer[typed].fg;
    assert_ne!(query_color, ratatui::style::Color::Reset);
    assert_eq!(query_color, buffer[(column, row)].fg);
    assert!(hit.unwrap().event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    ));
    assert!(!app.editing);
    assert!(!app.collapsed.contains(&root));
    assert!(!app.selected_only);
    assert_eq!(app.selected, old_selection);
    assert_eq!(app.category, Some(native));
    assert_eq!(app.tool, Some(index));
    assert_eq!(app.focus, Focus::Tools);
    assert!(app.highlighted(Target::Tool(index)));
}

#[test]
fn parent_selection_includes_children_even_with_a_search_filter() {
    let (_project, mut app) = fixture();
    let web = app.catalog.category_index("Web");
    let http = app.catalog.category_index("Web.Http");
    let tests = app.catalog.category_index("Web.Testing");
    app.apply(Action::ToggleBranch(http));
    assert_eq!(app.check(http), Check::Full);
    assert_eq!(app.check(web), Check::Partial);
    assert_eq!(app.check(tests), Check::Empty);

    app.query = "zap".to_owned();
    app.apply(Action::ToggleBranch(web));
    assert_eq!(app.tools().len(), 1);
    assert_eq!(app.selected.len(), 2);
    assert_eq!(app.check(web), Check::Full);

    app.apply(Action::ToggleBranch(web));
    assert!(app.selected.is_empty());
}

#[test]
fn reload_keeps_selections_by_id_and_preserves_catalog_if_edit_is_invalid() {
    let (_project, mut app) = fixture();
    let http = app.catalog.category_index("Web.Http");
    app.apply(Action::ToggleBranch(http));
    let mut tools = serde_json::from_str::<Vec<serde_json::Value>>(TOOLS).unwrap();
    tools.reverse();
    fs::write(
        app.directory.join("tools.json"),
        serde_json::to_string(&tools).unwrap(),
    )
    .unwrap();
    app.apply(Action::Reload);
    assert_eq!(app.selected.len(), 1);
    let selected = *app.selected.iter().next().unwrap();
    assert_eq!(app.catalog.tools[selected].id, "zaproxy");
    assert_eq!(app.check(http), Check::Full);

    tools[0]["category"] = "typo".into();
    fs::write(
        app.directory.join("tools.json"),
        serde_json::to_string(&tools).unwrap(),
    )
    .unwrap();
    app.apply(Action::Reload);
    assert_eq!(app.selected.len(), 1);
    assert_eq!(app.catalog.tools[selected].id, "zaproxy");
    assert!(app.notice.unwrap().contains("typo"));
}

#[test]
fn categories_support_deeper_nesting_and_reject_unknown_references() {
    let categories = r#"[{"id":"web","name":{"en":"Web"},"children":[{"id":"http","name":{"en":"HTTP"},"children":[{"id":"capture","name":{"en":"Capture"}}]}]}]"#;
    let tool = r#"[{"id":"example","name":"Example","category":"capture","description":{"en":"Example"}}]"#;
    let catalog = Catalog::from_json(categories, tool).unwrap();
    assert_eq!(catalog.branch(catalog.category_index("web")), &[0]);
    assert_eq!(catalog.branch(catalog.category_index("http")), &[0]);
    assert_eq!(
        catalog.category_line(0, Language::Es),
        "Web / HTTP / Capture"
    );
    assert!(Catalog::from_json(categories, &tool.replace("capture", "typo")).is_err());
}

#[test]
fn catalog_click_changes_subtree_selection_without_interfering_with_keyboard() {
    let (_project, mut app) = fixture();
    let mut terminal = Terminal::new(TestBackend::new(100, 26)).unwrap();
    let mut hits = None;
    terminal
        .draw(|frame| hits = Some(ui::draw(frame, &mut app)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let web_row = (0..buffer.area.height)
        .find(|row| {
            (0..29)
                .map(|column| buffer[(column, *row)].symbol())
                .collect::<String>()
                .contains("[ ] Web")
        })
        .unwrap();
    let column = (0..29)
        .find(|column| buffer[(*column, web_row)].symbol() == "[")
        .unwrap();
    assert!(hits.unwrap().event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: web_row,
            modifiers: KeyModifiers::NONE,
        }
    ));
    let web = app.catalog.category_index("Web");
    assert_eq!(app.category, web);
    assert_eq!(app.check(web), Check::Full);
    assert_eq!(app.selected.len(), 2);
    app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Tools);
    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.check(web), Check::Partial);
}

#[test]
fn compact_catalog_keeps_the_search_controls_and_children_in_view() {
    let (_project, mut app) = fixture();
    app.language = Language::Es;
    let mut tiny = Terminal::new(TestBackend::new(32, 16)).unwrap();
    tiny.draw(|frame| {
        ui::draw(frame, &mut app);
    })
    .unwrap();
    let display = tiny
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    for label in [
        "[Recargar]",
        "Selección",
        "Categorías",
        "4 herramientas",
        "[Salir]",
    ] {
        assert!(
            display.contains(label),
            "missing {label} in the smallest layout"
        );
    }

    app.apply(Action::Category(app.catalog.category_index("Web")));
    let mut compact = Terminal::new(TestBackend::new(58, 22)).unwrap();
    compact
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let display = compact
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(display.contains("Web"));
    assert!(display.contains("Inspección HTTP"));
    assert!(display.contains("Pruebas de solicitudes"));
}

#[test]
fn wide_category_tree_separates_roots_without_adding_a_focus_target() {
    let (_project, mut app) = fixture();
    let mut terminal = Terminal::new(TestBackend::new(113, 24)).unwrap();
    let mut hits = None;
    terminal
        .draw(|frame| hits = Some(ui::draw(frame, &mut app)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|row| {
            (3..31)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    let row = |label: &str| rows.iter().position(|text| text.contains(label)).unwrap();
    assert!(row("~ [ ] Networks") < row("Traffic analysis"));
    assert!(row("@ [ ] Web") < row("# [ ] Forensics"));
    let separator = row("@ [ ] Web") - 1;
    assert!(rows[separator].starts_with("────"));
    assert!(!hits.unwrap().event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: separator as u16,
            modifiers: KeyModifiers::NONE,
        }
    ));
    app.apply(Action::Category(
        app.catalog.category_index("Networking.Traffic"),
    ));
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.category, app.catalog.category_index("Web"));
}

#[test]
fn color_mode_draws_each_root_icon_with_a_distinct_accent() {
    let (project, _) = fixture();
    let mut app = App::new(project.path().join("data"), UiOptions::default()).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(113, 24)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let cells = terminal.backend().buffer().content();
    let colors =
        ["≈", "◎", "◇"].map(|glyph| cells.iter().find(|cell| cell.symbol() == glyph).unwrap().fg);
    assert_ne!(colors[0], colors[1]);
    assert_ne!(colors[0], colors[2]);
    assert_ne!(colors[1], colors[2]);
}

#[test]
fn active_tab_marker_and_underline_follow_navigation_in_monochrome() {
    let (_project, mut app) = fixture();
    let mut terminal = Terminal::new(TestBackend::new(104, 22)).unwrap();
    let mut hits = None;
    terminal
        .draw(|frame| hits = Some(ui::draw(frame, &mut app)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let names: String = (0..104)
        .map(|column| buffer[(column, 1)].symbol())
        .collect();
    let catalog = names[..names.find("Catalog").unwrap()].chars().count() as u16;
    let settings = names[..names.find("Settings").unwrap()].chars().count() as u16;
    assert!(names.contains("> Catalog"));
    assert_eq!(buffer[(catalog, 2)].symbol(), "━");
    assert_eq!(buffer[(settings, 2)].symbol(), "─");

    assert!(hits.unwrap().event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: settings,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }
    ));
    assert_eq!(app.page, Page::Settings);
    terminal
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let names: String = (0..104)
        .map(|column| buffer[(column, 1)].symbol())
        .collect();
    assert!(names.contains("> Settings"));
    assert_eq!(buffer[(catalog, 2)].symbol(), "─");
    assert_eq!(buffer[(settings, 2)].symbol(), "━");
}

#[test]
fn all_tools_stays_pinned_while_the_category_cursor_and_scrollbar_move() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let mut app = App::new(
        data,
        UiOptions {
            language: Some(Language::En),
            ..UiOptions::default()
        },
    )
    .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(128, 21)).unwrap();
    let locate = |buffer: &ratatui::buffer::Buffer, label: &str, columns: std::ops::Range<u16>| {
        (0..buffer.area.height)
            .find_map(|row| {
                let cells: String = columns
                    .clone()
                    .map(|column| buffer[(column, row)].symbol())
                    .collect();
                cells
                    .find(label)
                    .map(|offset| (columns.start + cells[..offset].chars().count() as u16, row))
            })
            .unwrap_or_else(|| panic!("{label} missing from {columns:?}"))
    };
    terminal
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let pinned = locate(buffer, "All tools", 0..41);
    for (label, columns) in [("Networks", 0..41), ("] Nmap", 43..90)] {
        let (column, row) = locate(buffer, label, columns.clone());
        let rail = (column..columns.end)
            .find(|&x| matches!(buffer[(x, row)].symbol(), "█" | "░"))
            .unwrap_or_else(|| panic!("no scrollbar alongside {label}"));
        assert_eq!(buffer[(rail - 1, row)].symbol(), " ", "{label}");
        assert_ne!(buffer[(rail - 2, row)].symbol(), " ", "{label}");
    }
    app.apply(Action::Category(
        app.catalog.category_index("Endpoint.Telemetry"),
    ));
    terminal
        .draw(|frame| {
            ui::draw(frame, &mut app);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(locate(buffer, "All tools", 0..41), pinned);
    let cursor = locate(buffer, "System telemetry", 0..41);
    assert!(cursor.1 > pinned.1 + 1);
    assert_ne!(buffer[cursor].bg, buffer[pinned].bg);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "█"));
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "░"));
}

#[test]
fn cancelling_a_search_restores_the_previous_tool_and_focus() {
    let (_project, mut app) = fixture();
    let previous = app
        .catalog
        .tools
        .iter()
        .position(|tool| tool.id == "exiftool")
        .unwrap();
    app.apply(Action::Tool(previous));
    app.apply(Action::Search);
    for letter in "zap".chars() {
        app.key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
    }
    assert_ne!(app.tool, Some(previous));
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.query.is_empty());
    assert_eq!(app.tool, Some(previous));
    assert_eq!(app.focus, Focus::Tools);
}

#[test]
fn terminal_text_removes_control_sequences_without_changing_normal_text() {
    let text = "Inspección HTTP\x1b[2J\u{202e}\nKali";
    assert_eq!(
        crate::terminal::clean_text(text),
        "Inspección HTTP [2J  Kali"
    );
}

#[test]
fn settings_keep_legacy_preferences_and_portable_overrides() {
    let (project, _) = fixture();
    let config = project.path().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("portable"), "").unwrap();
    let settings_file = config.join("preferences.json");
    fs::write(&settings_file, r#"{"Language":"es"}"#).unwrap();
    let data = project.path().join("data");
    let mut app = App::new(data.clone(), UiOptions::default()).unwrap();
    assert_eq!(app.language, Language::Es);
    assert!(app.mouse_enabled());
    assert!(app.package_badges());
    assert!(!app.monochrome());

    app.apply(Action::Page(Page::Settings));
    let press = |app: &mut App, key| app.key(KeyEvent::new(key, KeyModifiers::NONE));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    app.apply(Action::ChangeSetting(Setting::ReducedMotion));
    let web = app.catalog.category_index("Web");
    let current = app.catalog.category_index("Web.Http");
    app.apply(Action::ToggleBranch(current));
    let selected = app.selected.clone();
    app.apply(Action::ChangeSetting(Setting::CollapseOnStartup));
    assert_eq!(app.category, current);
    assert_eq!(app.selected, selected);
    assert!(!app.collapsed.contains(&web.unwrap()));
    assert!(
        app.collapsed
            .contains(&app.catalog.category_index("Networking").unwrap())
    );
    app.apply(Action::ChangeSetting(Setting::CollapseOnStartup));
    assert!(app.collapsed.is_empty());
    app.apply(Action::ChangeSetting(Setting::CollapseOnStartup));
    app.apply(Action::ChangeSetting(Setting::PackageBadges));

    let reopened = App::new(data.clone(), UiOptions::default()).unwrap();
    assert_eq!(reopened.language, Language::Es);
    assert!(reopened.monochrome());
    assert!(!reopened.mouse_enabled());
    assert!(reopened.reduced_motion());
    assert!(!reopened.package_badges());
    assert!(
        reopened
            .collapsed
            .contains(&reopened.catalog.category_index("Web").unwrap())
    );

    app.apply(Action::ChangeSetting(Setting::Theme));
    app.apply(Action::ChangeSetting(Setting::Mouse));
    let saved = fs::read(&settings_file).unwrap();
    let mut overridden = App::new(
        data,
        UiOptions {
            language: Some(Language::En),
            no_color: true,
            no_mouse: true,
        },
    )
    .unwrap();
    overridden.apply(Action::ChangeSetting(Setting::Theme));
    overridden.apply(Action::ChangeSetting(Setting::Mouse));
    assert_eq!(overridden.language, Language::En);
    assert!(overridden.monochrome());
    assert!(!overridden.mouse_enabled());
    assert_eq!(fs::read(settings_file).unwrap(), saved);
}

#[test]
fn compact_help_scrolls_and_short_wide_settings_keep_keyboard_focus_visible() {
    let (project, _) = fixture();
    let mut app = App::new(
        project.path().join("data"),
        UiOptions {
            language: Some(Language::Es),
            ..UiOptions::default()
        },
    )
    .unwrap();
    app.apply(Action::Page(Page::Help));
    let mut tiny = Terminal::new(TestBackend::new(32, 16)).unwrap();
    let draw = |terminal: &mut Terminal<TestBackend>, app: &mut App| {
        let mut hit = None;
        terminal
            .draw(|frame| hit = Some(ui::draw(frame, app)))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        (text, hit.unwrap())
    };
    let (first, _) = draw(&mut tiny, &mut app);
    assert!(first.contains("PenGrab ✎"));
    assert!(first.contains("Explorar y seleccionar"));
    assert!(!first.contains("Revisar versión PenGrab"));
    app.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let (last, hit) = draw(&mut tiny, &mut app);
    assert!(last.contains("Revisar versión PenGrab"));
    let before = app.help_offset;
    assert!(hit.event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 8,
            row: 6,
            modifiers: KeyModifiers::NONE
        }
    ));
    assert_eq!(app.help_offset, before - 1);

    app.apply(Action::Page(Page::Settings));
    let mut short = Terminal::new(TestBackend::new(100, 12)).unwrap();
    app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let (text, hit) = draw(&mut short, &mut app);
    assert!(text.contains("> Estado de paquetes"));
    assert!(text.contains("█"));
    let row = (0..12)
        .find(|&y| {
            short.backend().buffer().content[(y as usize * 100)..((y as usize + 1) * 100)]
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .contains("> Estado de paquetes")
        })
        .unwrap();
    assert!(hit.event(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 9,
            row,
            modifiers: KeyModifiers::NONE
        }
    ));
    assert!(app.package_badges());
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!app.package_badges());
}

#[test]
fn settings_preview_follows_focus_and_applies_only_the_requested_value() {
    let (project, _) = fixture();
    let mut app = App::new(project.path().join("data"), UiOptions::default()).unwrap();
    app.apply(Action::Page(Page::Settings));
    let mut terminal = Terminal::new(TestBackend::new(128, 24)).unwrap();
    let redraw = |terminal: &mut Terminal<TestBackend>, app: &mut App| {
        let mut hit = None;
        terminal
            .draw(|frame| hit = Some(ui::draw(frame, app)))
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(128)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect();
        (hit.unwrap(), rows)
    };
    let locate = |rows: &[String], text: &str| {
        rows.iter()
            .enumerate()
            .find_map(|(y, row)| {
                row.find(text)
                    .map(|byte| (row[..byte].chars().count() as u16, y as u16))
            })
            .unwrap_or_else(|| panic!("missing {text}\n{}", rows.join("\n")))
    };
    let click = |hit: &ui::HitMap, app: &mut App, (column, row)| {
        assert!(hit.event(
            app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }
        ));
    };

    let (hit, rows) = redraw(&mut terminal, &mut app);
    let text = rows.join("\n");
    assert!(text.contains("Current") && text.contains("Alternative"));
    assert!(text.contains("Categories │ Tools") && text.contains("Categorías │ Herramientas"));
    click(&hit, &mut app, locate(&rows, "Appearance"));
    assert!(app.focused_setting() == Setting::Theme);
    assert!(!app.monochrome());
    let (hit, rows) = redraw(&mut terminal, &mut app);
    let (first, y) = locate(&rows, "> [ ] Wireshark");
    let rest: String = rows[y as usize].chars().skip(first as usize + 1).collect();
    let second = first
        + 1
        + rest[..rest.find("> [ ] Wireshark").unwrap()]
            .chars()
            .count() as u16;
    let buffer = terminal.backend().buffer();
    assert_ne!(buffer[(first, y)].bg, buffer[(second, y)].bg);
    assert_eq!(buffer[(second, y)].bg, ratatui::style::Color::Reset);
    click(&hit, &mut app, locate(&rows, "[Enter] Use alternative"));
    assert!(app.monochrome());
    assert_eq!(app.notice.as_deref(), Some("Applied and saved"));
    let (_, rows) = redraw(&mut terminal, &mut app);
    assert!(rows.join("\n").contains("Applied and saved"));
    let (first, y) = locate(&rows, "> [ ] Wireshark");
    assert_eq!(
        terminal.backend().buffer()[(first, y)].bg,
        ratatui::style::Color::Reset
    );
}
