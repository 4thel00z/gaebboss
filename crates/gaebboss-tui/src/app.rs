//! Explorer state and its update function: messages in, state changes and
//! commands for the event loop out. Pure, so every interaction is testable
//! without a terminal.

use std::ops::Range;
use std::sync::Arc;

use crossterm::event::KeyEvent;
use gaebboss_core::Document;

use crate::clipboard::Transport;
use crate::input::{Action, KeyContext, action};
use crate::outline::{Outline, ROOT};
use crate::theme::Theme;
use crate::ui::{RIGHT_TOP_SPLIT, SPLIT_STEP, Splits, TREE_SPLIT};
use crate::yank::{YankTarget, yank_text};

/// The most source bytes the XML pane lays out for one element.
pub const SOURCE_LIMIT: usize = 1 << 20;
/// Ticks (100 ms each) a status-bar toast stays up.
pub const TOAST_TICKS: u16 = 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Tree,
    Inspector,
    Source,
}

#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    /// The outline built in the background replaces the loading placeholder.
    Loaded(Outline),
    Copied {
        target: YankTarget,
        result: Result<Transport, String>,
    },
    /// No native clipboard took the text; the loop falls back to OSC 52.
    NoNativeClipboard {
        target: YankTarget,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Hand the text to the native clipboard, off the input loop.
    Copy { target: YankTarget, text: String },
    /// Write the OSC 52 sequence from the loop, between frames.
    Osc52 { target: YankTarget, text: String },
}

#[derive(Clone, Debug, Default)]
pub struct Search {
    /// The prompt is open and taking keys.
    pub editing: bool,
    pub query: String,
    pub hits: Vec<usize>,
    pub current: usize,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub ticks_left: u16,
    /// Something went wrong or was not there, as opposed to a done action.
    pub problem: bool,
}

/// The selected element's source, split into display lines.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub lines: Vec<Range<usize>>,
    pub truncated: usize,
}

pub struct App {
    pub title: String,
    pub target: String,
    pub doc: Arc<Document>,
    pub outline: Outline,
    /// The outline is still being built; only the bill row exists.
    pub loading: bool,
    pub ticks: u64,
    /// Every row in tree order, collapsed or not.
    pub order: Vec<usize>,
    pub expanded: Vec<bool>,
    pub selected: usize,
    pub focus: Pane,
    pub inspector_scroll: u16,
    pub source_scroll: usize,
    pub source: Source,
    pub markdown: bool,
    pub search: Search,
    pub yanking: bool,
    /// The key overlay is open; the next key closes it.
    pub help: bool,
    pub toast: Option<Toast>,
    pub theme: Theme,
    pub splits: Splits,
    pub size: (u16, u16),
    pub should_quit: bool,
}

/// Line ranges of `range` in `bytes`, breaking at newlines and between
/// adjacent tags so minified XML reads one element per line.
pub fn source_lines(bytes: &[u8], range: Range<usize>) -> Source {
    let end = range.end.min(range.start.saturating_add(SOURCE_LIMIT));
    let truncated = range.end.saturating_sub(end);
    let indent = bytes
        .get(..range.start)
        .unwrap_or(&[])
        .iter()
        .rev()
        .take_while(|b| **b == b' ' || **b == b'\t')
        .count();
    let dedent = |at: usize| {
        let leading = bytes
            .get(at..end)
            .unwrap_or(&[])
            .iter()
            .take(indent)
            .take_while(|b| **b == b' ' || **b == b'\t')
            .count();
        at + leading
    };
    let mut lines = Vec::new();
    let mut start = range.start;
    for at in range.start..end {
        let byte = bytes.get(at).copied().unwrap_or(b'\n');
        let next = bytes.get(at + 1).copied();
        if byte == b'\n' {
            lines.push(start..at);
            start = dedent(at + 1);
            continue;
        }
        if byte == b'>' && next == Some(b'<') && at + 1 < end {
            lines.push(start..at + 1);
            start = at + 1;
        }
    }
    if start < end {
        lines.push(start..end);
    }
    Source { lines, truncated }
}

fn tree_order(outline: &Outline) -> Vec<usize> {
    let mut order = Vec::with_capacity(outline.rows.len());
    let mut stack = vec![ROOT];
    while let Some(index) = stack.pop() {
        order.push(index);
        let Some(row) = outline.row(index) else {
            continue;
        };
        stack.extend(row.children.iter().rev());
    }
    order
}

impl App {
    /// The explorer before its outline arrives: the bill row alone, keys
    /// other than quit ignored until [`Msg::Loaded`].
    pub fn new(doc: Arc<Document>, title: String, target: String, size: (u16, u16)) -> App {
        let pending = Outline::pending(&doc);
        let mut app = App {
            title,
            target,
            doc,
            outline: Outline::default(),
            loading: true,
            ticks: 0,
            order: Vec::new(),
            expanded: Vec::new(),
            selected: ROOT,
            focus: Pane::Tree,
            inspector_scroll: 0,
            source_scroll: 0,
            source: Source::default(),
            markdown: false,
            search: Search::default(),
            yanking: false,
            help: false,
            toast: None,
            theme: Theme::default(),
            splits: Splits::default(),
            size,
            should_quit: false,
        };
        app.install(pending);
        app.loading = true;
        app
    }

    fn install(&mut self, outline: Outline) {
        self.order = tree_order(&outline);
        self.expanded = vec![false; outline.rows.len()];
        self.outline = outline;
        self.loading = false;
        self.set_expanded(ROOT, true);
        self.select(ROOT);
    }

    /// Rows the tree pane shows, in order, with their indent.
    pub fn visible_rows(&self) -> Vec<usize> {
        let mut rows = Vec::new();
        let mut stack = vec![ROOT];
        while let Some(index) = stack.pop() {
            rows.push(index);
            if !self.is_expanded(index) {
                continue;
            }
            let Some(row) = self.outline.row(index) else {
                continue;
            };
            stack.extend(row.children.iter().rev());
        }
        rows
    }

    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.get(index).copied().unwrap_or(false)
    }

    pub fn has_children(&self, index: usize) -> bool {
        self.outline
            .row(index)
            .is_some_and(|r| !r.children.is_empty())
    }

    fn set_expanded(&mut self, index: usize, value: bool) {
        if let Some(slot) = self.expanded.get_mut(index) {
            *slot = value;
        }
    }

    /// Selects a row, opening every category above it.
    pub fn select(&mut self, index: usize) {
        for ancestor in self.outline.ancestors(index).into_iter().skip(1) {
            self.set_expanded(ancestor, true);
        }
        self.selected = index;
        self.inspector_scroll = 0;
        self.source_scroll = 0;
        let range = self.outline.source_range(&self.doc, index);
        self.source = source_lines(self.doc.bytes(), range);
    }

    fn key_context(&self) -> KeyContext {
        if self.search.editing {
            return KeyContext::Search;
        }
        if self.yanking {
            return KeyContext::Yank;
        }
        KeyContext::Normal
    }

    fn page(&self) -> usize {
        usize::from(self.size.1.saturating_sub(3)).max(1)
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            ticks_left: TOAST_TICKS,
            problem: false,
        });
    }

    fn problem(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            ticks_left: TOAST_TICKS,
            problem: true,
        });
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        match msg {
            Msg::Key(key) => {
                if self.help {
                    self.help = false;
                    return Vec::new();
                }
                let intent = action(key, self.key_context());
                if self.loading && intent != Action::Quit {
                    return Vec::new();
                }
                return self.apply(intent);
            }
            Msg::Loaded(outline) => self.install(outline),
            Msg::NoNativeClipboard { target, text } => return vec![Cmd::Osc52 { target, text }],
            Msg::Resize(width, height) => self.size = (width, height),
            Msg::Tick => self.tick(),
            Msg::Copied { target, result } => match result {
                Ok(Transport::Native) => self.toast(format!("Copied {}", target.name())),
                Ok(Transport::Osc52) => {
                    self.toast(format!("Copied {} through the terminal", target.name()))
                }
                Err(error) => self.problem(format!("Could not copy {}: {error}", target.name())),
            },
        }
        Vec::new()
    }

    fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        let Some(toast) = self.toast.as_mut() else {
            return;
        };
        toast.ticks_left = toast.ticks_left.saturating_sub(1);
        if toast.ticks_left == 0 {
            self.toast = None;
        }
    }

    fn apply(&mut self, intent: Action) -> Vec<Cmd> {
        match intent {
            Action::Quit => self.should_quit = true,
            Action::MoveUp => self.scroll(-1),
            Action::MoveDown => self.scroll(1),
            Action::PageUp => self.scroll(-(self.page() as isize)),
            Action::PageDown => self.scroll(self.page() as isize),
            Action::Top => self.scroll(isize::MIN / 2),
            Action::Bottom => self.scroll(isize::MAX / 2),
            Action::Collapse => self.collapse(),
            Action::Expand => self.expand(),
            Action::Toggle => self.toggle(),
            Action::Parent => self.parent(),
            Action::FocusNext => {
                self.focus = match self.focus {
                    Pane::Tree => Pane::Inspector,
                    Pane::Inspector => Pane::Source,
                    Pane::Source => Pane::Tree,
                };
            }
            Action::ToggleMarkdown => {
                self.markdown = !self.markdown;
                self.inspector_scroll = 0;
            }
            Action::NextFinding => self.jump_finding(true),
            Action::PrevFinding => self.jump_finding(false),
            Action::OpenSearch => {
                self.search.editing = true;
                self.search.query.clear();
            }
            Action::SearchChar(c) => {
                self.search.query.push(c);
                self.search.hits = self.matches();
            }
            Action::SearchBackspace => {
                self.search.query.pop();
                self.search.hits = self.matches();
            }
            Action::Help => self.help = true,
            Action::SearchCancel => self.search.editing = false,
            Action::SearchAccept => self.accept_search(),
            Action::NextHit => self.step_hit(true),
            Action::PrevHit => self.step_hit(false),
            Action::OpenYank => self.yanking = true,
            Action::YankCancel => self.yanking = false,
            Action::Yank(target) => return self.yank(target),
            Action::ResizeLeft => self.splits.tree = step(self.splits.tree, -1, TREE_SPLIT),
            Action::ResizeRight => self.splits.tree = step(self.splits.tree, 1, TREE_SPLIT),
            Action::ResizeUp => {
                self.splits.right_top = step(self.splits.right_top, -1, RIGHT_TOP_SPLIT)
            }
            Action::ResizeDown => {
                self.splits.right_top = step(self.splits.right_top, 1, RIGHT_TOP_SPLIT)
            }
            Action::Noop => {}
        }
        Vec::new()
    }

    fn scroll(&mut self, delta: isize) {
        match self.focus {
            Pane::Tree => self.move_selection(delta),
            Pane::Inspector => {
                let next = (self.inspector_scroll as isize).saturating_add(delta);
                self.inspector_scroll = next.clamp(0, u16::MAX as isize) as u16;
            }
            Pane::Source => {
                let last = self.source.lines.len().saturating_sub(1) as isize;
                self.source_scroll = (self.source_scroll as isize)
                    .saturating_add(delta)
                    .clamp(0, last) as usize;
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let rows = self.visible_rows();
        let position = rows.iter().position(|r| *r == self.selected).unwrap_or(0) as isize;
        let last = rows.len().saturating_sub(1) as isize;
        let next = position.saturating_add(delta).clamp(0, last) as usize;
        let Some(index) = rows.get(next).copied() else {
            return;
        };
        self.select(index);
    }

    fn collapse(&mut self) {
        if self.is_expanded(self.selected) && self.has_children(self.selected) {
            self.set_expanded(self.selected, false);
            return;
        }
        self.parent();
    }

    fn expand(&mut self) {
        if !self.has_children(self.selected) {
            return;
        }
        if !self.is_expanded(self.selected) {
            self.set_expanded(self.selected, true);
            return;
        }
        let first = self
            .outline
            .row(self.selected)
            .and_then(|r| r.children.first().copied());
        let Some(first) = first else {
            return;
        };
        self.select(first);
    }

    fn toggle(&mut self) {
        if !self.has_children(self.selected) {
            return;
        }
        let open = self.is_expanded(self.selected);
        self.set_expanded(self.selected, !open);
    }

    fn parent(&mut self) {
        let Some(parent) = self.outline.row(self.selected).and_then(|r| r.parent) else {
            return;
        };
        self.select(parent);
    }

    /// Every row in tree order, starting next to the selection in the given
    /// direction and wrapping around to end on it.
    fn cycle_from_selection(&self, forward: bool) -> Vec<usize> {
        let position = self
            .order
            .iter()
            .position(|r| *r == self.selected)
            .unwrap_or(0);
        let after = self.order.get(position + 1..).unwrap_or(&[]);
        let before = self.order.get(..=position).unwrap_or(&[]);
        let mut rotated: Vec<usize> = after.iter().chain(before).copied().collect();
        if !forward {
            rotated.rotate_right(1);
            rotated.reverse();
        }
        rotated
    }

    fn jump_finding(&mut self, forward: bool) {
        let next = self
            .cycle_from_selection(forward)
            .into_iter()
            .find(|i| self.outline.row(*i).is_some_and(|r| !r.findings.is_empty()));
        let Some(next) = next else {
            self.toast("No findings in this bill");
            return;
        };
        self.select(next);
    }

    /// Rows whose OZ, ID, label or long text hold the query, in tree order.
    pub fn matches(&self) -> Vec<usize> {
        let query = self.search.query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.order
            .iter()
            .copied()
            .filter(|i| {
                self.outline
                    .haystacks
                    .get(*i)
                    .is_some_and(|h| h.contains(&query))
            })
            .collect()
    }

    fn accept_search(&mut self) {
        self.search.editing = false;
        self.search.hits = self.matches();
        if self.search.query.trim().is_empty() {
            return;
        }
        let Some(hit) = std::iter::once(self.selected)
            .chain(self.cycle_from_selection(true))
            .find(|i| self.search.hits.contains(i))
        else {
            self.problem(format!("Nothing matches \"{}\"", self.search.query));
            return;
        };
        self.search.current = self.search.hits.iter().position(|h| *h == hit).unwrap_or(0);
        self.select(hit);
    }

    fn step_hit(&mut self, forward: bool) {
        let count = self.search.hits.len();
        if count == 0 {
            return;
        }
        self.search.current = match forward {
            true => (self.search.current + 1) % count,
            false => (self.search.current + count - 1) % count,
        };
        let Some(hit) = self.search.hits.get(self.search.current).copied() else {
            return;
        };
        self.select(hit);
    }

    fn yank(&mut self, target: YankTarget) -> Vec<Cmd> {
        self.yanking = false;
        let text = yank_text(
            &self.doc,
            &self.outline,
            self.selected,
            target,
            &self.target,
        );
        let Some(text) = text else {
            self.problem(format!("This row has no {}", target.name()));
            return Vec::new();
        };
        vec![Cmd::Copy { target, text }]
    }
}

fn step(value: u16, direction: i32, bounds: std::ops::RangeInclusive<u16>) -> u16 {
    let next = i32::from(value) + direction * i32::from(SPLIT_STEP);
    next.clamp(i32::from(*bounds.start()), i32::from(*bounds.end())) as u16
}
