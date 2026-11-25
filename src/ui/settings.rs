use super::{
    BAR, BORDER, FIELD, HitMap, ICON_TEAL, INK, MUTED, PANEL, Palette, RESULTS, THUMB, clean, clip,
    inset, package_badge, rule, scroll_content, scroll_rail, section, visible_offset,
};
use crate::{
    app::{Action, App, Setting},
    catalog::Language,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

struct Choice {
    label: &'static str,
    names: [&'static str; 2],
    current: bool,
    description: &'static str,
}

fn choice(app: &App, setting: Setting, compact: bool) -> Choice {
    let (label, names, current, description) = match setting {
        Setting::Language => (
            if compact {
                "SettingsLanguageShort"
            } else {
                "Language"
            },
            ["English", "Spanish"],
            app.language == Language::Es,
            "SettingsLanguageDetail",
        ),
        Setting::Theme => (
            "SettingsTheme",
            ["SettingsDark", "SettingsMono"],
            app.monochrome(),
            "SettingsThemeDetail",
        ),
        Setting::Mouse => (
            "SettingsMouse",
            ["SettingsOff", "SettingsOn"],
            app.mouse_enabled(),
            "SettingsMouseDetail",
        ),
        Setting::ReducedMotion => (
            "SettingsMotion",
            ["SettingsOff", "SettingsOn"],
            app.reduced_motion(),
            "SettingsMotionDetail",
        ),
        Setting::CollapseOnStartup => (
            "SettingsTree",
            ["SettingsExpanded", "SettingsCollapsed"],
            app.collapse_on_startup(),
            "SettingsTreeDetail",
        ),
        Setting::PackageBadges => (
            "SettingsBadges",
            ["SettingsOff", "SettingsOn"],
            app.package_badges(),
            "SettingsBadgesDetail",
        ),
    };
    Choice {
        label,
        names,
        current,
        description,
    }
}

pub(super) fn draw(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let side_by_side = area.width >= 94 && area.height >= 10;
    let stacked = !side_by_side && area.width >= 54 && area.height >= 18;
    let height = if stacked {
        area.height - 9
    } else {
        area.height
    }
    .min(14);
    let options_width = if side_by_side {
        (area.width / 3).clamp(32, 57)
    } else {
        area.width.min(72)
    };
    let options = Rect::new(area.x, area.y, options_width, height);
    draw_options(frame, app, hit, palette, options);
    let preview = if side_by_side {
        Rect::new(
            options.right() + 2,
            area.y,
            (area.width - options_width - 2).min(98),
            height,
        )
    } else if stacked {
        Rect::new(
            area.x,
            options.bottom() + 1,
            options.width,
            (area.height - height - 1).min(12),
        )
    } else {
        return;
    };
    draw_preview(frame, app, hit, palette, preview);
}

fn draw_options(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let inner = inset(
        section(frame, palette, area, app.t("Settings"), "", PANEL, true),
        if area.width >= 48 { 2 } else { 1 },
    );
    let items = Setting::ALL.len();
    let top = inner.y + u16::from(inner.height >= 11);
    let height = (inner.bottom() - top).min(items as u16);
    let scroll_area = Rect::new(inner.x, top, inner.width, height);
    let body = scroll_content(scroll_area, items);
    let offset = visible_offset(0, app.settings_cursor, items, height as usize);
    hit.settings = area;
    for (row, setting) in Setting::ALL
        .into_iter()
        .enumerate()
        .skip(offset)
        .take(height as usize)
    {
        let options = choice(app, setting, body.width < 40);
        let active = setting == app.focused_setting();
        let overridden = app.setting_overridden(setting);
        let value = app.t(options.names[usize::from(options.current)]);
        let indicator = if overridden { " ·" } else { " ↔" };
        let width = body.width as usize;
        let name = clip(
            app.t(options.label),
            width.saturating_sub(value.width() + 6),
        );
        let padding = width.saturating_sub(name.width() + value.width() + 5);
        let line = Line::from(vec![
            Span::raw(if active { "> " } else { "  " }),
            Span::raw(name),
            Span::raw(" ".repeat(padding)),
            Span::styled(
                value,
                palette.foreground(if active { INK } else { ICON_TEAL }),
            ),
            Span::styled(
                indicator,
                palette.foreground(if active { INK } else { MUTED }),
            ),
            Span::raw(" "),
        ]);
        let y = top + (row - offset) as u16;
        let area = Rect::new(body.x, y, body.width, 1);
        let value_width = (value.width() as u16 + 4).min(area.width);
        frame.render_widget(Paragraph::new(line).style(palette.row(active)), area);
        if !overridden {
            hit.button(
                Rect::new(area.right() - value_width, y, value_width, 1),
                Action::ChangeSetting(setting),
            );
        }
        hit.button(area, Action::FocusSetting(setting));
    }
    scroll_rail(frame, palette, scroll_area, offset, items, PANEL);
    let divider = top + height;
    if divider + 1 < inner.bottom() {
        rule(
            frame,
            palette,
            Rect::new(inner.x, divider, inner.width, 1),
            PANEL,
        );
        let y = divider + 1;
        let setting = app.focused_setting();
        let description = if app.setting_overridden(setting) {
            "SettingsOverride"
        } else {
            choice(app, setting, false).description
        };
        frame.render_widget(
            Paragraph::new(app.t(description))
                .style(palette.foreground(MUTED))
                .wrap(Wrap { trim: true }),
            Rect::new(inner.x, y, inner.width, inner.bottom() - y),
        );
    }
}

fn draw_preview(frame: &mut Frame, app: &App, hit: &mut HitMap, palette: Palette, area: Rect) {
    let setting = app.focused_setting();
    let options = choice(app, setting, false);
    let inner = inset(
        section(
            frame,
            palette,
            area,
            app.t("SettingsPreview"),
            app.t(options.label),
            PANEL,
            false,
        ),
        1,
    );
    let top = inner.y + u16::from(inner.height >= 9);
    let cases = Rect::new(inner.x, top, inner.width, 7.min(inner.bottom() - top));
    let [current, _, alternate] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(cases);

    for (area, alternative) in [(current, false), (alternate, true)] {
        let value = options.current ^ alternative;
        let title = app.t(if alternative {
            "SettingsAlternative"
        } else {
            "SettingsCurrent"
        });
        let sample = section(frame, palette, area, title, "", RESULTS, false);
        frame.render_widget(
            Paragraph::new(app.t(options.names[usize::from(value)]))
                .style(palette.foreground(THUMB).add_modifier(Modifier::BOLD)),
            Rect::new(sample.x + 1, sample.y, sample.width.saturating_sub(2), 1),
        );
        draw_example(
            frame,
            app,
            palette,
            Rect::new(
                sample.x + 1,
                sample.y + 1,
                sample.width.saturating_sub(2),
                sample.height.saturating_sub(1),
            ),
            setting,
            value,
        );
    }

    if cases.bottom() < inner.bottom() {
        let locked = app.setting_overridden(setting);
        let label = app.t(if locked {
            "SettingsOverrideShort"
        } else {
            "SettingsApplyAlternative"
        });
        let width = (label.width() as u16 + 2).min(alternate.width);
        let button = Rect::new(alternate.x, cases.bottom(), width, 1);
        frame.render_widget(
            Paragraph::new(format!(" {label}"))
                .style(palette.colors(if locked { MUTED } else { INK }, FIELD)),
            button,
        );
        if !locked {
            hit.button(button, Action::ChangeSetting(setting));
        }
    }
}

fn draw_example(
    frame: &mut Frame,
    app: &App,
    palette: Palette,
    area: Rect,
    setting: Setting,
    value: bool,
) {
    let tool = app.catalog.tools.first();
    let name = || {
        tool.map(|tool| clean(&tool.name))
            .unwrap_or_else(|| app.t("ToolListTitle").to_owned())
    };
    let lines = match setting {
        Setting::Language => {
            let language = if value { Language::Es } else { Language::En };
            vec![
                Line::from(format!(
                    "{} │ {}",
                    app.t_in(language, "CategoriesTitle"),
                    app.t_in(language, "ToolListTitle")
                )),
                Line::from(format!("/ {}", app.t_in(language, "Search"))),
                Line::from(app.t_in(language, "SelectedCount").replace("{0}", "0")),
            ]
        }
        Setting::Theme => {
            let theme = Palette {
                mono: value || app.setting_overridden(Setting::Theme),
            };
            if theme.mono {
                frame.buffer_mut().set_style(area, Style::reset());
            }
            let inner = section(
                frame,
                theme,
                area,
                app.t("ToolListTitle"),
                "",
                RESULTS,
                true,
            );
            if inner.height > 0 {
                frame.render_widget(
                    Paragraph::new(format!("> [ ] {}", name())).style(theme.row(true)),
                    inner,
                );
            }
            return;
        }
        Setting::Mouse => {
            let (first, second) = if value {
                ("SettingsMouseClick", "SettingsMouseWheel")
            } else {
                ("SettingsMouseArrows", "SettingsMouseEnter")
            };
            vec![Line::from(app.t(first)), Line::from(app.t(second))]
        }
        Setting::ReducedMotion => {
            let active = palette.jump();
            let middle = if value {
                active
            } else {
                palette.colors(MUTED, BAR)
            };
            vec![
                Line::from(vec![
                    Span::styled(" ● ", active),
                    Span::styled(" → ", palette.foreground(BORDER)),
                    Span::styled(if value { " ● " } else { " ○ " }, middle),
                    Span::styled(" → ", palette.foreground(BORDER)),
                    Span::styled(" ● ", active),
                ]),
                Line::from(app.t(if value {
                    "SettingsMotionSteady"
                } else {
                    "SettingsMotionPulse"
                })),
            ]
        }
        Setting::CollapseOnStartup => {
            let root = app
                .catalog
                .categories
                .iter()
                .find(|category| category.parent.is_none() && !category.children.is_empty());
            let mut lines = Vec::new();
            if let Some(root) = root {
                lines.push(Line::from(format!(
                    "{} {}",
                    if value { "+" } else { "-" },
                    clean(root.name.get(app.language))
                )));
                if !value {
                    for &index in root.children.iter().take(2) {
                        let guide = if root.children.last() == Some(&index) {
                            "└"
                        } else {
                            "├"
                        };
                        lines.push(Line::from(format!(
                            "  {guide}─ {}",
                            clean(app.catalog.categories[index].name.get(app.language))
                        )));
                    }
                }
            }
            lines
        }
        Setting::PackageBadges => {
            let (status, accent) = tool.map_or(("", MUTED), |tool| package_badge(app, tool));
            let width = area.width as usize;
            let status = if value {
                clip(status, width / 2)
            } else {
                String::new()
            };
            let room = width.saturating_sub(5 + status.width() + usize::from(value));
            let name = clip(&name(), room);
            let pad = width.saturating_sub(4 + name.width() + status.width());
            vec![
                Line::from(vec![
                    Span::raw(format!("[ ] {name}{}", " ".repeat(pad))),
                    Span::styled(status, palette.foreground(accent)),
                ])
                .style(palette.colors(INK, FIELD)),
            ]
        }
    };
    frame.render_widget(Paragraph::new(lines), area);
}
