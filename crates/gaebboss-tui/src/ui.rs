//! Rendering: the pane layout and one painter per pane.

use std::ops::RangeInclusive;

use gaebboss_core::validate::{Finding, Severity};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::{App, Pane};
use crate::outline::{Row, RowKind};
use crate::yank::TARGETS;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Panes {
    pub tree: Rect,
    pub inspector: Rect,
    pub source: Rect,
    pub status: Rect,
}

/// The two pane dividers as percentages: the tree column's width and the
/// inspector's share of the right column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Splits {
    pub tree: u16,
    pub right_top: u16,
}

impl Default for Splits {
    fn default() -> Splits {
        Splits {
            tree: 40,
            right_top: 60,
        }
    }
}

pub const SPLIT_STEP: u16 = 5;
pub const TREE_SPLIT: RangeInclusive<u16> = 15..=70;
pub const RIGHT_TOP_SPLIT: RangeInclusive<u16> = 20..=85;
/// Spinner frames the tree shows while the outline loads, one per tick.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// Parse warnings the bill's inspector lists before summarizing the rest.
const WARNINGS_SHOWN: usize = 50;

pub fn panes(area: Rect, splits: Splits) -> Panes {
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let columns = Layout::horizontal([
        Constraint::Percentage(splits.tree),
        Constraint::Percentage(100 - splits.tree),
    ])
    .split(rows[0]);
    let right = Layout::vertical([
        Constraint::Percentage(splits.right_top),
        Constraint::Percentage(100 - splits.right_top),
    ])
    .split(columns[1]);
    Panes {
        tree: columns[0],
        inspector: right[0],
        source: right[1],
        status: rows[1],
    }
}

pub fn draw(app: &App, frame: &mut Frame) {
    let split = panes(frame.area(), app.splits);
    draw_tree(app, frame, split.tree);
    draw_inspector(app, frame, split.inspector);
    draw_source(app, frame, split.source);
    draw_status(app, frame, split.status);
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let block = Block::bordered().title(title);
    match focused {
        true => block.title_style(Style::default().add_modifier(Modifier::BOLD)),
        false => block,
    }
}

fn severity_style(severity: Severity) -> Style {
    let color = match severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Blue,
    };
    Style::default().fg(color)
}

fn severity_glyph(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => " ✗",
        Severity::Warning => " !",
        Severity::Info => " ·",
    }
}

/// The tree line for a row, without indent or glyph.
pub fn row_label(app: &App, row: &Row) -> String {
    match row.kind {
        RowKind::Bill => {
            let phase = app
                .doc
                .phase()
                .map(|p| format!(" X{}", p.code()))
                .unwrap_or_default();
            format!("{} · GAEB {}{phase}", row.label, app.doc.version().as_str())
        }
        RowKind::Category | RowKind::Item | RowKind::Markup => format!("{} {}", row.oz, row.label),
    }
}

fn draw_tree(app: &App, frame: &mut Frame, area: Rect) {
    let block = pane_block("Tree".to_string(), app.focus == Pane::Tree);
    let height = usize::from(area.height.saturating_sub(2));
    let rows = app.visible_rows();
    let position = rows.iter().position(|r| *r == app.selected).unwrap_or(0);
    let offset = position.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .skip(offset)
        .take(height)
        .filter_map(|index| {
            let row = app.outline.row(*index)?;
            let glyph = match (app.has_children(*index), app.is_expanded(*index)) {
                (false, _) => "  ",
                (true, true) => "▾ ",
                (true, false) => "▸ ",
            };
            let text = format!("{}{glyph}{}", "  ".repeat(row.depth), row_label(app, row));
            let style = match *index == app.selected {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let mut spans = vec![Span::styled(text, style)];
            spans.extend(
                row.severity()
                    .map(|s| Span::styled(severity_glyph(s), severity_style(s))),
            );
            Some(Line::from(spans))
        })
        .collect();
    let mut lines = lines;
    if app.loading {
        let spinner = SPINNER[(app.ticks % SPINNER.len() as u64) as usize];
        lines.push(Line::styled(
            format!("  {spinner} validating, reading texts"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn finding_line(finding: &Finding) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:?} {:?}", finding.severity, finding.code),
            severity_style(finding.severity),
        ),
        Span::raw(format!(": {}", finding.message)),
    ])
}

fn markdown_line(line: &str) -> Line<'static> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    match line.starts_with('#') {
        true => Line::styled(line.to_string(), bold),
        false => Line::raw(line.to_string()),
    }
}

/// Everything the inspector shows for the selected row, top to bottom.
pub fn inspector_lines(app: &App) -> Vec<Line<'static>> {
    let Some(row) = app.outline.row(app.selected) else {
        return Vec::new();
    };
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let width = row
        .fields
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line<'static>> = row
        .fields
        .iter()
        .map(|(key, value)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:<width$}  "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(value.clone()),
            ])
        })
        .collect();
    if row.kind == RowKind::Item && !row.label.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(row.label.clone(), bold));
    }
    let body = match app.markdown {
        true => &row.markdown,
        false => &row.long_text,
    };
    if !body.trim().is_empty() {
        lines.push(Line::raw(""));
        lines.extend(body.lines().map(|l| match app.markdown {
            true => markdown_line(l),
            false => Line::raw(l.to_string()),
        }));
    }
    if !row.findings.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Findings ({})", row.findings.len()),
            bold,
        ));
        lines.extend(row.findings.iter().map(finding_line));
    }
    let warnings = &app.outline.warnings;
    if row.kind == RowKind::Bill && !warnings.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Parse warnings ({})", warnings.len()),
            bold,
        ));
        lines.extend(
            warnings
                .iter()
                .take(WARNINGS_SHOWN)
                .map(|w| Line::raw(w.clone())),
        );
        if warnings.len() > WARNINGS_SHOWN {
            lines.push(Line::raw(format!(
                "… {} more",
                warnings.len() - WARNINGS_SHOWN
            )));
        }
    }
    lines
}

fn draw_inspector(app: &App, frame: &mut Frame, area: Rect) {
    let Some(row) = app.outline.row(app.selected) else {
        return;
    };
    let kind = match row.kind {
        RowKind::Bill => "Bill".to_string(),
        RowKind::Category => format!("Category {}", row.oz),
        RowKind::Item => format!("Item {}", row.oz),
        RowKind::Markup => format!("Markup {}", row.oz),
    };
    let mode = match app.markdown {
        true => " · Markdown",
        false => "",
    };
    let block = pane_block(
        format!("Inspector · {kind}{mode}"),
        app.focus == Pane::Inspector,
    );
    let paragraph = Paragraph::new(Text::from(inspector_lines(app)))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.inspector_scroll, 0));
    frame.render_widget(paragraph, area);
}

fn draw_source(app: &App, frame: &mut Frame, area: Rect) {
    let range = app.outline.source_range(&app.doc, app.selected);
    let title = format!("XML {:#x}..{:#x}", range.start, range.end);
    let block = pane_block(title, app.focus == Pane::Source);
    let height = usize::from(area.height.saturating_sub(2));
    let bytes = app.doc.bytes();
    let mut lines: Vec<Line> = app
        .source
        .lines
        .iter()
        .skip(app.source_scroll)
        .take(height)
        .map(|r| {
            let text = String::from_utf8_lossy(bytes.get(r.clone()).unwrap_or(&[]));
            Line::raw(text.trim_end_matches('\r').to_string())
        })
        .collect();
    let shown_to_end = app.source_scroll + height >= app.source.lines.len();
    if app.source.truncated > 0 && shown_to_end {
        lines.push(Line::styled(
            format!("… {} more bytes (gaebboss hex)", app.source.truncated),
            Style::default().fg(Color::DarkGray),
        ));
    }
    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// The status bar: the search prompt, the yank menu, a toast, or the
/// breadcrumb with the key hints.
pub fn status_text(app: &App) -> String {
    if app.search.editing {
        return format!("/{}█", app.search.query);
    }
    if app.yanking {
        let choices: Vec<String> = TARGETS
            .iter()
            .map(|t| format!("[{}] {}", t.key(), t.name()))
            .collect();
        return format!("yank: {}", choices.join("  "));
    }
    if let Some(toast) = &app.toast {
        return toast.text.clone();
    }
    let crumb = app
        .outline
        .row(app.selected)
        .map(|r| match r.oz.is_empty() {
            true => "Bill".to_string(),
            false => r.oz.clone(),
        })
        .unwrap_or_default();
    let hits = match app.search.hits.len() {
        0 => String::new(),
        n => format!(" · [{}/{n}]", app.search.current + 1),
    };
    format!(
        "{} · {crumb}{hits} · [/] search  []] finding  [m] markdown  [y] yank  [q] quit",
        app.title
    )
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    frame.render_widget(Paragraph::new(status_text(app)), area);
}
