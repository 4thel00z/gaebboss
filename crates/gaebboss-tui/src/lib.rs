//! Interactive terminal explorer for GAEB DA XML: the bill-of-quantities
//! tree, an inspector with fields, long texts and validation findings, and
//! the selected element's XML.
//!
//! State (`app`), the bill model (`outline`), key mapping (`input`), yank
//! texts (`yank`) and rendering (`ui`) are pure; only [`run`] touches the
//! terminal. Its loop `tokio::select!`s over the crossterm event stream, a
//! channel of background results and a 100 ms tick, so building the outline
//! and talking to the clipboard never block input.

pub mod app;
pub mod clipboard;
pub mod input;
pub mod outline;
pub mod theme;
pub mod ui;
pub mod yank;

use std::io::{self, IsTerminal};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use gaebboss_core::Document;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::app::{App, Cmd, Msg};
use crate::clipboard::Transport;
use crate::outline::Outline;

const TICK: Duration = Duration::from_millis(100);
const NO_TTY: &str = "gaebboss tui requires an interactive terminal";

/// Names the missing terminal when raw mode cannot attach: `ENXIO` (6) with
/// no controlling terminal, `ENOTTY` (25) on a non-tty device. Neither has a
/// stable `io::ErrorKind`, so the raw OS error is the only portable signal.
fn friendly_no_tty_error(err: io::Error) -> io::Error {
    match err.raw_os_error() {
        Some(6) | Some(25) => io::Error::new(err.kind(), NO_TTY),
        _ => err,
    }
}

/// Restores the terminal on drop, so early returns never leave the shell in
/// raw mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    }
}

/// Runs the explorer until the user quits. `title` labels the status bar and
/// `target` is the path as given, used verbatim in yanked shell commands.
/// Needs a tokio runtime with time enabled; the current-thread one will do.
pub async fn run(doc: Document, title: String, target: String) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(NO_TTY));
    }
    enable_raw_mode().map_err(friendly_no_tty_error)?;
    let guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let size = terminal.size()?;
    let doc = Arc::new(doc);
    let mut app = App::new(Arc::clone(&doc), title, target, (size.width, size.height));
    app.theme = theme::Theme::detect();
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    spawn_outline(doc, tx.clone());
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(&app, frame))?;
        let msg = tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => Msg::Key(key),
                Some(Ok(Event::Resize(width, height))) => Msg::Resize(width, height),
                Some(Ok(_)) => continue,
                Some(Err(error)) => return Err(error),
                None => break,
            },
            Some(msg) = rx.recv() => msg,
            _ = tick.tick() => Msg::Tick,
        };
        let mut pending = app.update(msg);
        while let Some(cmd) = pending.pop() {
            pending.extend(execute_cmd(&mut app, &tx, cmd));
        }
    }
    drop(guard);
    Ok(())
}

/// Builds the outline (validation and every item's texts) on the blocking
/// pool and sends it back.
fn spawn_outline(doc: Arc<Document>, tx: UnboundedSender<Msg>) {
    tokio::task::spawn_blocking(move || {
        tx.send(Msg::Loaded(Outline::build(&doc))).ok();
    });
}

/// Starts what a [`Cmd`] asks for; work done in place returns the commands
/// its result leads to.
fn execute_cmd(app: &mut App, tx: &UnboundedSender<Msg>, cmd: Cmd) -> Vec<Cmd> {
    match cmd {
        Cmd::Copy { target, text } => {
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let msg = match clipboard::copy_native(&text) {
                    true => Msg::Copied {
                        target,
                        result: Ok(Transport::Native),
                    },
                    false => Msg::NoNativeClipboard { target, text },
                };
                tx.send(msg).ok();
            });
            Vec::new()
        }
        Cmd::Osc52 { target, text } => {
            let result = clipboard::write_osc52(&text).map(|()| Transport::Osc52);
            app.update(Msg::Copied { target, result })
        }
    }
}
