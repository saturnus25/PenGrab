use crate::{
    catalog::{AppResult, Catalog, Language, Tool, search_text},
    operations::{self, Operation, Phase, Queue},
    packages::{CheckResult, Inventory, Scan, WinGet},
    search::{self, Target},
    selfupdate::{ReleaseUpdate, State},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, mpsc::TryRecvError},
    time::Instant,
};

pub type Node = Option<usize>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Catalog,
    Diagnostics,
    Settings,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Categories,
    Tools,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Empty,
    Partial,
    Full,
}

#[derive(Default)]
pub struct UiOptions {
    pub language: Option<Language>,
    pub no_color: bool,
    pub no_mouse: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Language,
    Theme,
    Mouse,
    ReducedMotion,
    CollapseOnStartup,
    PackageBadges,
}

impl Setting {
    pub const ALL: [Self; 6] = [
        Self::Language,
        Self::Theme,
        Self::Mouse,
        Self::ReducedMotion,
        Self::CollapseOnStartup,
        Self::PackageBadges,
    ];
}

#[derive(Clone, Copy)]
pub enum Action {
    Page(Page),
    Category(Node),
    ToggleBranch(Node),
    Expand(usize),
    Tool(usize),
    ToggleTool(usize),
    Details(usize),
    CloseDetails,
    CopyWinGet,
    OpenPublisher,
    Search,
    CancelSearch,
    SearchJump(Target),
    SelectedOnly,
    Reload,
    FocusSetting(Setting),
    ChangeSetting(Setting),
    HelpScroll(isize),
    Diagnose,
    Review(Operation),
    QueueConfirm,
    QueueClose,
    QueueScroll(isize),
    ReviewSelfUpdate,
    ApplySelfUpdate,
    CloseSelfUpdate,
    Exit,
}

struct Highlight {
    target: Target,
    started: Instant,
    visible: bool,
}

pub struct App {
    pub catalog: Catalog,
    pub directory: PathBuf,
    pub language: Language,
    pub settings_cursor: usize,
    pub help_offset: usize,
    pub page: Page,
    pub focus: Focus,
    pub category: Node,
    pub tool: Option<usize>,
    pub details_open: bool,
    pub detail_scroll: u16,
    pub query: String,
    pub editing: bool,
    pub search_matches: Vec<search::Match>,
    pub search_cursor: usize,
    pub selected_only: bool,
    pub selected: HashSet<usize>,
    pub collapsed: HashSet<usize>,
    pub category_offset: usize,
    pub tool_offset: usize,
    pub notice: Option<String>,
    pub winget: Option<WinGet>,
    pub inventory: Inventory,
    pub diagnostic_offset: usize,
    pub queue: Option<Queue>,
    pub release_update: Option<ReleaseUpdate>,
    pub release_dialog: bool,
    pub quit: bool,
    preferences: Preferences,
    options: UiOptions,
    previous_query: String,
    previous_focus: Focus,
    previous_tool: Option<usize>,
    highlight: Option<Highlight>,
    probe: Option<Scan>,
    en: HashMap<String, String>,
    es: HashMap<String, String>,
}

impl App {
    pub fn new(directory: PathBuf, options: UiOptions) -> AppResult<Self> {
        let load_locale = |locale: &str| -> AppResult<HashMap<String, String>> {
            let path = directory.join("locales").join(format!("{locale}.json"));
            let contents = fs::read_to_string(&path)?;
            Ok(
                serde_json::from_str(contents.trim_start_matches('\u{feff}'))
                    .map_err(|e| format!("{}: {e}", path.display()))?,
            )
        };
        let preferences = Preferences::load(&directory);
        let language = options.language.unwrap_or(preferences.language);
        let catalog = Catalog::load(&directory)?;
        let collapsed = if preferences.collapse_on_startup {
            catalog.collapsed_branches()
        } else {
            HashSet::new()
        };
        let mut app = Self {
            catalog,
            en: load_locale("en")?,
            es: load_locale("es")?,
            language,
            settings_cursor: 0,
            help_offset: 0,
            directory,
            page: Page::Catalog,
            focus: Focus::Categories,
            category: None,
            tool: None,
            details_open: false,
            detail_scroll: 0,
            query: String::new(),
            editing: false,
            search_matches: Vec::new(),
            search_cursor: 0,
            selected_only: false,
            selected: HashSet::new(),
            collapsed,
            category_offset: 0,
            tool_offset: 0,
            notice: None,
            winget: None,
            inventory: Inventory::default(),
            diagnostic_offset: 0,
            queue: None,
            release_update: None,
            release_dialog: false,
            quit: false,
            preferences,
            options,
            previous_query: String::new(),
            previous_focus: Focus::Categories,
            previous_tool: None,
            highlight: None,
            probe: None,
        };
        app.reconcile_tool();
        Ok(app)
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.t_in(self.language, key)
    }

    pub fn t_in<'a>(&'a self, language: Language, key: &'a str) -> &'a str {
        if language == Language::Es
            && let Some(value) = self.es.get(key)
        {
            return value;
        }
        self.en.get(key).map_or(key, String::as_str)
    }

    pub fn publisher_label(&self, tool: &Tool) -> Option<&str> {
        if tool.publisher_download.is_some() {
            Some(self.t("OfficialDownload"))
        } else {
            tool.publisher_project
                .as_ref()
                .map(|_| self.t("OfficialProject"))
        }
    }

    pub fn tools(&self) -> Vec<usize> {
        let query = search_text(&self.query);
        let words: Vec<&str> = query.split_whitespace().collect();
        self.catalog
            .branch(self.category)
            .iter()
            .copied()
            .filter(|index| !self.selected_only || self.selected.contains(index))
            .filter(|index| self.catalog.contains_words(*index, &words, self.language))
            .collect()
    }

    pub fn check(&self, node: Node) -> Check {
        let branch = self.catalog.branch(node);
        let checked = branch
            .iter()
            .filter(|index| self.selected.contains(index))
            .count();
        match checked {
            0 => Check::Empty,
            n if n == branch.len() => Check::Full,
            _ => Check::Partial,
        }
    }

    pub fn apply(&mut self, action: Action) {
        self.notice = None;
        self.highlight = None;
        match action {
            Action::Page(page) => {
                if self.editing {
                    self.finish_search(false);
                }
                self.page = page;
                if page == Page::Diagnostics && self.winget.is_none() {
                    self.start_probe();
                }
            }
            Action::Category(node) => {
                self.finish_search(false);
                self.focus = Focus::Categories;
                if self.category != node {
                    self.category = node;
                    self.tool_offset = 0;
                    self.reconcile_tool();
                }
            }
            Action::ToggleBranch(node) => {
                self.apply(Action::Category(node));
                if self.check(node) == Check::Full {
                    for index in self.catalog.branch(node) {
                        self.selected.remove(index);
                    }
                } else {
                    self.selected.extend(self.catalog.branch(node));
                }
                self.reconcile_tool();
            }
            Action::Expand(index) => {
                self.apply(Action::Category(Some(index)));
                if !self.catalog.categories[index].children.is_empty()
                    && !self.collapsed.remove(&index)
                {
                    self.collapsed.insert(index);
                }
            }
            Action::Tool(index) => {
                self.finish_search(false);
                self.focus = Focus::Tools;
                self.tool = Some(index);
            }
            Action::ToggleTool(index) => {
                self.apply(Action::Tool(index));
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                }
                self.reconcile_tool();
            }
            Action::Details(index) => {
                self.apply(Action::Tool(index));
                self.detail_scroll = 0;
                self.details_open = true;
            }
            Action::CloseDetails => self.details_open = false,
            Action::CopyWinGet | Action::OpenPublisher => {
                if self.details_open
                    && let Some(tool) = self.tool.map(|index| &self.catalog.tools[index])
                    && let Some(url) = tool.publisher_url()
                {
                    let (result, confirmation) = match action {
                        Action::CopyWinGet => {
                            let Some(command) = tool.winget_install_command() else {
                                return;
                            };
                            (copy_to_clipboard(&command), "WinGetCopied")
                        }
                        Action::OpenPublisher => (open_download(url), "PublisherOpened"),
                        _ => unreachable!(),
                    };
                    self.notice = Some(
                        match result {
                            Ok(()) => self.t(confirmation),
                            Err(_) => self.t("SourceActionFailed"),
                        }
                        .to_owned(),
                    );
                }
            }
            Action::Search => {
                if !self.editing {
                    self.previous_query.clone_from(&self.query);
                    self.previous_focus = self.focus;
                    self.previous_tool = self.tool;
                    self.focus = Focus::Search;
                    self.editing = true;
                    self.refresh_matches();
                }
            }
            Action::CancelSearch => self.finish_search(true),
            Action::SearchJump(target) if self.editing => self.jump_to(target),
            Action::SearchJump(_) => {}
            Action::SelectedOnly => {
                self.finish_search(false);
                self.selected_only = !self.selected_only;
                self.tool_offset = 0;
                self.reconcile_tool();
            }
            Action::Reload => self.reload(),
            Action::FocusSetting(setting) => self.focus_setting(setting),
            Action::ChangeSetting(setting) => self.change_setting(setting),
            Action::HelpScroll(delta) => {
                self.help_offset = self.help_offset.saturating_add_signed(delta);
            }
            Action::Diagnose => self.start_probe(),
            Action::Review(operation) => self.review(operation),
            Action::QueueConfirm => {
                if let Some(queue) = &mut self.queue {
                    queue.start();
                }
            }
            Action::QueueClose => {
                if let Some(queue) = &self.queue
                    && queue.phase == Phase::Running
                {
                    queue.stop();
                } else {
                    self.queue = None;
                }
            }
            Action::QueueScroll(delta) => {
                if let Some(queue) = &mut self.queue {
                    queue.offset = queue
                        .offset
                        .saturating_add_signed(delta)
                        .min(queue.entries.len());
                }
            }
            Action::ReviewSelfUpdate => {
                if let Some(update) = &mut self.release_update
                    && matches!(update.state, State::Available(_))
                {
                    update.confirm = false;
                    self.release_dialog = true;
                }
            }
            Action::ApplySelfUpdate => {
                if let Some(update) = &self.release_update
                    && matches!(update.state, State::Available(_))
                {
                    if update.apply().is_ok() {
                        self.quit = true;
                    } else {
                        self.release_dialog = false;
                        self.notice = Some(self.t("SelfUpdateStartFailed").to_owned());
                    }
                }
            }
            Action::CloseSelfUpdate => self.release_dialog = false,
            Action::Exit => self.quit = true,
        }
    }

    pub fn step(&mut self, focus: Focus, delta: isize) {
        match focus {
            Focus::Categories => {
                let rows = self.catalog.visible_categories(&self.collapsed);
                let current = rows
                    .iter()
                    .position(|node| *node == self.category)
                    .unwrap_or(0);
                let next = current.saturating_add_signed(delta).min(rows.len() - 1);
                self.apply(Action::Category(rows[next]));
            }
            Focus::Tools => {
                let tools = self.tools();
                let current = self
                    .tool
                    .and_then(|tool| tools.iter().position(|index| *index == tool));
                if let Some(index) = current {
                    let next = index.saturating_add_signed(delta).min(tools.len() - 1);
                    self.apply(Action::Tool(tools[next]));
                } else {
                    self.focus = Focus::Tools;
                }
            }
            Focus::Search => {}
        }
    }

    pub fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.release_dialog
            && let Some(update) = &mut self.release_update
        {
            match key.code {
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    update.confirm = !update.confirm
                }
                KeyCode::Enter if update.confirm => self.apply(Action::ApplySelfUpdate),
                KeyCode::Esc | KeyCode::Enter => self.apply(Action::CloseSelfUpdate),
                _ => {}
            }
            return;
        }
        if self.queue.is_some() {
            self.queue_key(key, ctrl);
            return;
        }
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.details_open {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.apply(Action::CloseDetails),
                KeyCode::Up => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                KeyCode::Down => self.detail_scroll = self.detail_scroll.saturating_add(1),
                KeyCode::Char('w' | 'W') if !ctrl && key.kind == KeyEventKind::Press => {
                    self.apply(Action::CopyWinGet);
                }
                KeyCode::Char('o' | 'O') if !ctrl && key.kind == KeyEventKind::Press => {
                    self.apply(Action::OpenPublisher);
                }
                _ => {}
            }
            return;
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.apply(Action::CancelSearch);
                    return;
                }
                KeyCode::Enter => {
                    if let Some(hit) = self.search_matches.get(self.search_cursor) {
                        self.apply(Action::SearchJump(hit.target));
                    }
                    return;
                }
                KeyCode::Tab => {
                    self.finish_search(false);
                    self.focus = Focus::Tools;
                    return;
                }
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                    let delta = match key.code {
                        KeyCode::Up => -1,
                        KeyCode::Down => 1,
                        KeyCode::PageUp => -7,
                        _ => 7,
                    };
                    self.move_search(delta);
                    return;
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char('u') if ctrl => self.query.clear(),
                KeyCode::Char(letter)
                    if !ctrl && !letter.is_control() && self.query.len() < 320 =>
                {
                    self.query.push(letter);
                }
                _ => return,
            }
            self.refresh_matches();
            self.reconcile_tool();
            return;
        }
        match key.code {
            KeyCode::Esc if self.page != Page::Catalog => self.apply(Action::Page(Page::Catalog)),
            KeyCode::Esc | KeyCode::Char('q') => self.quit = true,
            KeyCode::F(1) | KeyCode::Char('?') => self.apply(Action::Page(Page::Help)),
            KeyCode::F(2) => self.apply(Action::Page(Page::Catalog)),
            KeyCode::F(3) => self.apply(Action::Page(Page::Diagnostics)),
            KeyCode::F(4) => self.apply(Action::Page(Page::Settings)),
            KeyCode::F(8) if self.page == Page::Diagnostics => self.apply(Action::ReviewSelfUpdate),
            KeyCode::F(6) if self.page == Page::Catalog => {
                self.apply(Action::Review(Operation::Install))
            }
            KeyCode::F(7) if matches!(self.page, Page::Catalog | Page::Diagnostics) => {
                self.apply(Action::Review(Operation::Upgrade));
            }
            KeyCode::Enter if self.page == Page::Diagnostics => self.apply(Action::Diagnose),
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                if self.page == Page::Diagnostics =>
            {
                let delta = match key.code {
                    KeyCode::Up => -1,
                    KeyCode::Down => 1,
                    KeyCode::PageUp => -8,
                    _ => 8,
                };
                self.scroll_diagnostics(delta);
            }
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                if self.page == Page::Help =>
            {
                let delta = match key.code {
                    KeyCode::Up => -1,
                    KeyCode::Down => 1,
                    KeyCode::PageUp => -8,
                    _ => 8,
                };
                self.apply(Action::HelpScroll(delta));
            }
            KeyCode::Home if self.page == Page::Help => self.help_offset = 0,
            KeyCode::End if self.page == Page::Help => self.help_offset = usize::MAX,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right
                if self.page == Page::Settings =>
            {
                self.apply(Action::ChangeSetting(self.focused_setting()));
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab
                if self.page == Page::Settings =>
            {
                let delta = if matches!(key.code, KeyCode::Up | KeyCode::BackTab) {
                    -1
                } else {
                    1
                };
                self.move_setting(delta);
            }
            _ if self.page != Page::Catalog => {}
            KeyCode::Char('/') => self.apply(Action::Search),
            KeyCode::Char('f') if ctrl => self.apply(Action::Search),
            KeyCode::Char('v') => self.apply(Action::SelectedOnly),
            KeyCode::F(5) => self.apply(Action::Reload),
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Categories {
                    Focus::Tools
                } else {
                    Focus::Categories
                };
            }
            KeyCode::Up => self.step(self.focus, -1),
            KeyCode::Down => self.step(self.focus, 1),
            KeyCode::PageUp => self.step(self.focus, -10),
            KeyCode::PageDown => self.step(self.focus, 10),
            KeyCode::Home => self.step(self.focus, isize::MIN),
            KeyCode::End => self.step(self.focus, isize::MAX),
            KeyCode::Enter if self.focus == Focus::Categories => {
                if let Some(node) = self.category
                    && !self.catalog.categories[node].children.is_empty()
                {
                    self.apply(Action::Expand(node));
                } else {
                    self.focus = Focus::Tools;
                }
            }
            KeyCode::Left if self.focus == Focus::Categories => self.collapse_or_parent(),
            KeyCode::Left => self.focus = Focus::Categories,
            KeyCode::Right if self.focus == Focus::Categories => self.open_or_enter(),
            KeyCode::Char(' ') if self.focus == Focus::Categories => {
                self.apply(Action::ToggleBranch(self.category))
            }
            KeyCode::Enter => {
                if let Some(tool) = self.tool {
                    self.apply(Action::Details(tool));
                }
            }
            KeyCode::Char(' ') => {
                if let Some(tool) = self.tool {
                    self.apply(Action::ToggleTool(tool));
                }
            }
            _ => {}
        }
    }

    pub fn move_setting(&mut self, delta: isize) {
        let count = Setting::ALL.len() as isize;
        self.settings_cursor = (self.settings_cursor as isize + delta).rem_euclid(count) as usize;
        self.notice = None;
    }

    pub fn focused_setting(&self) -> Setting {
        Setting::ALL[self.settings_cursor]
    }

    pub fn monochrome(&self) -> bool {
        self.options.no_color || self.preferences.monochrome
    }

    pub fn mouse_enabled(&self) -> bool {
        !self.options.no_mouse && self.preferences.mouse
    }

    pub fn reduced_motion(&self) -> bool {
        self.preferences.reduced_motion
    }

    pub fn collapse_on_startup(&self) -> bool {
        self.preferences.collapse_on_startup
    }

    pub fn package_badges(&self) -> bool {
        self.preferences.package_badges
    }

    pub fn setting_overridden(&self, setting: Setting) -> bool {
        match setting {
            Setting::Theme => self.options.no_color,
            Setting::Mouse => self.options.no_mouse,
            _ => false,
        }
    }

    fn focus_setting(&mut self, setting: Setting) {
        self.settings_cursor = Setting::ALL
            .iter()
            .position(|item| *item == setting)
            .unwrap();
    }

    fn change_setting(&mut self, setting: Setting) {
        self.focus_setting(setting);
        if self.setting_overridden(setting) {
            self.notice = Some(self.t("SettingsOverrideShort").to_owned());
            return;
        }
        match setting {
            Setting::Language => {
                self.language = match self.language {
                    Language::En => Language::Es,
                    Language::Es => Language::En,
                };
                self.preferences.language = self.language;
                self.reconcile_tool();
            }
            Setting::Theme => self.preferences.monochrome = !self.preferences.monochrome,
            Setting::Mouse => self.preferences.mouse = !self.preferences.mouse,
            Setting::ReducedMotion => {
                self.preferences.reduced_motion = !self.preferences.reduced_motion
            }
            Setting::CollapseOnStartup => {
                self.preferences.collapse_on_startup = !self.preferences.collapse_on_startup;
                if self.collapse_on_startup() {
                    self.collapsed = self.catalog.collapsed_branches();
                    // Keep the current category reachable without changing the active filter.
                    let mut parent = self
                        .category
                        .and_then(|node| self.catalog.categories[node].parent);
                    while let Some(index) = parent {
                        self.collapsed.remove(&index);
                        parent = self.catalog.categories[index].parent;
                    }
                } else {
                    self.collapsed.clear();
                }
                self.category_offset = 0;
            }
            Setting::PackageBadges => {
                self.preferences.package_badges = !self.preferences.package_badges
            }
        }
        self.notice = Some(match self.preferences.save(&self.directory) {
            Ok(()) => self.t("SettingsSaved").to_owned(),
            Err(error) => format!("{} {error}", self.t("SettingsSaveFailed")),
        });
    }

    fn review(&mut self, operation: Operation) {
        let mut indices = self.selected.clone();
        if operation == Operation::Upgrade {
            if indices.is_empty() {
                indices.extend(self.catalog.all.iter().copied());
            }
            indices.retain(|&index| {
                matches!(
                    self.inventory
                        .package(self.catalog.tools[index].winget_id.as_deref()),
                    crate::packages::Package::Update(_)
                )
            });
        }
        if indices.is_empty() {
            let message = if operation == Operation::Install {
                "ChoosePackages"
            } else {
                "NoMatchingUpdates"
            };
            self.notice = Some(self.t(message).to_owned());
            return;
        }
        self.queue = Some(Queue::new(
            operations::Settings {
                operation,
                unattended: false,
                location: None,
            },
            operations::entries(&self.catalog, &indices),
        ));
    }

    fn queue_key(&mut self, key: KeyEvent, ctrl: bool) {
        let queue = self.queue.as_mut().expect("queue is active");
        if let Some(location) = &mut queue.edit_location {
            match key.code {
                KeyCode::Esc => queue.edit_location = None,
                KeyCode::Enter => {
                    let path = location.trim();
                    if path.is_empty() {
                        queue.settings.location = None;
                    } else if let Ok(path) = operations::Settings::location(PathBuf::from(path)) {
                        queue.settings.location = Some(path);
                    } else {
                        queue.location_invalid = true;
                        return;
                    }
                    queue.edit_location = None;
                    queue.location_invalid = false;
                }
                KeyCode::Backspace => {
                    location.pop();
                }
                KeyCode::Char(character)
                    if !ctrl && !character.is_control() && location.len() < 1024 =>
                {
                    location.push(character)
                }
                _ => {}
            }
            return;
        }
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
            self.apply(Action::QueueClose);
            return;
        }
        match key.code {
            KeyCode::Up => self.apply(Action::QueueScroll(-1)),
            KeyCode::Down => self.apply(Action::QueueScroll(1)),
            KeyCode::PageUp => self.apply(Action::QueueScroll(-8)),
            KeyCode::PageDown => self.apply(Action::QueueScroll(8)),
            KeyCode::Char('l') if !ctrl && queue.phase == Phase::Review => {
                queue.edit_location = Some(
                    queue
                        .settings
                        .location
                        .as_ref()
                        .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
                );
                queue.location_invalid = false;
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right
                if queue.phase == Phase::Review =>
            {
                queue.confirm = !queue.confirm;
            }
            KeyCode::Enter if matches!(queue.phase, Phase::Review | Phase::Finished) => {
                if queue.phase == Phase::Review && queue.confirm {
                    self.apply(Action::QueueConfirm);
                } else {
                    self.apply(Action::QueueClose);
                }
            }
            _ => {}
        }
    }

    pub fn poll_queue(&mut self) -> bool {
        let Some(queue) = self.queue.as_mut() else {
            return false;
        };
        let running = queue.phase == Phase::Running;
        let changed = queue.poll();
        if running && queue.phase == Phase::Finished {
            let report = serde_json::json!({
                "schemaVersion":1,"operation":queue.settings.operation,
                "location":queue.settings.location,"items":queue.entries
            });
            for path in preferences(&self.directory) {
                if let Some(folder) = path.parent()
                    && fs::create_dir_all(folder).is_ok()
                    && let Ok(mut file) = tempfile::NamedTempFile::new_in(folder)
                    && serde_json::to_writer(&mut file, &report).is_ok()
                    && file.persist(folder.join("last-operation.json")).is_ok()
                {
                    break;
                }
            }
            self.probe = None;
            self.start_probe();
        }
        changed
    }

    fn collapse_or_parent(&mut self) {
        if let Some(node) = self.category {
            let category = &self.catalog.categories[node];
            if !category.children.is_empty() && !self.collapsed.contains(&node) {
                self.apply(Action::Expand(node));
            } else {
                self.apply(Action::Category(category.parent));
            }
        }
    }

    fn open_or_enter(&mut self) {
        if let Some(node) = self.category {
            if self.collapsed.contains(&node) {
                self.apply(Action::Expand(node));
                return;
            }
            if let Some(child) = self.catalog.categories[node].children.first() {
                self.apply(Action::Category(Some(*child)));
                return;
            }
        }
        self.focus = Focus::Tools;
    }

    fn finish_search(&mut self, cancel: bool) {
        if self.editing {
            if cancel {
                self.query.clone_from(&self.previous_query);
                self.tool = self.previous_tool;
            }
            self.editing = false;
            self.search_matches.clear();
            self.focus = self.previous_focus;
            self.reconcile_tool();
        }
    }

    fn refresh_matches(&mut self) {
        self.search_matches = search::find(&self.catalog, self.language, &self.query);
        self.search_cursor = 0;
    }

    pub fn move_search(&mut self, delta: isize) {
        self.search_cursor = self
            .search_cursor
            .saturating_add_signed(delta)
            .min(self.search_matches.len().saturating_sub(1));
    }

    fn jump_to(&mut self, target: Target) {
        let (category, still_visible) = match target {
            Target::Category(index) => (
                index,
                self.catalog
                    .branch(Some(index))
                    .iter()
                    .any(|id| self.selected.contains(id)),
            ),
            Target::Tool(index) => (
                self.catalog.tool_category(index),
                self.selected.contains(&index),
            ),
        };
        self.selected_only &= still_visible;
        self.query.clear();
        self.finish_search(false);
        let mut node = Some(category);
        while let Some(index) = node {
            self.collapsed.remove(&index);
            node = self.catalog.categories[index].parent;
        }
        self.apply(Action::Category(Some(category)));
        if let Target::Tool(index) = target {
            self.apply(Action::Tool(index));
        }
        self.highlight = Some(Highlight {
            target,
            started: Instant::now(),
            visible: true,
        });
    }

    pub fn highlighted(&self, target: Target) -> bool {
        self.highlight
            .as_ref()
            .is_some_and(|mark| mark.visible && mark.target == target)
    }

    pub fn poll_highlight(&mut self) -> bool {
        let Some(mark) = &mut self.highlight else {
            return false;
        };
        let elapsed = mark.started.elapsed().as_millis();
        if elapsed >= 580 {
            let redraw = mark.visible;
            self.highlight = None;
            return redraw;
        }
        let visible = self.preferences.reduced_motion || !(160..330).contains(&elapsed);
        let redraw = visible != mark.visible;
        mark.visible = visible;
        redraw
    }

    fn reconcile_tool(&mut self) {
        let tools = self.tools();
        if self.tool.is_none_or(|index| !tools.contains(&index)) {
            self.tool = tools.first().copied();
        }
    }

    fn reload(&mut self) {
        let catalog = match Catalog::load(&self.directory) {
            Ok(catalog) => catalog,
            Err(error) => {
                self.notice = Some(format!("{} {error}", self.t("ReloadFailed")));
                return;
            }
        };
        let id = |node| {
            self.catalog
                .categories
                .get(node)
                .map(|entry: &crate::catalog::Category| entry.id.as_str())
        };
        self.category = self
            .category
            .and_then(id)
            .and_then(|id| catalog.category_index(id));
        self.collapsed = self
            .collapsed
            .iter()
            .copied()
            .filter_map(id)
            .filter_map(|id| catalog.category_index(id))
            .collect();
        let old_tool = self.tool.map(|index| self.catalog.tools[index].id.as_str());
        let selected_ids: HashSet<&str> = self
            .selected
            .iter()
            .map(|index| self.catalog.tools[*index].id.as_str())
            .collect();
        self.selected = catalog
            .tools
            .iter()
            .enumerate()
            .filter(|(_, tool)| selected_ids.contains(tool.id.as_str()))
            .map(|(index, _)| index)
            .collect();
        self.tool = old_tool.and_then(|id| catalog.tools.iter().position(|tool| tool.id == id));
        self.catalog = catalog;
        self.reconcile_tool();
        self.notice = Some(self.t("Reloaded").to_owned());
    }

    pub fn start_probe(&mut self) {
        if self.probe.is_none() {
            let tracked = Arc::new(
                self.catalog
                    .tools
                    .iter()
                    .filter_map(|tool| tool.winget_id.as_deref().map(str::to_ascii_lowercase))
                    .collect(),
            );
            self.winget = None;
            self.inventory = Inventory {
                tracked: Arc::clone(&tracked),
                ..Inventory::default()
            };
            self.diagnostic_offset = 0;
            self.probe = Some(Scan::start(tracked));
        }
    }

    pub fn poll_probe(&mut self) -> bool {
        let Some(probe) = &self.probe else {
            return false;
        };
        let mut changed = false;
        loop {
            match probe.results.try_recv() {
                Ok(CheckResult::Version(version)) => self.winget = Some(version),
                Ok(CheckResult::Installed(found)) => self.inventory.installed = Some(found),
                Ok(CheckResult::Updates(found)) => self.inventory.updates = Some(found),
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    return true;
                }
            }
            changed = true;
        }
    }

    pub fn probing(&self) -> bool {
        self.probe.is_some()
    }

    pub fn scroll_diagnostics(&mut self, delta: isize) {
        self.diagnostic_offset = self
            .diagnostic_offset
            .saturating_add_signed(delta)
            .min(self.catalog.tools.len());
    }
}

fn preferences(data: &Path) -> impl Iterator<Item = PathBuf> {
    let config = data.parent().unwrap_or(data).join("config");
    let local = config.join("preferences.json");
    let user = (!config.join("portable").is_file())
        .then(|| std::env::var_os("LOCALAPPDATA"))
        .flatten()
        .map(|root| PathBuf::from(root).join("PenGrab").join("preferences.json"));
    std::iter::once(local).chain(user)
}

#[derive(Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
struct Preferences {
    language: Language,
    monochrome: bool,
    mouse: bool,
    reduced_motion: bool,
    collapse_on_startup: bool,
    package_badges: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::En,
            monochrome: false,
            mouse: true,
            reduced_motion: false,
            collapse_on_startup: false,
            package_badges: true,
        }
    }
}

impl Preferences {
    fn load(data: &Path) -> Self {
        let mut files: Vec<_> = preferences(data)
            .filter_map(|path| {
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                Some((modified, path))
            })
            .collect();
        files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        files
            .into_iter()
            .find_map(|(_, path)| {
                let raw = fs::read_to_string(path).ok()?;
                serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()
            })
            .unwrap_or_default()
    }

    fn save(&self, data: &Path) -> io::Result<()> {
        let mut last = Ok(());
        for path in preferences(data) {
            let folder = path.parent().expect("preference path includes a folder");
            let result = (|| {
                fs::create_dir_all(folder)?;
                let mut temp = tempfile::NamedTempFile::new_in(folder)?;
                serde_json::to_writer(&mut temp, self)?;
                temp.persist(&path).map_err(|error| error.error)?;
                Ok(())
            })();
            if result.is_ok() {
                return result;
            }
            last = result;
        }
        last
    }
}

fn copy_to_clipboard(text: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let mut child = Command::new(windows_directory()?.join("System32").join("clip.exe"))
            .creation_flags(0x0800_0000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let written = child.stdin.take().unwrap().write_all(text.as_bytes());
        let status = child.wait()?;
        written?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("clip.exe could not copy the command"))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err(io::ErrorKind::Unsupported.into())
    }
}

fn open_download(url: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        Command::new(windows_directory()?.join("explorer.exe"))
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(windows)]
fn windows_directory() -> io::Result<PathBuf> {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Windows directory not found"))
}
