mod settings;

use crate::{
    app::{Action, App, Check, Focus, Page},
    catalog::{IconAccent, Tool},
    operations::{Operation, Phase, Status},
    packages::{CheckError, Package, WinGet},
    search::{Match, Target},
    selfupdate::State,
    terminal::clean_text as clean,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

const INK: Color = Color::Rgb(243, 238, 235);
const MUTED: Color = Color::Rgb(168, 166, 165);
const BORDER: Color = Color::Rgb(82, 79, 79);
const BACKGROUND: Color = Color::Rgb(17, 17, 17);
const BAR: Color = Color::Rgb(29, 28, 28);
const SIDEBAR: Color = Color::Rgb(25, 25, 25);
const RESULTS: Color = Color::Rgb(21, 21, 21);
const PANEL: Color = Color::Rgb(30, 29, 29);
const FIELD: Color = Color::Rgb(36, 34, 34);
const RED: Color = Color::Rgb(255, 106, 108);
const FOCUS: Color = Color::Rgb(135, 47, 54);
const CURRENT: Color = Color::Rgb(69, 41, 44);
const TRACK: Color = Color::Rgb(111, 73, 78);
const THUMB: Color = Color::Rgb(255, 151, 156);
const AMBER: Color = Color::Rgb(236, 185, 111);
const YELLOW: Color = Color::Rgb(255, 218, 113);
const HIGHLIGHT: Color = Color::Rgb(66, 49, 24);
const PANEL_BORDER: Color = Color::Rgb(145, 61, 67);
const ICON_TEAL: Color = Color::Rgb(105, 225, 205);
const ICON_BLUE: Color = Color::Rgb(150, 184, 255);
const ICON_GOLD: Color = Color::Rgb(252, 214, 145);

#[derive(Clone, Copy)]
struct Palette {
    mono: bool,
}

impl Palette {
    fn colors(self, foreground: Color, background: Color) -> Style {
        if self.mono {
            Style::default()
        } else {
            Style::default().fg(foreground).bg(background)
        }
    }

    fn panel(self, name: impl Into<Line<'static>>, focus: bool, surface: Color) -> Block<'static> {
        Block::default()
            .title(name)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(self.colors(if focus { RED } else { PANEL_BORDER }, surface))
            .style(self.colors(INK, surface))
    }

    fn row(self, focused: bool) -> Style {
        self.row_on(focused, PANEL)
    }

    fn row_on(self, focused: bool, surface: Color) -> Style {
        if focused {
            let mut style = self.colors(INK, FOCUS).add_modifier(Modifier::BOLD);
            if self.mono {
                style = style.add_modifier(Modifier::REVERSED);
            }
            style
        } else {
            self.colors(INK, surface)
        }
    }

    fn foreground(self, foreground: Color) -> Style {
        if self.mono {
            Style::default()
        } else {
            Style::default().fg(foreground)
        }
    }

    fn cursor(self, current: bool, focused: bool, surface: Color) -> Style {
        if current && !focused {
            let style = self.colors(INK, CURRENT).add_modifier(Modifier::BOLD);
            return if self.mono {
                style.add_modifier(Modifier::UNDERLINED)
            } else {
                style
            };
        }
        self.row_on(focused, surface)
    }

    fn search_match(self) -> Style {
        let style = self.foreground(YELLOW).add_modifier(Modifier::BOLD);
        if self.mono {
            style.add_modifier(Modifier::UNDERLINED)
        } else {
            style
        }
    }

    fn jump(self) -> Style {
        self.colors(INK, HIGHLIGHT)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }
}

#[derive(Default)]
pub struct HitMap {
    buttons: Vec<(Rect, Action)>,
    categories: Rect,
    tools: Rect,
    search_field: Rect,
    search_results: Rect,
    diagnostics: Rect,
    help: Rect,
    settings: Rect,
    jobs: Rect,
}

impl HitMap {
    fn button(&mut self, area: Rect, action: Action) {
        if !area.is_empty() {
            self.buttons.push((area, action));
        }
    }

    pub fn event(&self, app: &mut App, event: MouseEvent) -> bool {
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, action)) = self
                    .buttons
                    .iter()
                    .find(|(area, _)| area.contains(position))
                {
                    app.apply(*action);
                    return true;
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if event.kind == MouseEventKind::ScrollUp {
                    -1
                } else {
                    1
                };
                if self.jobs.contains(position) {
                    app.apply(Action::QueueScroll(delta));
                    return true;
                }
                if self.diagnostics.contains(position) {
                    app.scroll_diagnostics(delta);
                    return true;
                }
                if self.settings.contains(position) {
                    app.move_setting(delta);
                    return true;
                }
                if self.help.contains(position) {
                    app.apply(Action::HelpScroll(delta));
                    return true;
                }
                if self.search_results.contains(position) {
                    app.move_search(delta);
                    return true;
                }
                if self.categories.contains(position) {
                    app.step(Focus::Categories, delta);
                    return true;
                }
                if self.tools.contains(position) {
                    app.step(Focus::Tools, delta);
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) -> HitMap {
    let mut hit = HitMap::default();
    let full = frame.area();
    let palette = Palette {
        mono: app.monochrome(),
    };
    frame.render_widget(
        Block::default().style(palette.colors(INK, BACKGROUND)),
        full,
    );
    let minimum_height = if full.width >= 74 { 12 } else { 16 };
    if full.width < 32 || full.height < minimum_height {
        frame.render_widget(
            Paragraph::new(app.t("Resize")).wrap(Wrap { trim: true }),
            full,
        );
        return hit;
    }

    let [top, content, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(if full.height >= 20 { 3 } else { 2 }),
    ])
    .areas(full);
    let margin = if full.width >= 50 { 2 } else { 1 };
    chrome(frame, app, &mut hit, palette, top, margin);
    let content = inset(content, margin);

    match app.page {
        Page::Catalog => catalog(frame, app, &mut hit, palette, content),
        Page::Diagnostics => diagnostics(frame, app, &mut hit, palette, content),
        Page::Settings => settings::draw(frame, app, &mut hit, palette, content),
        Page::Help => help(frame, app, &mut hit, palette, content),
    }
    footer_bar(frame, app, &mut hit, palette, footer, margin);
    if app.release_dialog {
        hit = HitMap::default();
        frame
            .buffer_mut()
            .set_style(full, Style::default().add_modifier(Modifier::DIM));
        release_dialog(frame, app, &mut hit, palette, full);
    } else if app.queue.is_some() {
        hit = HitMap::default();
        frame
            .buffer_mut()
            .set_style(full, Style::default().add_modifier(Modifier::DIM));
        queue_dialog(frame, app, &mut hit, palette, full);
    } else if app.details_open {
        hit = HitMap::default();
        frame
            .buffer_mut()
            .set_style(full, Style::default().add_modifier(Modifier::DIM));
        detail_dialog(frame, app, &mut hit, palette, full);
    } else if app.editing {
        let search_field = hit.search_field;
        hit = HitMap::default();
        hit.button(search_field, Action::Search);
        search_overlay(frame, app, &mut hit, palette, search_field, footer.y);
        hit.button(full, Action::CancelSearch);
    }
    hit
}

fn chrome(
    frame: &mut Frame,
    app: &App,
    hit: &mut HitMap,
    palette: Palette,
    area: Rect,
    margin: u16,
) {
    frame.render_widget(Block::default().style(palette.colors(INK, BAR)), area);
    let body = inset(area, margin);
    let wide = body.width >= 83;
    let y = body.y + u16::from(wide);
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    let version_width = version.width() as u16;
    let brand = Line::from(vec![
        Span::styled("▌", palette.foreground(RED)),
        Span::styled(
            " PenGrab ",
            palette.foreground(INK).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if palette.mono { "/" } else { "✎" },
            palette.foreground(THUMB),
        ),
    ]);
    frame.render_widget(Paragraph::new(brand), Rect::new(body.x, y, 13, 1));
    frame.render_widget(
        Paragraph::new(version).style(palette.foreground(MUTED)),
        Rect::new(body.right() - version_width, y, version_width, 1),
    );
    let nav = if wide {
        Rect::new(body.x + 23, y, body.width - 25 - version_width, 1)
    } else {
        Rect::new(body.x, y + 1, body.width, 1)
    };
    rule(
        frame,
        palette,
        Rect::new(body.x, area.bottom() - 1, body.width, 1),
        BAR,
    );
    navigation(frame, app, hit, palette, nav);
}

fn navigation(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let pages = [
        (Page::Catalog, "CatalogTitle", "CatalogShort"),
        (Page::Diagnostics, "Diagnostics", "DiagnosticsShort"),
        (Page::Settings, "Settings", "SettingsShort"),
        (Page::Help, "Help", "Help"),
    ];
    let total: usize = pages
        .iter()
        .map(|(_, name, _)| app.t(name).width() + 4)
        .sum();
    let short = total > area.width as usize;
    let mut column = area.x;

    for (page, name, compact) in pages {
        let name = app.t(if short { compact } else { name });
        let selected = page == app.page;
        let padding = if short { " " } else { "  " };
        let marker = if palette.mono { ">" } else { "▸" };
        let prefix = if selected {
            format!("{marker}{}", if short { "" } else { " " })
        } else {
            padding.to_owned()
        };
        let width = ((prefix.width() + name.width() + padding.width()) as u16)
            .min(area.right().saturating_sub(column));
        let region = Rect::new(column, area.y, width, 1);
        let style = if selected {
            palette.row_on(true, BAR)
        } else {
            palette.colors(MUTED, BAR)
        };
        let label = Line::from(vec![
            Span::styled(
                prefix,
                if selected {
                    palette.foreground(RED)
                } else {
                    Style::default()
                },
            ),
            Span::raw(format!("{name}{padding}")),
        ]);
        frame.render_widget(Paragraph::new(label).style(style), region);
        if selected {
            frame.render_widget(
                Paragraph::new("━".repeat(width as usize)).style(palette.colors(RED, BAR)),
                Rect::new(region.x, region.bottom(), width, 1),
            );
        }
        hit.button(region, Action::Page(page));
        column += width;
    }
}

fn catalog(frame: &mut Frame, app: &mut App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let wide = area.width >= 70;
    let [toolbar, body] = Layout::vertical([
        Constraint::Length((if wide { 2 } else { 3 }) + u16::from(area.height >= 18)),
        Constraint::Min(1),
    ])
    .areas(area);
    catalog_toolbar(frame, app, hit, palette, toolbar, wide);

    let [tree, results] = if wide {
        let tree_width = if body.width >= 111 {
            39
        } else if body.width >= 87 {
            34
        } else {
            29
        };
        let [tree, _, results] = Layout::horizontal([
            Constraint::Length(tree_width),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .areas(body);
        [tree, results]
    } else {
        let rows = (body.height / 2).clamp(2, 7);
        Layout::vertical([Constraint::Length(rows), Constraint::Min(1)]).areas(body)
    };

    let [list, detail] = if wide && results.width >= 68 {
        let [list, _, detail] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length((results.width * 4 / 9).clamp(28, 46)),
        ])
        .areas(results);
        [list, detail]
    } else if results.height >= 13 {
        let [list, _, detail] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(7),
        ])
        .areas(results);
        [list, detail]
    } else {
        [results, Rect::ZERO]
    };

    category_tree(frame, app, hit, palette, tree);
    tool_list(frame, app, hit, palette, list, detail.is_empty());
    if !detail.is_empty() {
        tool_detail(frame, app, palette, detail, false);
    }
}

fn catalog_toolbar(
    frame: &mut Frame,
    app: &App,
    hit: &mut HitMap,
    palette: Palette,
    toolbar: Rect,
    wide: bool,
) {
    let find_label = app.t("Search");
    let selection_label = app.t(if toolbar.width < 38 {
        "SelectionCompact"
    } else {
        "SelectionTitle"
    });
    let selected = format!(
        "[{}] {}",
        if app.selected_only { "x" } else { " " },
        selection_label
    );
    let reload = if palette.mono {
        format!("[{}]", app.t("ReloadLabel"))
    } else {
        format!(" {} ", app.t("ReloadLabel"))
    };
    let buttons_width = (selected.width() + reload.width() + 2) as u16;
    let field = if wide {
        Rect::new(
            toolbar.x,
            toolbar.y + 1,
            toolbar.width.saturating_sub(buttons_width + 2),
            1,
        )
    } else {
        Rect::new(toolbar.x, toolbar.y + 1, toolbar.width, 1)
    };
    let label = format!(" {} {find_label}: ", if app.editing { ">" } else { "/" });
    let available = field.width.saturating_sub(label.width() as u16) as usize;
    let display = if app.query.is_empty() && !app.editing {
        clip(app.t("SearchFieldHint"), available)
    } else {
        let mut suffix = tail(
            &app.query,
            available.saturating_sub(usize::from(app.editing)),
        );
        if app.editing {
            suffix.push('_');
        }
        suffix
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label,
                palette.foreground(if app.editing { RED } else { INK }),
            ),
            Span::styled(
                display,
                if app.editing {
                    palette.search_match()
                } else {
                    palette.foreground(if app.query.is_empty() { MUTED } else { INK })
                },
            ),
        ]))
        .style(palette.row_on(app.editing, FIELD)),
        field,
    );
    hit.search_field = field;
    hit.button(field, Action::Search);

    let (x, y) = if wide {
        (field.right() + 2, field.y)
    } else {
        (toolbar.x, field.y + 1)
    };
    let select_area = Rect::new(
        x,
        y,
        (selected.width() as u16).min(toolbar.right().saturating_sub(x)),
        1,
    );
    frame.render_widget(
        Paragraph::new(selected).style(palette.row_on(app.selected_only, FIELD)),
        select_area,
    );
    hit.button(select_area, Action::SelectedOnly);
    let gap = if toolbar.right().saturating_sub(select_area.right()) as usize >= reload.width() + 2
    {
        2
    } else {
        0
    };
    let reload_x = select_area.right() + gap;
    if reload_x < toolbar.right() {
        let reload_area = Rect::new(
            reload_x,
            y,
            (reload.width() as u16).min(toolbar.right() - reload_x),
            1,
        );
        frame.render_widget(
            Paragraph::new(reload).style(palette.colors(INK, FIELD)),
            reload_area,
        );
        hit.button(reload_area, Action::Reload);
    }
}

fn search_overlay(
    frame: &mut Frame,
    app: &App,
    hit: &mut HitMap,
    palette: Palette,
    field: Rect,
    bottom: u16,
) {
    let count = app.search_matches.len();
    let y = field.bottom();
    let height = (count.clamp(1, 8) as u16 + 2).min(bottom.saturating_sub(y));
    let bounds = Rect::new(field.x, y, field.width.min(78), height);
    if height <= 2 {
        return;
    }
    frame.render_widget(Clear, bounds);
    let title = format!(" {} ", app.t("SearchMatches"));
    let panel = palette
        .panel(title, true, PANEL)
        .padding(Padding::horizontal(1));
    let scroll_area = panel.inner(bounds);
    let body = scroll_content(scroll_area, count);
    frame.render_widget(panel, bounds);
    hit.search_results = bounds;
    if count == 0 {
        let message = if app.query.trim().is_empty() {
            "SearchFieldHint"
        } else {
            "NoSearchMatches"
        };
        frame.render_widget(
            Paragraph::new(app.t(message)).style(palette.foreground(MUTED)),
            body,
        );
        return;
    }

    let offset = visible_offset(0, app.search_cursor, count, body.height as usize);
    let mut rows = Vec::new();
    for (row, item) in app
        .search_matches
        .iter()
        .skip(offset)
        .take(body.height as usize)
        .enumerate()
    {
        let current = offset + row == app.search_cursor;
        rows.push(
            ListItem::new(search_match_line(app, palette, item, current, body.width))
                .style(palette.row(current)),
        );
        hit.button(
            Rect::new(body.x, body.y + row as u16, body.width, 1),
            Action::SearchJump(item.target),
        );
    }
    frame.render_widget(List::new(rows), body);
    scroll_rail(frame, palette, scroll_area, offset, count, PANEL);
}

fn search_match_line(
    app: &App,
    palette: Palette,
    item: &Match,
    current: bool,
    width: u16,
) -> Line<'static> {
    let width = (width as usize).saturating_sub(2);
    let evidence = item.evidence.join(" / ");
    let evidence = clip(&clean(&evidence), width / 3);
    let title_width =
        width.saturating_sub(evidence.width() + 3 * usize::from(!evidence.is_empty()));
    let title = clip(&item.title, title_width);
    let mut spans = vec![Span::styled(
        if current { "> " } else { "  " },
        palette.foreground(RED),
    )];
    let mut offset = 0;
    for grapheme in Span::raw(&title).styled_graphemes(Style::default()) {
        let next = offset + grapheme.symbol.len();
        let matched = item
            .highlights
            .iter()
            .any(|range| offset < range.end && next > range.start);
        spans.push(Span::styled(
            clean(grapheme.symbol),
            if matched {
                palette.search_match()
            } else {
                Style::default()
            },
        ));
        offset = next;
    }
    if !evidence.is_empty() {
        spans.push(Span::styled(" · ", palette.foreground(MUTED)));
        spans.push(Span::styled(evidence.clone(), palette.search_match()));
    }
    if matches!(item.target, Target::Category(_)) {
        let kind = app.t("SearchCategory");
        let used = title.width() + evidence.width() + 3 * usize::from(!evidence.is_empty());
        if width.saturating_sub(used) >= kind.width() + 2 {
            spans.push(Span::styled(format!("  {kind}"), palette.foreground(MUTED)));
        }
    }
    Line::from(spans)
}

fn tail(text: &str, width: usize) -> String {
    let span = Span::raw(text);
    let graphemes: Vec<_> = span.styled_graphemes(Style::default()).collect();
    let mut used = 0;
    let mut bytes = 0;
    for grapheme in graphemes.into_iter().rev() {
        used += grapheme.symbol.width();
        if used > width {
            break;
        }
        bytes += grapheme.symbol.len();
    }
    text[text.len() - bytes..].to_owned()
}

fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut result = String::new();
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
        let next = used + grapheme.symbol.width();
        if next >= width {
            break;
        }
        result.push_str(grapheme.symbol);
        used = next;
    }
    result.push('…');
    result
}

#[cfg(test)]
#[test]
fn width_limits_keep_combined_characters_and_emoji_intact() {
    let emoji = "👩‍💻";
    let label = format!("a\u{301}{emoji}xyz");
    assert_eq!(clip(&label, 4), format!("a\u{301}{emoji}…"));
    assert_eq!(clip(&label, 3), "a\u{301}…");
    assert_eq!(tail(&label, 4), "xyz");
    assert_eq!(tail(&label, 5), format!("{emoji}xyz"));
}
fn inset(area: Rect, margin: u16) -> Rect {
    Rect::new(
        area.x + margin,
        area.y,
        area.width.saturating_sub(margin * 2),
        area.height,
    )
}

fn rule(frame: &mut Frame, palette: Palette, area: Rect, surface: Color) {
    let bar = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(bar).style(palette.colors(BORDER, surface)),
        area,
    );
}

fn section(
    frame: &mut Frame,
    palette: Palette,
    area: Rect,
    title: &str,
    aside: &str,
    surface: Color,
    focused: bool,
) -> Rect {
    let width = area.width.saturating_sub(2) as usize;
    let focused_title = if focused {
        format!("{} {title}", if palette.mono { ">" } else { "▸" })
    } else {
        title.to_owned()
    };
    let heading = clip(&focused_title, width.saturating_sub(2));
    let label = Line::styled(
        format!(" {heading} "),
        palette
            .foreground(if focused { THUMB } else { INK })
            .add_modifier(Modifier::BOLD),
    );
    let mut block = palette.panel(label, focused, surface);
    if !aside.is_empty() && heading.width() + aside.width() + 5 <= width {
        block = block.title_top(
            Line::styled(format!(" {aside} "), palette.foreground(MUTED)).right_aligned(),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn category_tree(frame: &mut Frame, app: &mut App, hit: &mut HitMap, palette: Palette, area: Rect) {
    enum Row {
        Category(usize),
        Divider,
    }

    let inner = section(
        frame,
        palette,
        area,
        app.t("CategoriesTitle"),
        "",
        SIDEBAR,
        app.focus == Focus::Categories,
    );
    hit.categories = area;
    if inner.height == 0 {
        return;
    }
    let global = Rect::new(inner.x, inner.y, inner.width, 1);
    let separator = u16::from(inner.height >= 5);
    if separator > 0 {
        rule(
            frame,
            palette,
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
            SIDEBAR,
        );
    }
    let start = 1 + separator;
    let scroll_area = Rect::new(inner.x, inner.y + start, inner.width, inner.height - start);
    let mut rows = Vec::new();
    let mut saw_root = false;
    // In short panes, reserve all visible rows for actionable categories.
    let show_dividers = scroll_area.height >= 8;
    for node in app
        .catalog
        .visible_categories(&app.collapsed)
        .into_iter()
        .flatten()
    {
        if app.catalog.categories[node].parent.is_none() {
            if show_dividers && saw_root {
                rows.push(Row::Divider);
            }
            saw_root = true;
        }
        rows.push(Row::Category(node));
    }
    let active_row = rows
        .iter()
        .position(|row| matches!(row, Row::Category(node) if Some(*node) == app.category))
        .unwrap_or(0);
    let inner = scroll_content(scroll_area, rows.len());
    let count_inset = (scroll_area.width - inner.width).max(1);
    all_tools_row(frame, app, hit, palette, global, count_inset);
    app.category_offset = visible_offset(
        app.category_offset,
        active_row,
        rows.len(),
        inner.height as usize,
    );
    if let Some(node) = app.category
        && !app.collapsed.contains(&node)
        && !app.catalog.categories[node].children.is_empty()
        && inner.height >= 2
    {
        let children = app.catalog.categories[node]
            .children
            .len()
            .min(inner.height as usize - 1);
        app.category_offset = visible_offset(
            app.category_offset,
            active_row + children,
            rows.len(),
            inner.height as usize,
        )
        .min(active_row);
    }

    let mut items = Vec::new();
    for (row, entry) in rows
        .iter()
        .skip(app.category_offset)
        .take(inner.height as usize)
        .enumerate()
    {
        let Row::Category(node) = entry else {
            items.push(
                ListItem::new("─".repeat(inner.width as usize))
                    .style(palette.colors(BORDER, SIDEBAR)),
            );
            continue;
        };
        let node = *node;
        let category = &app.catalog.categories[node];
        let name = category.name.get(app.language);
        let depth = category.depth + 1;
        let branches = !category.children.is_empty();
        let selected = app.category == Some(node);
        let focused = selected && app.focus == Focus::Categories;
        let highlight = app.highlighted(Target::Category(node));
        let marker = if highlight {
            "» "
        } else if selected {
            "> "
        } else {
            "  "
        };
        let display_icon = category.icon.filter(|_| !palette.mono);
        let icon_color = match display_icon.and_then(|value| value.accent) {
            Some(IconAccent::Teal) => ICON_TEAL,
            Some(IconAccent::Blue) => ICON_BLUE,
            Some(IconAccent::Gold) => ICON_GOLD,
            None if selected => RED,
            None => MUTED,
        };
        let glyph = match display_icon.map(|value| value.glyph).or(category.symbol) {
            Some(symbol) => format!("{symbol} "),
            None if depth == 1 => "  ".to_owned(),
            None => String::new(),
        };
        let glyph_width = glyph.width();
        let indent = (depth * 2).min(inner.width.saturating_sub(10 + glyph_width as u16) as usize);
        let arrow = if !branches {
            " "
        } else if app.collapsed.contains(&node) {
            "+"
        } else {
            "-"
        };
        let guides = if depth > 1 && indent >= 2 {
            let last = category.parent.is_some_and(|parent| {
                app.catalog.categories[parent].children.last() == Some(&node)
            });
            format!(
                "{}{}─",
                "  ".repeat((indent - 2) / 2),
                if last { "└" } else { "├" }
            )
        } else {
            " ".repeat(indent)
        };
        let state = app.check(Some(node));
        let count = app.catalog.branch(Some(node)).len().to_string();
        let show_count = inner.width >= 29;
        let leading = 8 + guides.width() + glyph_width;
        let trailing = if show_count { count.width() + 2 } else { 0 };
        let label_space = (inner.width as usize).saturating_sub(leading + trailing);
        let title = clip(&clean(name), label_space);
        let mut line = vec![
            Span::styled(
                marker,
                palette.foreground(if highlight || focused { YELLOW } else { THUMB }),
            ),
            Span::styled(format!("{guides}{arrow} "), palette.foreground(MUTED)),
            Span::styled(glyph, palette.foreground(icon_color)),
            check_span(state, palette),
            Span::raw(format!(" {title}")),
        ];
        if show_count {
            line.push(Span::styled(
                format!(
                    "{}{count}",
                    " ".repeat(label_space.saturating_sub(title.width()) + 2)
                ),
                palette.foreground(MUTED),
            ));
        }
        let style = if highlight {
            palette.jump()
        } else {
            palette.cursor(selected, focused, SIDEBAR)
        };
        items.push(ListItem::new(Line::from(line)).style(style));

        let row_area = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let arrow_x = inner.x + 2 + guides.width() as u16;
        if branches {
            hit.button(segment(row_area, arrow_x, 2), Action::Expand(node));
        }
        hit.button(
            segment(row_area, arrow_x + 2 + glyph_width as u16, 3),
            Action::ToggleBranch(Some(node)),
        );
        hit.button(row_area, Action::Category(Some(node)));
    }
    frame.render_widget(List::new(items), inner);
    scroll_rail(
        frame,
        palette,
        scroll_area,
        app.category_offset,
        rows.len(),
        SIDEBAR,
    );
}

fn all_tools_row(
    frame: &mut Frame,
    app: &App,
    hit: &mut HitMap,
    palette: Palette,
    area: Rect,
    count_inset: u16,
) {
    let current = app.category.is_none();
    let focused = current && app.focus == Focus::Categories;
    let count = app.catalog.all.len().to_string();
    let count_end = area.width.saturating_sub(count_inset) as usize;
    let room = count_end.saturating_sub(6 + count.width());
    let full_title = app.t("AllCatalogTools");
    let title = clip(
        if full_title.width() <= room {
            full_title
        } else {
            app.t("SearchAll")
        },
        room,
    );
    let pad = room.saturating_sub(title.width());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(if current { "> " } else { "  " }, palette.foreground(THUMB)),
            check_span(app.check(None), palette),
            Span::styled(
                format!(" {title}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}{count}", " ".repeat(pad)),
                palette.foreground(MUTED),
            ),
        ]))
        .style(palette.cursor(current, focused, FIELD)),
        area,
    );
    hit.button(segment(area, area.x + 2, 3), Action::ToggleBranch(None));
    hit.button(area, Action::Category(None));
}

fn package_badge<'a>(app: &'a App, tool: &Tool) -> (&'a str, Color) {
    match app.inventory.package(tool.winget_id.as_deref()) {
        Package::Update(_) => (app.t("UpdateBadge"), YELLOW),
        Package::Installed(_) => (app.t("InstalledShort"), ICON_TEAL),
        Package::NotDetected => (app.t("NotDetected"), MUTED),
        Package::Unknown if tool.winget_id.is_some() => (app.t("PackageIdShort"), MUTED),
        Package::Unknown if tool.publisher_download.is_some() => (app.t("DownloadShort"), MUTED),
        Package::Unknown if tool.publisher_project.is_some() => (app.t("ProjectShort"), MUTED),
        Package::Unknown => (app.t("PackagePendingShort"), AMBER),
    }
}

fn tool_list(
    frame: &mut Frame,
    app: &mut App,
    hit: &mut HitMap,
    palette: Palette,
    area: Rect,
    open_details: bool,
) {
    let visible = app.tools();
    let title = if visible.len() == 1 {
        app.t("ResultCountOne").to_owned()
    } else {
        app.t("ResultCount")
            .replace("{0}", &visible.len().to_string())
    };
    let heading = app.t("ToolListTitle");
    let (heading, aside) = if heading.width() + title.width() + 5 <= area.width as usize {
        (heading, title.as_str())
    } else {
        (title.as_str(), "")
    };
    let scroll_area = section(
        frame,
        palette,
        area,
        heading,
        aside,
        RESULTS,
        app.focus == Focus::Tools,
    );
    let inner = scroll_content(scroll_area, visible.len());
    hit.tools = area;
    if visible.is_empty() {
        let text = if app.selected_only {
            "NoSelected"
        } else {
            "NoTools"
        };
        frame.render_widget(Paragraph::new(app.t(text)).wrap(Wrap { trim: true }), inner);
        return;
    }
    let active = visible
        .iter()
        .position(|index| Some(*index) == app.tool)
        .unwrap_or(0);
    app.tool_offset = visible_offset(
        app.tool_offset,
        active,
        visible.len(),
        inner.height as usize,
    );
    let items: Vec<_> = visible
        .iter()
        .skip(app.tool_offset)
        .take(inner.height as usize)
        .enumerate()
        .map(|(row, index)| {
            let current = Some(*index) == app.tool;
            let highlight = app.highlighted(Target::Tool(*index));
            let marker = if highlight {
                "» "
            } else if current {
                "> "
            } else {
                "  "
            };
            let check = if app.selected.contains(index) {
                Check::Full
            } else {
                Check::Empty
            };
            let tool = &app.catalog.tools[*index];
            let (status, accent) = package_badge(app, tool);
            let status = if inner.width < 35 {
                clip(status, inner.width as usize / 3)
            } else {
                status.to_owned()
            };
            let trailing = if app.package_badges() && inner.width >= 24 {
                status.width() + 2
            } else {
                0
            };
            let width = (inner.width as usize).saturating_sub(6 + trailing);
            let title = clip(&clean(&tool.name), width);
            let mut label = vec![
                Span::styled(
                    marker,
                    palette.foreground(if highlight { YELLOW } else { RED }),
                ),
                check_span(check, palette),
                Span::raw(format!(" {title}")),
            ];
            if trailing > 0 {
                label.push(Span::styled(
                    format!(
                        "{}{status}",
                        " ".repeat(width.saturating_sub(title.width()) + 2)
                    ),
                    palette.foreground(accent),
                ));
            }
            let row_area = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
            hit.button(
                segment(row_area, inner.x + 2, 3),
                Action::ToggleTool(*index),
            );
            let action = if open_details {
                Action::Details(*index)
            } else {
                Action::Tool(*index)
            };
            hit.button(row_area, action);
            let style = if highlight {
                palette.jump()
            } else {
                palette.cursor(current, current && app.focus == Focus::Tools, RESULTS)
            };
            ListItem::new(Line::from(label)).style(style)
        })
        .collect();
    frame.render_widget(List::new(items), inner);
    scroll_rail(
        frame,
        palette,
        scroll_area,
        app.tool_offset,
        visible.len(),
        RESULTS,
    );
}

fn release_dialog(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let Some(update) = &app.release_update else {
        return;
    };
    let State::Available(version) = &update.state else {
        return;
    };
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(4).min(10);
    let bounds = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, bounds);
    let inner = section(
        frame,
        palette,
        bounds,
        app.t("SelfUpdateTitle"),
        "",
        PANEL,
        true,
    );
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(app.t("SelfUpdateConfirm").replace("{0}", version))
            .wrap(Wrap { trim: true }),
        body,
    );
    let cancel = format!("[{}]", app.t("JobCancel"));
    let proceed = format!("[{}]", app.t("JobStart"));
    let cancel_box = segment(footer, footer.x, cancel.width() as u16);
    let proceed_box = segment(footer, cancel_box.right() + 1, proceed.width() as u16);
    frame.render_widget(
        Paragraph::new(cancel).style(palette.row(!update.confirm)),
        cancel_box,
    );
    frame.render_widget(
        Paragraph::new(proceed).style(palette.row(update.confirm)),
        proceed_box,
    );
    hit.button(cancel_box, Action::CloseSelfUpdate);
    hit.button(proceed_box, Action::ApplySelfUpdate);
}

fn queue_dialog(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let queue = app.queue.as_ref().expect("queue is active");
    let width = area.width.saturating_sub(4).min(86);
    let height = area.height.saturating_sub(2).min(24);
    let bounds = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, bounds);
    let title = match (queue.phase, queue.settings.operation) {
        (Phase::Review, Operation::Install) => "JobReviewInstall",
        (Phase::Review, Operation::Upgrade) => "JobReviewUpgrade",
        (Phase::Running, _) => "JobRunning",
        (Phase::Finished, _) => "JobResults",
    };
    let inner = section(frame, palette, bounds, app.t(title), "", PANEL, true);
    let [summary, location, list, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    let mut done = 0;
    let mut finished = 0;
    for entry in &queue.entries {
        if entry.status.complete() {
            done += 1;
        }
        if !matches!(entry.status, Status::Planned | Status::Running) {
            finished += 1;
        }
    }
    let info = if queue.phase == Phase::Review {
        app.t("JobReviewHint").to_owned()
    } else if queue.stopping() && queue.phase == Phase::Running {
        app.t("JobStopping").to_owned()
    } else {
        app.t("JobProgress")
            .replace("{0}", &finished.to_string())
            .replace("{1}", &queue.entries.len().to_string())
            .replace("{2}", &done.to_string())
    };
    frame.render_widget(
        Paragraph::new(clip(&info, summary.width as usize)).style(palette.foreground(MUTED)),
        summary,
    );
    if let Some(input) = &queue.edit_location {
        let label = app.t(if queue.location_invalid {
            "JobLocationInvalid"
        } else {
            "JobLocation"
        });
        let room = (location.width as usize).saturating_sub(label.width() + 2);
        let display = format!("{label} {}_", tail(input, room));
        frame.render_widget(
            Paragraph::new(display).style(palette.search_match()),
            location,
        );
    } else if queue.phase == Phase::Review {
        let value = queue
            .settings
            .location
            .as_ref()
            .map(|path| path.to_string_lossy())
            .unwrap_or_else(|| app.t("JobDefaultPath").into());
        let line = format!("[L] {}  {value}", app.t("JobLocation"));
        frame.render_widget(
            Paragraph::new(clip(&line, location.width as usize)).style(palette.foreground(MUTED)),
            location,
        );
    } else if let Some(error) = queue
        .entries
        .iter()
        .rev()
        .find_map(|entry| entry.detail.as_deref())
    {
        frame.render_widget(
            Paragraph::new(clip(error, location.width as usize)).style(palette.foreground(AMBER)),
            location,
        );
    }
    let count = queue.entries.len();
    let offset = queue.offset.min(count.saturating_sub(list.height as usize));
    let body = scroll_content(list, count);
    hit.jobs = list;
    let items = queue
        .entries
        .iter()
        .skip(offset)
        .take(body.height as usize)
        .map(|entry| {
            let status = if let Some(version) = &entry.version {
                format!("{} {version}", app.t(entry.status.label()))
            } else {
                app.t(entry.status.label()).to_owned()
            };
            let status = clip(&status, (body.width as usize * 2 / 3).max(1));
            let name_room = (body.width as usize).saturating_sub(status.width() + 2);
            let name = clip(&clean(&entry.name), name_room);
            let accent = if entry.status.complete() {
                ICON_TEAL
            } else if entry.status == Status::Running {
                YELLOW
            } else {
                MUTED
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!(
                    "{name}{}",
                    " ".repeat(name_room.saturating_sub(name.width()) + 2)
                )),
                Span::styled(status, palette.foreground(accent)),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items), body);
    scroll_rail(frame, palette, list, offset, count, PANEL);
    if queue.edit_location.is_some() {
        frame.render_widget(Paragraph::new(app.t("JobEditKeys")), footer);
    } else if queue.phase == Phase::Review {
        let cancel = format!("[{}]", app.t("JobCancel"));
        let confirm = format!("[{}]", app.t("JobStart"));
        let cancel_box = segment(footer, footer.x, cancel.width() as u16);
        let confirm_box = segment(footer, cancel_box.right() + 1, confirm.width() as u16);
        frame.render_widget(
            Paragraph::new(cancel).style(palette.row(!queue.confirm)),
            cancel_box,
        );
        frame.render_widget(
            Paragraph::new(confirm).style(palette.row(queue.confirm)),
            confirm_box,
        );
        hit.button(cancel_box, Action::QueueClose);
        hit.button(confirm_box, Action::QueueConfirm);
    } else {
        let label = app.t(if queue.phase == Phase::Running {
            "JobStop"
        } else {
            "Back"
        });
        let label = format!("[Esc {label}]");
        let button = segment(footer, footer.x, label.width() as u16);
        frame.render_widget(Paragraph::new(label).style(palette.row(true)), button);
        hit.button(button, Action::QueueClose);
    }
}

fn detail_dialog(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let width = area.width.saturating_sub(4).min(70);
    let height = area.height.saturating_sub(4).min(16);
    let bounds = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, bounds);
    let download = app
        .tool
        .map(|index| &app.catalog.tools[index])
        .filter(|tool| tool.publisher_url().is_some());
    let choices = [
        (
            download.is_some_and(|tool| tool.winget_install_command().is_some()),
            "W",
            app.t("CopyWinGet"),
            Action::CopyWinGet,
        ),
        (
            download.is_some(),
            "O",
            download
                .and_then(|tool| app.publisher_label(tool))
                .unwrap_or_default(),
            Action::OpenPublisher,
        ),
    ];
    let count = choices.iter().filter(|(show, ..)| *show).count() as u16;
    let [body, sources, action] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(count + u16::from(app.notice.is_some())),
        Constraint::Length(1),
    ])
    .areas(bounds);
    tool_detail(frame, app, palette, body, true);
    let mut row = sources.y;
    for (_, key, label, choice) in choices.into_iter().filter(|(show, ..)| *show) {
        let label = format!("[{key}] {label}");
        let button = Rect::new(sources.x, row, sources.width.min(label.width() as u16), 1);
        frame.render_widget(
            Paragraph::new(label).style(palette.row_on(false, FIELD)),
            button,
        );
        hit.button(button, choice);
        row += 1;
    }
    if let Some(notice) = &app.notice {
        frame.render_widget(
            Paragraph::new(clip(&clean(notice), sources.width as usize))
                .style(palette.foreground(INK)),
            Rect::new(sources.x, row, sources.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(format!("[Esc {}]", app.t("Back"))).style(palette.row(true)),
        action,
    );
    hit.button(action, Action::CloseDetails);
}

fn tool_detail(frame: &mut Frame, app: &App, palette: Palette, area: Rect, full: bool) {
    let tool = app.tool.map(|index| (index, &app.catalog.tools[index]));
    let heading = app.t("DetailsTitle");
    let body = if full {
        let block = palette
            .panel(format!(" {heading} "), true, PANEL)
            .padding(Padding::horizontal(1));
        let body = block.inner(area);
        frame.render_widget(block, area);
        body
    } else {
        section(frame, palette, area, heading, "", PANEL, false)
    };
    if let Some((index, tool)) = tool {
        let mut lines = vec![
            Line::styled(
                clean(&tool.name),
                palette.foreground(INK).add_modifier(Modifier::BOLD),
            ),
            Line::from(clean(&app.catalog.category_line(index, app.language)))
                .style(palette.foreground(MUTED)),
        ];
        match app.inventory.package(tool.winget_id.as_deref()) {
            Package::Update(update) => {
                lines.push(Line::styled(
                    format!("{}  {}", app.t("InstalledShort"), clean(&update.installed)),
                    palette.foreground(ICON_TEAL),
                ));
                lines.push(Line::styled(
                    format!("{}  {}", app.t("UpdateShort"), clean(&update.available)),
                    palette.search_match(),
                ));
            }
            Package::Installed(version) => lines.push(Line::styled(
                format!("{}  {}", app.t("InstalledShort"), clean(version)),
                palette.foreground(ICON_TEAL),
            )),
            Package::NotDetected => lines.push(Line::styled(
                app.t("NotDetected"),
                palette.foreground(MUTED),
            )),
            Package::Unknown => {}
        }
        if body.height >= 9 {
            lines.push(Line::default());
        }
        lines.push(Line::from(clean(tool.description.get(app.language))));
        if body.height >= 9 {
            lines.push(Line::default());
        }
        let package = if let Some(id) = &tool.winget_id {
            Line::from(vec![
                Span::styled("WinGet  ", palette.foreground(MUTED)),
                Span::raw(clean(id)),
            ])
        } else if let Some(label) = app.publisher_label(tool) {
            Line::styled(label, palette.foreground(MUTED))
        } else {
            Line::styled(app.t("NoWinGetPackage"), palette.foreground(AMBER))
        };
        lines.push(package);
        if !full
            && tool.winget_id.is_some()
            && let Some(label) = app.publisher_label(tool)
        {
            lines.push(Line::styled(label, palette.foreground(MUTED)));
        }
        if full && tool.winget_id.is_some() && body.height >= 8 {
            lines.push(Line::styled(
                app.t("PackageVerified"),
                palette.foreground(MUTED),
            ));
        }
        if full && let Some(source) = &tool.kali {
            lines.push(Line::default());
            lines.push(Line::styled(app.t("SourceKali"), palette.foreground(MUTED)));
            lines.push(Line::from(clean(source)));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .scroll((if full { app.detail_scroll } else { 0 }, 0)),
            body,
        );
    }
}

fn diagnostics(frame: &mut Frame, app: &mut App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let title = app.t(if area.width < 36 {
        "InventoryTitleShort"
    } else {
        "InstalledAndUpdates"
    });
    let inner = section(frame, palette, area, title, "", PANEL, false);
    let [connection, action, summary, divider, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2 + u16::from(app.release_update.is_some())),
        Constraint::Length(u16::from(inner.height >= 11)),
        Constraint::Min(1),
    ])
    .areas(inner);
    let checking = app.probing();
    let version = if checking && app.winget.is_none() {
        app.t("WinGetChecking").to_owned()
    } else {
        winget_status(app)
    };
    let environment = format!(
        "{version}   ·   {} {}",
        app.t("Architecture"),
        std::env::consts::ARCH
    );
    frame.render_widget(
        Paragraph::new(if environment.width() <= connection.width as usize {
            environment
        } else {
            version
        })
        .style(palette.foreground(MUTED)),
        connection,
    );
    if !checking {
        let refresh = format!("[{}]", app.t("Refresh"));
        let button = Rect::new(
            action.x,
            action.y,
            action.width.min(refresh.width() as u16),
            action.height,
        );
        frame.render_widget(Paragraph::new(refresh).style(palette.row(true)), button);
        hit.button(button, Action::Diagnose);
    }

    let mut rows: Vec<_> = app
        .catalog
        .tools
        .iter()
        .filter_map(|tool| {
            let package = app.inventory.package(tool.winget_id.as_deref());
            matches!(package, Package::Installed(_) | Package::Update(_)).then_some((tool, package))
        })
        .collect();
    let updates = rows
        .iter()
        .filter(|(_, package)| matches!(package, Package::Update(_)))
        .count();
    let counts = [
        (
            app.t("InstalledCount"),
            app.inventory
                .installed
                .as_ref()
                .map(|state| state.as_ref().map(|_| rows.len())),
            ICON_TEAL,
        ),
        (
            app.t("UpdatesAvailable"),
            app.inventory
                .updates
                .as_ref()
                .map(|state| state.as_ref().map(|_| updates)),
            YELLOW,
        ),
    ];
    let mut lines = counts
        .into_iter()
        .map(|(label, result, accent)| {
            let value = match result {
                Some(Ok(count)) => count.to_string(),
                Some(Err(CheckError::TimedOut)) => app.t("InventoryTimeout").to_owned(),
                None if checking => app.t("InventoryChecking").to_owned(),
                Some(Err(_)) | None => app.t("InventoryUnavailable").to_owned(),
            };
            let value_style = if result.is_some_and(|state| state.is_ok()) {
                palette.foreground(accent).add_modifier(Modifier::BOLD)
            } else {
                palette.foreground(MUTED)
            };
            Line::from(vec![
                Span::raw(format!("{label}  ")),
                Span::styled(value, value_style),
            ])
        })
        .collect::<Vec<_>>();
    if let Some(update) = &app.release_update {
        let (label, style) = match &update.state {
            State::Checking => (
                app.t("SelfUpdateChecking").to_owned(),
                palette.foreground(MUTED),
            ),
            State::Current => (
                app.t("SelfUpdateCurrent").to_owned(),
                palette.foreground(MUTED),
            ),
            State::Unavailable => (
                app.t("SelfUpdateUnavailable").to_owned(),
                palette.foreground(MUTED),
            ),
            State::Available(version) => {
                let label = format!("[F8 {} {version}]", app.t("SelfUpdateAvailable"));
                let button = Rect::new(
                    summary.x,
                    summary.y + 2,
                    summary.width.min(label.width() as u16),
                    1,
                );
                hit.button(button, Action::ReviewSelfUpdate);
                (label, palette.search_match())
            }
        };
        lines.push(Line::styled(label, style));
    }
    frame.render_widget(Paragraph::new(lines), summary);
    if divider.height > 0 {
        rule(frame, palette, divider, PANEL);
    }
    if rows.is_empty() {
        if matches!(app.inventory.installed, Some(Ok(_))) && !checking {
            frame.render_widget(
                Paragraph::new(app.t("InventoryEmpty"))
                    .style(palette.foreground(MUTED))
                    .wrap(Wrap { trim: true }),
                list,
            );
        }
        return;
    }

    rows.sort_by_key(|(tool, package)| (!matches!(package, Package::Update(_)), &tool.name));
    let items = rows.len();
    app.diagnostic_offset = app
        .diagnostic_offset
        .min(items.saturating_sub(list.height as usize));
    let body = scroll_content(list, items);
    hit.diagnostics = list;
    let rows = rows
        .into_iter()
        .skip(app.diagnostic_offset)
        .take(body.height as usize)
        .enumerate()
        .map(|(position, (tool, package))| {
            let (release, accent) = match package {
                Package::Update(update) => (
                    format!(
                        "{} → {}",
                        clean(&update.installed),
                        clean(&update.available)
                    ),
                    YELLOW,
                ),
                Package::Installed(version) => (clean(version), ICON_TEAL),
                _ => unreachable!(),
            };
            let width = body.width as usize;
            let release = clip(&release, width / 2);
            let title = clip(
                &clean(&tool.name),
                width.saturating_sub(release.width() + 4),
            );
            let padding = width.saturating_sub(release.width() + title.width());
            let surface = if position % 2 == 0 { PANEL } else { RESULTS };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{title}{}", " ".repeat(padding))),
                Span::styled(
                    release,
                    palette.foreground(accent).add_modifier(Modifier::BOLD),
                ),
            ]))
            .style(palette.colors(INK, surface))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(rows), body);
    scroll_rail(frame, palette, list, app.diagnostic_offset, items, PANEL);
}

pub fn winget_status(app: &App) -> String {
    match &app.winget {
        Some(WinGet::Available(version)) => format!("WinGet {version}"),
        Some(WinGet::Missing) => format!("WinGet: {}", app.t("WinGetMissing")),
        Some(WinGet::TimedOut) => format!("WinGet: {}", app.t("WinGetTimeout")),
        _ => format!("WinGet: {}", app.t("WinGetUnavailable")),
    }
}

const HELP_CATALOG: &[(&str, &[(&str, &str)])] = &[
    (
        "HelpBrowse",
        &[
            ("Tab", "HelpPanels"),
            ("↑ / ↓", "HelpMove"),
            ("← / →", "HelpBranches"),
            ("Space", "HelpSelect"),
            ("Enter", "HelpDetails"),
            ("V", "SelectionTitle"),
            ("F5", "ReloadLabel"),
        ],
    ),
    (
        "Search",
        &[
            ("/, Ctrl+F", "HelpOpenSearch"),
            ("↑↓ + Enter", "HelpJump"),
            ("Tab", "HelpFilter"),
            ("Esc", "HelpCancelSearch"),
        ],
    ),
];

const HELP_APP: &[(&str, &[(&str, &str)])] = &[
    (
        "HelpTabs",
        &[
            ("F1, ?", "Help"),
            ("F2", "CatalogTitle"),
            ("F3", "Diagnostics"),
            ("F4", "Settings"),
            ("Esc", "HelpReturn"),
            ("Q", "Exit"),
        ],
    ),
    (
        "HelpOperations",
        &[
            ("F6", "HelpInstall"),
            ("F7", "HelpUpdates"),
            ("F8", "HelpAppUpdate"),
            ("Enter", "HelpRefresh"),
        ],
    ),
];

fn help_lines(
    app: &App,
    palette: Palette,
    width: u16,
    groups: &[(&str, &[(&'static str, &str)])],
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (title, shortcuts) in groups {
        let label_column = shortcuts
            .iter()
            .map(|(key, _)| key.width())
            .max()
            .unwrap_or(0)
            + 4;
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(
            app.t(title).to_owned(),
            palette.foreground(THUMB).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            "─".repeat(width as usize),
            palette.foreground(BORDER),
        ));
        for &(key, label) in *shortcuts {
            let translation = app.t(label).to_owned();
            let keycap = Span::styled(
                format!(" {key} "),
                palette.colors(INK, FIELD).add_modifier(Modifier::BOLD),
            );
            let padding = label_column - key.width() - 2;
            if translation.width() + label_column <= width as usize {
                lines.push(Line::from(vec![
                    keycap,
                    Span::raw(" ".repeat(padding)),
                    Span::raw(translation),
                ]));
            } else {
                lines.push(Line::from(keycap));
                lines.push(Line::from(format!(
                    " {}",
                    clip(&translation, width.saturating_sub(1) as usize)
                )));
            }
        }
    }
    lines
}

fn help(frame: &mut Frame, app: &mut App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let area = Rect::new(area.x, area.y, area.width.min(106), area.height);
    let inner = section(frame, palette, area, app.t("HelpHeading"), "", PANEL, false);
    let scroll_area = Rect::new(
        inner.x,
        inner.y + u16::from(inner.height >= 12),
        inner.width,
        inner.height.saturating_sub(u16::from(inner.height >= 12)),
    );
    hit.help = area;
    let wide = scroll_area.width >= 84;
    let column_width = if wide {
        (scroll_area.width - 9) / 2
    } else {
        scroll_area.width - 3
    };
    let mut left = help_lines(app, palette, column_width, HELP_CATALOG);
    let mut right = help_lines(app, palette, column_width, HELP_APP);
    if !wide {
        left.push(Line::default());
        left.append(&mut right);
    }
    let total = left.len().max(right.len());
    let offset = app
        .help_offset
        .min(total.saturating_sub(scroll_area.height as usize));
    for (column, lines) in [left, right].into_iter().enumerate() {
        let x = scroll_area.x + 1 + column as u16 * (column_width + 4);
        if !lines.is_empty() {
            frame.render_widget(
                Paragraph::new(lines).scroll((offset as u16, 0)),
                Rect::new(x, scroll_area.y, column_width, scroll_area.height),
            );
        }
    }
    app.help_offset = offset;
    scroll_rail(frame, palette, scroll_area, offset, total, PANEL);
}

fn footer_bar(
    frame: &mut Frame,
    app: &App,
    hit: &mut HitMap,
    palette: Palette,
    area: Rect,
    margin: u16,
) {
    frame.render_widget(Block::default().style(palette.colors(INK, BAR)), area);
    let area = inset(area, margin);
    let y = area.y + u16::from(area.height == 3);
    if area.height == 3 {
        rule(
            frame,
            palette,
            Rect::new(area.x, area.y, area.width, 1),
            BAR,
        );
    }
    let label = app
        .t("SelectedCount")
        .replace("{0}", &app.selected.len().to_string());
    let exit = if app.details_open {
        String::new()
    } else {
        format!("[{}]", app.t("Exit"))
    };
    let exit_x = area.right().saturating_sub(exit.width() as u16);
    let exit_area = Rect::new(exit_x, y, exit.width() as u16, 1);
    let mut actions_x = exit_x;
    if !app.editing && !app.details_open {
        for (show, short, label, operation) in [
            (
                matches!(app.page, Page::Catalog | Page::Diagnostics),
                "F7",
                "JobUpdatesButton",
                Operation::Upgrade,
            ),
            (
                app.page == Page::Catalog,
                "F6",
                "JobInstallButton",
                Operation::Install,
            ),
        ] {
            if show {
                let caption = if area.width >= 72 {
                    format!("[{short} {}]", app.t(label))
                } else {
                    format!("[{short}]")
                };
                let width = caption.width() as u16;
                actions_x = actions_x.saturating_sub(width + 1);
                let button = Rect::new(actions_x, y, width, 1);
                frame.render_widget(
                    Paragraph::new(caption).style(palette.row_on(false, FIELD)),
                    button,
                );
                hit.button(button, Action::Review(operation));
            }
        }
    }
    let label_area = Rect::new(area.x, y, actions_x.saturating_sub(area.x + 1), 1);
    let mut status = Vec::new();
    let show_selection = matches!(app.page, Page::Catalog | Page::Diagnostics);
    if show_selection {
        status.push(Span::styled(
            &label,
            palette
                .foreground(if app.selected.is_empty() { MUTED } else { RED })
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(notice) = &app.notice {
        let mut room = label_area.width as usize;
        if show_selection {
            status.push(Span::styled("  │  ", palette.foreground(BORDER)));
            room = room.saturating_sub(label.width() + 5);
        }
        status.push(Span::styled(
            clip(&clean(notice), room),
            palette.foreground(INK),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(status)), label_area);
    frame.render_widget(
        Paragraph::new(exit).style(palette.row_on(false, FIELD)),
        exit_area,
    );
    hit.button(exit_area, Action::Exit);
    let hints = if app.details_open {
        app.t("DetailKeys")
    } else if app.editing && area.width < 44 {
        app.t("SearchKeysCompact")
    } else if app.editing {
        app.t("SearchKeys")
    } else if app.page == Page::Catalog && area.width >= 79 {
        app.t("CatalogKeys")
    } else if app.page == Page::Catalog && area.width < 40 {
        app.t("CatalogKeysTiny")
    } else if app.page == Page::Catalog {
        app.t("CatalogKeysShort")
    } else if app.page == Page::Settings {
        app.t("SettingsKeys")
    } else if app.page == Page::Help {
        app.t("HelpKeys")
    } else if app.page == Page::Diagnostics {
        app.t(if area.width < 40 {
            "DiagnosticsKeysCompact"
        } else {
            "DiagnosticsKeys"
        })
    } else {
        app.t("NavigationKeys")
    };
    let mut shortcuts = Vec::new();
    for (index, hint) in hints.split('|').enumerate() {
        let hint = hint.trim();
        if index > 0 {
            shortcuts.push(Span::styled("   ", palette.foreground(BORDER)));
        }
        let (key, label) = hint.split_once(' ').unwrap_or((hint, ""));
        shortcuts.push(Span::styled(
            key,
            palette.foreground(INK).add_modifier(Modifier::BOLD),
        ));
        if !label.is_empty() {
            shortcuts.push(Span::styled(format!(" {label}"), palette.foreground(MUTED)));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(shortcuts)),
        Rect::new(area.x, y + 1, area.width, 1),
    );
}

fn scroll_rail(
    frame: &mut Frame,
    palette: Palette,
    body: Rect,
    offset: usize,
    items: usize,
    surface: Color,
) {
    let height = body.height as usize;
    if !has_scroll_rail(body, items) {
        return;
    }
    let minimum = if height >= 6 { 2 } else { 1 };
    let thumb = (height * height / items)
        .max(minimum)
        .min(height.saturating_sub(1).max(1));
    let top = offset * (height - thumb) / (items - height);
    let spans = (0..height)
        .map(|row| {
            if (top..top + thumb).contains(&row) {
                Line::styled("█", palette.colors(THUMB, surface))
            } else {
                Line::styled("░", palette.colors(TRACK, surface))
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(spans),
        Rect::new(body.right() - 1, body.y, 1, body.height),
    );
}

fn has_scroll_rail(area: Rect, items: usize) -> bool {
    area.height > 0 && area.width > 2 && items > area.height as usize
}

fn scroll_content(mut area: Rect, items: usize) -> Rect {
    if has_scroll_rail(area, items) {
        area.width -= 2;
    }
    area
}

fn visible_offset(offset: usize, active: usize, items: usize, height: usize) -> usize {
    let offset = offset.min(items.saturating_sub(height)).min(active);
    offset.max(active.saturating_add(1).saturating_sub(height))
}

fn segment(row: Rect, x: u16, width: u16) -> Rect {
    Rect::new(x, row.y, width.min(row.right().saturating_sub(x)), 1)
}

fn checkbox(check: Check) -> &'static str {
    match check {
        Check::Empty => "[ ]",
        Check::Partial => "[-]",
        Check::Full => "[x]",
    }
}

fn check_span(check: Check, palette: Palette) -> Span<'static> {
    Span::styled(
        checkbox(check),
        palette.foreground(if check == Check::Empty { MUTED } else { RED }),
    )
}
