//! Rendering: the layout, one painter per pane, and the copy and key popups.

use std::ops::RangeInclusive;

use gaebboss_core::boq::ItemKind;
use gaebboss_core::decimal::Decimal;
use gaebboss_core::validate::{Finding, Severity};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};

use crate::app::{App, Pane};
use crate::outline::{BillFacts, CategoryFacts, Facts, ItemFacts, MarkupFacts, ROOT, Row, RowKind};
use crate::theme::Theme;
use crate::yank::{TARGETS, yank_text};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Panes {
    pub header: Rect,
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
            tree: 42,
            right_top: 62,
        }
    }
}

pub const SPLIT_STEP: u16 = 5;
pub const TREE_SPLIT: RangeInclusive<u16> = 15..=70;
pub const RIGHT_TOP_SPLIT: RangeInclusive<u16> = 20..=85;
/// Spinner frames shown while the outline loads, one per tick.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// Parse warnings the bill lists before summarizing the rest.
const WARNINGS_SHOWN: usize = 50;
/// The narrowest label the tree keeps before it drops the amount column.
const LABEL_ROOM: usize = 18;

pub fn panes(area: Rect, splits: Splits) -> Panes {
    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [tree, right] = Layout::horizontal([
        Constraint::Percentage(splits.tree),
        Constraint::Percentage(100 - splits.tree),
    ])
    .areas(body);
    let [inspector, source] = Layout::vertical([
        Constraint::Percentage(splits.right_top),
        Constraint::Percentage(100 - splits.right_top),
    ])
    .areas(right);
    Panes {
        header,
        tree,
        inspector,
        source,
        status,
    }
}

pub fn draw(app: &App, frame: &mut Frame) {
    let split = panes(frame.area(), app.splits);
    draw_header(app, frame, split.header);
    draw_tree(app, frame, split.tree);
    draw_inspector(app, frame, split.inspector);
    draw_source(app, frame, split.source);
    draw_status(app, frame, split.status);
    if app.yanking {
        draw_yank(app, frame);
    }
    if app.help {
        draw_help(app, frame);
    }
}

/// `1234567.5` as `1,234,567.5`; the scale the file wrote is kept.
pub fn grouped(value: Decimal) -> String {
    let text = value.to_string();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text.as_str()),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let mut out = String::new();
    for (index, c) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match fraction.is_empty() {
        true => format!("{sign}{out}"),
        false => format!("{sign}{out}.{fraction}"),
    }
}

fn amount(value: Option<Decimal>) -> String {
    value.map(grouped).unwrap_or_default()
}

/// `text` cut to `width` columns, ending in `…` when cut.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn pane(app: &App, title: String, focused: bool) -> Block<'static> {
    let theme = &app.theme;
    let (border, heading) = match focused {
        true => (theme.fg(theme.accent), theme.bold().fg(theme.accent)),
        false => (theme.fg(theme.border), theme.bold().fg(Color::Reset)),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(format!(" {title} "), heading))
}

fn severity_color(theme: &Theme, severity: Severity) -> Color {
    match severity {
        Severity::Error => theme.error,
        Severity::Warning => theme.warning,
        Severity::Info => theme.info,
    }
}

fn severity_glyph(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "✗",
        Severity::Warning => "!",
        Severity::Info => "i",
    }
}

pub fn kind_label(kind: ItemKind) -> String {
    match kind {
        ItemKind::Standard => "Standard".to_string(),
        ItemKind::LumpSum => "Lump sum".to_string(),
        ItemKind::ProvisionalWithTotal => "Provisional".to_string(),
        ItemKind::ProvisionalWithoutTotal => "Provisional, no total".to_string(),
        ItemKind::Alternative { group, serial } => format!("Alternative {group}.{serial}"),
        ItemKind::Base { group } => format!("Base of group {group}"),
        ItemKind::Hourly => "Hourly".to_string(),
        ItemKind::Key => "Key item".to_string(),
        ItemKind::FreeQuantity => "Free quantity".to_string(),
        ItemKind::NotApplicable => "Not applicable".to_string(),
    }
}

fn bill_facts(app: &App) -> Option<&BillFacts> {
    let Facts::Bill(bill) = &app.outline.row(ROOT)?.facts else {
        return None;
    };
    Some(bill)
}

fn header_left(app: &App) -> Vec<Span<'static>> {
    let theme = &app.theme;
    let Some(bill) = bill_facts(app) else {
        return Vec::new();
    };
    let mut spans = vec![
        Span::styled(" gaebboss ", theme.chip(theme.accent)),
        Span::raw("  "),
        Span::styled(app.title.clone(), theme.bold()),
        Span::styled(
            format!("   GAEB {} {}", bill.version, bill.phase),
            theme.muted(),
        ),
    ];
    if !app.loading {
        spans.push(Span::styled(
            format!("   {} items in {} categories", bill.items, bill.categories),
            theme.muted(),
        ));
    }
    spans
}

fn header_right(app: &App) -> Vec<Span<'static>> {
    let theme = &app.theme;
    if app.loading {
        let spinner = SPINNER[(app.ticks % SPINNER.len() as u64) as usize];
        return vec![Span::styled(
            format!("{spinner} checking the bill "),
            theme.muted(),
        )];
    }
    let Some(bill) = bill_facts(app) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    if let Some(total) = bill.declared.or(bill.computed) {
        spans.push(Span::styled("Σ ", theme.muted()));
        spans.push(Span::styled(grouped(total), theme.bold()));
        spans.push(Span::styled(
            format!(" {}   ", bill.currency),
            theme.muted(),
        ));
    }
    let counts = [
        (Severity::Error, bill.errors),
        (Severity::Warning, bill.warnings),
        (Severity::Info, bill.notes),
    ];
    if counts.iter().all(|(_, n)| *n == 0) {
        spans.push(Span::styled("✓ no findings ", theme.fg(theme.ok)));
        return spans;
    }
    spans.extend(
        counts
            .into_iter()
            .filter(|(_, n)| *n > 0)
            .map(|(severity, n)| {
                Span::styled(
                    format!("{} {n} ", severity_glyph(severity)),
                    theme
                        .fg(severity_color(theme, severity))
                        .add_modifier(Modifier::BOLD),
                )
            }),
    );
    spans
}

/// One line with `left` at the start and `right` flush with the end.
fn spread(frame: &mut Frame, area: Rect, left: Vec<Span<'static>>, right: Vec<Span<'static>>) {
    let right = Line::from(right);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right.width() as u16)])
            .areas(area);
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        right_area,
    );
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    spread(frame, area, header_left(app), header_right(app));
}

/// The figure the tree shows at a row's right edge.
fn row_amount(row: &Row) -> String {
    match &row.facts {
        Facts::Bill(_) => String::new(),
        Facts::Category(c) => amount(c.declared.or(c.computed)),
        Facts::Markup(m) => amount(m.total),
        Facts::Item(i) => match (i.total, i.quantity) {
            (Some(total), _) => grouped(total),
            (None, Some(quantity)) => format!("{} {}", grouped(quantity), i.unit),
            (None, None) => String::new(),
        },
    }
}

/// `text` as spans with every case-insensitive occurrence of `query` marked.
fn marked(text: &str, query: &str, base: Style, mark: Style) -> Vec<Span<'static>> {
    let lower = text.to_lowercase();
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || lower.len() != text.len() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for (start, found) in lower.match_indices(&needle) {
        let end = start + found.len();
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        spans.push(Span::styled(text[at..start].to_string(), base));
        spans.push(Span::styled(text[start..end].to_string(), mark));
        at = end;
    }
    spans.push(Span::styled(text[at..].to_string(), base));
    spans
}

/// Indent guides for a row: a rule under every ancestor that has siblings
/// still to come, blank under the last ones and under the bill.
fn guides(app: &App, index: usize) -> String {
    let mut ancestors = app.outline.ancestors(index);
    ancestors.retain(|a| *a != index);
    ancestors
        .iter()
        .rev()
        .map(|a| match app.outline.is_last_child(*a) {
            true => "  ",
            false => "│ ",
        })
        .collect()
}

fn tree_line(app: &App, index: usize, width: usize) -> Option<Line<'static>> {
    let theme = &app.theme;
    let row = app.outline.row(index)?;
    let selected = index == app.selected;
    let band = match selected {
        true => Style::default().bg(theme.selection),
        false => Style::default(),
    };
    let gutter = match selected {
        true => Span::styled("▌", band.fg(theme.accent)),
        false => Span::styled(" ", band),
    };
    let severity = match row.severity() {
        Some(s) => Span::styled(
            severity_glyph(s),
            band.fg(severity_color(theme, s))
                .add_modifier(Modifier::BOLD),
        ),
        None => Span::styled(" ", band),
    };
    let guides = guides(app, index);
    let fold = match (app.has_children(index), app.is_expanded(index)) {
        (false, _) => "  ",
        (true, true) => "▾ ",
        (true, false) => "▸ ",
    };
    let oz = match row.kind() {
        RowKind::Bill => String::new(),
        RowKind::Category | RowKind::Item | RowKind::Markup => format!("{} ", row.own_oz()),
    };
    let used = 2 + guides.chars().count() + fold.chars().count() + oz.chars().count();
    let figure = Some(row_amount(row))
        .filter(|f| width.saturating_sub(used + f.chars().count() + 1) >= LABEL_ROOM)
        .unwrap_or_default();
    let room = width.saturating_sub(used + figure.chars().count() + 1);
    let label = fit(&row.label, room);
    let label_style = match (row.kind(), selected) {
        (_, true) | (RowKind::Bill | RowKind::Category, _) => band.add_modifier(Modifier::BOLD),
        (RowKind::Item | RowKind::Markup, false) => band,
    };
    let query = match app.search.hits.contains(&index) {
        true => app.search.query.as_str(),
        false => "",
    };
    let mark = band
        .fg(theme.accent)
        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
    let pad = width.saturating_sub(used + label.chars().count() + figure.chars().count());
    let mut spans = vec![
        gutter,
        severity,
        Span::styled(guides, band.fg(theme.border)),
        Span::styled(fold, band.fg(theme.muted)),
        Span::styled(oz, band.fg(theme.muted)),
    ];
    spans.extend(marked(&label, query, label_style, mark));
    spans.push(Span::styled(" ".repeat(pad), band));
    spans.push(Span::styled(figure, band.fg(theme.muted)));
    Some(Line::from(spans))
}

fn scrollbar(app: &App, frame: &mut Frame, area: Rect, length: usize, position: usize) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let visible = usize::from(inner.height);
    if length <= visible {
        return;
    }
    let mut state = ScrollbarState::new(length.saturating_sub(visible)).position(position);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .track_style(app.theme.fg(app.theme.border))
        .thumb_symbol("┃")
        .thumb_style(app.theme.fg(app.theme.muted));
    frame.render_stateful_widget(bar, inner, &mut state);
}

fn draw_tree(app: &App, frame: &mut Frame, area: Rect) {
    let block = pane(app, "Outline".to_string(), app.focus == Pane::Tree);
    let height = usize::from(area.height.saturating_sub(2));
    let width = usize::from(area.width.saturating_sub(2));
    let rows = app.visible_rows();
    let position = rows.iter().position(|r| *r == app.selected).unwrap_or(0);
    let offset = position.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .skip(offset)
        .take(height)
        .filter_map(|index| tree_line(app, *index, width))
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
    scrollbar(app, frame, area, rows.len(), offset);
}

/// A section heading: bold muted title, then a rule to the edge.
fn section(theme: &Theme, title: &str, width: usize) -> Line<'static> {
    let rule = "─".repeat(width.saturating_sub(title.chars().count() + 1));
    Line::from(vec![
        Span::styled(
            format!("{title} "),
            theme.muted().add_modifier(Modifier::BOLD),
        ),
        Span::styled(rule, theme.fg(theme.border)),
    ])
}

fn fact(theme: &Theme, key: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("{key:<11}"), theme.muted())];
    spans.extend(value);
    Line::from(spans)
}

/// `declared` against `computed`: a tick when they agree, the computed sum
/// when they do not.
fn checked(
    theme: &Theme,
    declared: Option<Decimal>,
    computed: Option<Decimal>,
) -> Vec<Span<'static>> {
    match (declared, computed) {
        (Some(d), Some(c)) if d == c => vec![
            Span::styled(grouped(d), theme.bold()),
            Span::styled("  ✓", theme.fg(theme.ok)),
        ],
        (Some(d), Some(c)) => vec![
            Span::styled(grouped(d), theme.bold().fg(theme.error)),
            Span::styled(
                format!("  ✗ adds up to {}", grouped(c)),
                theme.fg(theme.error),
            ),
        ],
        (Some(d), None) => vec![Span::styled(grouped(d), theme.bold())],
        (None, Some(c)) => vec![
            Span::styled(grouped(c), theme.bold()),
            Span::styled("  computed", theme.muted()),
        ],
        (None, None) => vec![Span::styled("not priced", theme.muted())],
    }
}

fn bill_lines(app: &App, row: &Row, bill: &BillFacts) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let title = match bill.project.is_empty() {
        true => row.label.clone(),
        false => bill.project.clone(),
    };
    let mut lines = vec![Line::styled(title, theme.bold())];
    lines.extend(
        [&bill.project_label, &bill.boq_label]
            .into_iter()
            .filter(|t| !t.is_empty())
            .map(|t| Line::styled(t.clone(), theme.muted())),
    );
    lines.push(Line::raw(""));
    lines.push(fact(theme, "Bill", vec![Span::raw(row.label.clone())]));
    lines.push(fact(
        theme,
        "Format",
        vec![Span::raw(format!(
            "GAEB DA XML {} {}",
            bill.version, bill.phase
        ))],
    ));
    if app.loading {
        return lines;
    }
    lines.push(fact(
        theme,
        "Contents",
        vec![Span::raw(format!(
            "{} categories, {} items",
            bill.categories, bill.items
        ))],
    ));
    if bill.markups > 0 {
        lines.push(fact(
            theme,
            "Markups",
            vec![Span::raw(bill.markups.to_string())],
        ));
    }
    lines.push(fact(
        theme,
        "OZ shape",
        vec![Span::raw(bill.breakdown.clone())],
    ));
    if !bill.currency.is_empty() {
        lines.push(fact(
            theme,
            "Currency",
            vec![Span::raw(bill.currency.clone())],
        ));
    }
    lines.push(fact(
        theme,
        "Total",
        checked(theme, bill.declared, bill.computed),
    ));
    lines
}

fn category_lines(app: &App, row: &Row, category: &CategoryFacts) -> Vec<Line<'static>> {
    let theme = &app.theme;
    vec![
        Line::styled(row.label.clone(), theme.bold()),
        Line::raw(""),
        fact(theme, "Items", vec![Span::raw(category.items.to_string())]),
        fact(
            theme,
            "Total",
            checked(theme, category.declared, category.computed),
        ),
        fact(
            theme,
            "ID",
            vec![Span::styled(row.id.clone(), theme.muted())],
        ),
    ]
}

/// `quantity unit × unit price less discount % = total`, the total checked
/// against quantity times price less the discount.
fn price_line(theme: &Theme, item: &ItemFacts) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(quantity) = item.quantity {
        spans.push(Span::styled(grouped(quantity), theme.bold()));
        spans.push(Span::styled(format!(" {}", item.unit), theme.muted()));
    }
    if item.quantity.is_none() && !item.unit.is_empty() {
        spans.push(Span::styled(format!("per {}", item.unit), theme.muted()));
    }
    let Some(price) = item.unit_price else {
        spans.push(Span::styled("   not priced", theme.muted()));
        return Line::from(spans);
    };
    spans.push(Span::styled("  ×  ", theme.muted()));
    spans.push(Span::raw(grouped(price)));
    if let Some(discount) = item.discount {
        spans.push(Span::styled("  less  ", theme.muted()));
        spans.push(Span::raw(format!("{} %", grouped(discount))));
    }
    if item.total.is_none() && item.computed.is_none() {
        return Line::from(spans);
    }
    spans.push(Span::styled("  =  ", theme.muted()));
    spans.extend(checked(theme, item.total, item.computed));
    Line::from(spans)
}

fn item_lines(app: &App, row: &Row, item: &ItemFacts) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let mut chips = vec![Span::styled(
        format!(" {} ", kind_label(item.kind)),
        Style::default()
            .bg(theme.border)
            .add_modifier(Modifier::BOLD),
    )];
    chips.push(match item.counts {
        true => Span::styled("  counts towards the total", theme.muted()),
        false => Span::styled("  left out of the total", theme.fg(theme.warning)),
    });
    if item.not_offered {
        chips.push(Span::styled("  not offered", theme.fg(theme.warning)));
    }
    let mut meta = vec![Span::styled(format!("ID {}", row.id), theme.muted())];
    if let Some(vat) = item.vat {
        meta.push(Span::styled(
            format!("   VAT {} %", grouped(vat)),
            theme.muted(),
        ));
    }
    vec![
        Line::styled(row.label.clone(), theme.bold()),
        Line::from(chips),
        Line::raw(""),
        price_line(theme, item),
        Line::from(meta),
    ]
}

fn markup_lines(app: &App, markup: &MarkupFacts) -> Vec<Line<'static>> {
    let theme = &app.theme;
    vec![
        Line::styled("Markup item", theme.bold()),
        Line::raw(""),
        fact(theme, "Percent", vec![Span::raw(amount(markup.percent))]),
        fact(theme, "On", vec![Span::raw(amount(markup.base))]),
        fact(
            theme,
            "Total",
            vec![Span::styled(amount(markup.total), theme.bold())],
        ),
    ]
}

fn markdown_line(theme: &Theme, line: &str) -> Line<'static> {
    match line.starts_with('#') {
        true => Line::styled(line.to_string(), theme.bold().fg(theme.accent)),
        false => Line::raw(line.to_string()),
    }
}

fn finding_line(theme: &Theme, finding: &Finding) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{} ", severity_glyph(finding.severity)),
            theme
                .fg(severity_color(theme, finding.severity))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(finding.message.clone()),
        Span::styled(format!("  {:?}", finding.code), theme.muted()),
    ])
}

/// Everything the inspector shows for the selected row, top to bottom, with
/// rules `width` columns wide.
pub fn inspector_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let Some(row) = app.outline.row(app.selected) else {
        return Vec::new();
    };
    let mut lines = match &row.facts {
        Facts::Bill(bill) => bill_lines(app, row, bill),
        Facts::Category(category) => category_lines(app, row, category),
        Facts::Item(item) => item_lines(app, row, item),
        Facts::Markup(markup) => markup_lines(app, markup),
    };
    let (title, body) = match app.markdown {
        true => ("Long text as Markdown", &row.markdown),
        false => ("Long text", &row.long_text),
    };
    if !body.trim().is_empty() {
        lines.push(Line::raw(""));
        lines.push(section(theme, title, width));
        lines.extend(body.trim().lines().map(|l| match app.markdown {
            true => markdown_line(theme, l),
            false => Line::raw(l.to_string()),
        }));
    }
    if !row.findings.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section(theme, "Findings", width));
        lines.extend(row.findings.iter().map(|f| finding_line(theme, f)));
    }
    let warnings = &app.outline.warnings;
    if row.kind() != RowKind::Bill || warnings.is_empty() {
        return lines;
    }
    lines.push(Line::raw(""));
    lines.push(section(
        theme,
        &format!("Repaired while reading ({})", warnings.len()),
        width,
    ));
    lines.extend(
        warnings
            .iter()
            .take(WARNINGS_SHOWN)
            .map(|w| Line::styled(w.clone(), theme.muted())),
    );
    if warnings.len() > WARNINGS_SHOWN {
        lines.push(Line::styled(
            format!("and {} more", warnings.len() - WARNINGS_SHOWN),
            theme.muted(),
        ));
    }
    lines
}

fn draw_inspector(app: &App, frame: &mut Frame, area: Rect) {
    let Some(row) = app.outline.row(app.selected) else {
        return;
    };
    let title = match row.kind() {
        RowKind::Bill => "Bill".to_string(),
        RowKind::Category => format!("Category {}", row.oz),
        RowKind::Item => format!("Item {}", row.oz),
        RowKind::Markup => format!("Markup item {}", row.oz),
    };
    let block = pane(app, title, app.focus == Pane::Inspector).padding(Padding::horizontal(1));
    let width = usize::from(area.width.saturating_sub(4));
    let paragraph = Paragraph::new(Text::from(inspector_lines(app, width)))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.inspector_scroll, 0));
    frame.render_widget(paragraph, area);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Xml {
    Text,
    Name,
    Attributes,
    Value(char),
}

fn flush(buffer: &mut String, style: Style, spans: &mut Vec<Span<'static>>) {
    if buffer.is_empty() {
        return;
    }
    spans.push(Span::styled(std::mem::take(buffer), style));
}

/// One line of XML with tag names, attribute names and values colored.
pub fn xml_line(theme: &Theme, line: &str) -> Line<'static> {
    let punctuation = theme.muted();
    let name = theme.fg(theme.info);
    let attribute = theme.fg(theme.attribute);
    let value = theme.fg(theme.ok);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buffer = String::new();
    let mut state = Xml::Text;
    for c in line.chars() {
        match (state, c) {
            (Xml::Text, '<') => {
                flush(&mut buffer, Style::default(), &mut spans);
                spans.push(Span::styled("<", punctuation));
                state = Xml::Name;
            }
            (Xml::Name, '/' | '?' | '!') if buffer.is_empty() => {
                spans.push(Span::styled(c.to_string(), punctuation));
            }
            (Xml::Name, '>') | (Xml::Attributes, '>') => {
                let style = match state {
                    Xml::Name => name,
                    Xml::Text | Xml::Attributes | Xml::Value(_) => attribute,
                };
                flush(&mut buffer, style, &mut spans);
                spans.push(Span::styled(">", punctuation));
                state = Xml::Text;
            }
            (Xml::Name, c) if c.is_whitespace() => {
                flush(&mut buffer, name, &mut spans);
                buffer.push(c);
                state = Xml::Attributes;
            }
            (Xml::Attributes, '"' | '\'') => {
                flush(&mut buffer, attribute, &mut spans);
                buffer.push(c);
                state = Xml::Value(c);
            }
            (Xml::Attributes, '=' | '/' | '?') => {
                flush(&mut buffer, attribute, &mut spans);
                spans.push(Span::styled(c.to_string(), punctuation));
            }
            (Xml::Value(quote), c) if c == quote => {
                buffer.push(c);
                flush(&mut buffer, value, &mut spans);
                state = Xml::Attributes;
            }
            (Xml::Text | Xml::Name | Xml::Attributes | Xml::Value(_), c) => buffer.push(c),
        }
    }
    let rest = match state {
        Xml::Text => Style::default(),
        Xml::Name => name,
        Xml::Attributes => attribute,
        Xml::Value(_) => value,
    };
    flush(&mut buffer, rest, &mut spans);
    Line::from(spans)
}

fn draw_source(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let range = app.outline.source_range(&app.doc, app.selected);
    let block = pane(app, "XML".to_string(), app.focus == Pane::Source)
        .title(
            Line::styled(
                format!(" {:#x}..{:#x} ", range.start, range.end),
                theme.muted(),
            )
            .right_aligned(),
        )
        .padding(Padding::horizontal(1));
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
            xml_line(theme, text.trim_end_matches('\r'))
        })
        .collect();
    let shown_to_end = app.source_scroll + height >= app.source.lines.len();
    if app.source.truncated > 0 && shown_to_end {
        lines.push(Line::styled(
            format!(
                "{} more bytes: gaebboss hex shows them",
                app.source.truncated
            ),
            theme.muted(),
        ));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
    scrollbar(app, frame, area, app.source.lines.len(), app.source_scroll);
}

fn hint(theme: &Theme, key: &str, label: &str) -> [Span<'static>; 2] {
    [
        Span::styled(key.to_string(), theme.key()),
        Span::styled(format!(" {label}   "), theme.muted()),
    ]
}

fn mode(app: &App) -> &'static str {
    if app.help {
        return "keys";
    }
    if app.search.editing {
        return "search";
    }
    if app.yanking {
        return "copy";
    }
    "browse"
}

fn search_status(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    let left = vec![
        Span::styled("/", theme.key()),
        Span::raw(app.search.query.clone()),
        Span::styled("█", theme.fg(theme.accent)),
    ];
    let count = match app.search.hits.len() {
        1 => "1 match".to_string(),
        n => format!("{n} matches"),
    };
    let mut right = vec![Span::styled(format!("{count}   "), theme.muted())];
    right.extend(hint(theme, "enter", "go"));
    right.extend(hint(theme, "esc", "cancel"));
    (left, right)
}

fn browse_status(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    let left = [
        ("/", "search"),
        ("]", "next finding"),
        ("y", "copy"),
        ("?", "keys"),
        ("q", "quit"),
    ]
    .iter()
    .flat_map(|(k, l)| hint(theme, k, l))
    .collect();
    let right = match &app.toast {
        Some(toast) if toast.problem => vec![Span::styled(
            format!("{} ", toast.text),
            theme.fg(theme.error).add_modifier(Modifier::BOLD),
        )],
        Some(toast) => vec![Span::styled(
            format!("✓ {} ", toast.text),
            theme.fg(theme.ok).add_modifier(Modifier::BOLD),
        )],
        None if !app.search.hits.is_empty() => {
            let mut spans = vec![Span::styled(
                format!(
                    "match {} of {}   ",
                    app.search.current + 1,
                    app.search.hits.len()
                ),
                theme.muted(),
            )];
            spans.extend(hint(theme, "n", "next"));
            spans
        }
        None => Vec::new(),
    };
    (left, right)
}

fn status_spans(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let theme = &app.theme;
    let (rest, right) = match (app.search.editing, app.yanking) {
        (true, _) => search_status(app),
        (false, true) => (
            hint(theme, "key", "copies that line")
                .into_iter()
                .chain(hint(theme, "esc", "cancel"))
                .collect(),
            Vec::new(),
        ),
        (false, false) => browse_status(app),
    };
    let mut left = vec![
        Span::styled(format!(" {} ", mode(app)), theme.chip(theme.accent)),
        Span::raw("  "),
    ];
    left.extend(rest);
    (left, right)
}

/// The status bar as plain text, left part then right part.
pub fn status_text(app: &App) -> String {
    let (left, right) = status_spans(app);
    left.iter()
        .chain(&right)
        .map(|s| s.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let (left, right) = status_spans(app);
    spread(frame, area, left, right);
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(row);
    cell
}

fn popup_block(app: &App, title: &str, footer: &str) -> Block<'static> {
    let theme = &app.theme;
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.fg(theme.accent))
        .title(Span::styled(
            format!(" {title} "),
            theme.bold().fg(theme.accent),
        ))
        .title_bottom(Line::styled(format!(" {footer} "), theme.muted()).right_aligned())
        .padding(Padding::new(2, 2, 1, 0))
}

fn yank_line(app: &App, target: crate::yank::YankTarget, width: usize) -> Line<'static> {
    let theme = &app.theme;
    let name = format!("{:<15}", target.name());
    let text = yank_text(&app.doc, &app.outline, app.selected, target, &app.target);
    let Some(text) = text else {
        return Line::from(vec![
            Span::styled(format!("{}  ", target.key()), theme.fg(theme.border)),
            Span::styled(name, theme.fg(theme.border)),
            Span::styled("none here", theme.fg(theme.border)),
        ]);
    };
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = fit(&flat, width.saturating_sub(name.chars().count() + 3));
    Line::from(vec![
        Span::styled(format!("{}  ", target.key()), theme.key()),
        Span::raw(name),
        Span::styled(preview, theme.muted()),
    ])
}

fn draw_yank(app: &App, frame: &mut Frame) {
    let area = popup(frame.area(), 76, TARGETS.len() as u16 + 3);
    let width = usize::from(area.width.saturating_sub(6));
    let lines: Vec<Line> = TARGETS
        .iter()
        .map(|target| yank_line(app, *target, width))
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(popup_block(app, "Copy", "esc closes")),
        area,
    );
}

pub const KEYS: [(&str, &str); 12] = [
    ("↑ ↓  j k", "move, or scroll the focused pane"),
    ("← →  h l", "fold, or go to the parent or first child"),
    ("enter", "fold or unfold"),
    ("g G", "first row, last row"),
    ("tab", "focus tree, inspector, XML in turn"),
    ("/", "search OZ, ID, labels and long texts"),
    ("n N", "next, previous match"),
    ("] [", "next, previous row with findings"),
    ("m", "long text as Markdown"),
    ("y", "copy the OZ, ID, a text, the XML, a query"),
    ("alt arrows", "resize the panes"),
    ("q esc", "quit"),
];

fn draw_help(app: &App, frame: &mut Frame) {
    let theme = &app.theme;
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:<13}"), theme.key()),
                Span::raw(what.to_string()),
            ])
        })
        .collect();
    let area = popup(frame.area(), 64, KEYS.len() as u16 + 3);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(popup_block(app, "Keys", "any key closes")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_keeps_the_written_scale() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        assert_eq!(grouped(d("10731435383.24")), "10,731,435,383.24");
        assert_eq!(grouped(d("5000.000")), "5,000.000");
        assert_eq!(grouped(d("999")), "999");
        assert_eq!(grouped(d("-1234.5")), "-1,234.5");
    }

    #[test]
    fn fit_cuts_with_an_ellipsis() {
        assert_eq!(fit("Betonpflaster", 20), "Betonpflaster");
        assert_eq!(fit("Betonpflaster", 6), "Beton…");
    }

    #[test]
    fn marked_finds_every_occurrence_ignoring_case() {
        let mark = Style::default().add_modifier(Modifier::BOLD);
        let spans = marked("Beton und beton", "BETON", Style::default(), mark);
        let texts: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(texts, ["", "Beton", " und ", "beton", ""]);
        assert_eq!(spans[1].style, mark);
    }

    #[test]
    fn xml_lines_color_tags_attributes_and_values() {
        let theme = Theme::truecolor();
        let line = xml_line(&theme, "<Item ID=\"a\">5000</Item>");
        let parts: Vec<(&str, Option<Color>)> = line
            .spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style.fg))
            .collect();
        assert!(parts.contains(&("Item", Some(theme.info))));
        assert!(parts.contains(&(" ID", Some(theme.attribute))));
        assert!(parts.contains(&("\"a\"", Some(theme.ok))));
        assert!(parts.contains(&("5000", None)));
        assert_eq!(line.to_string(), "<Item ID=\"a\">5000</Item>");
    }

    #[test]
    fn kind_labels_read_as_words() {
        assert_eq!(kind_label(ItemKind::LumpSum), "Lump sum");
        assert_eq!(
            kind_label(ItemKind::Alternative {
                group: 1,
                serial: 2
            }),
            "Alternative 1.2"
        );
    }
}
