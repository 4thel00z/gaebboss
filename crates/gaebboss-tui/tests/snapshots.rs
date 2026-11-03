//! Full frames on an 80x24 TestBackend over the official BVBS X84, and the
//! interactions that lead to them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;

use gaebboss_core::Document;
use gaebboss_tui::app::{App, Cmd, Msg, Pane};
use gaebboss_tui::outline::{Outline, ROOT};
use gaebboss_tui::ui;
use gaebboss_tui::yank::YankTarget;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn fixture(rel: &str) -> String {
    format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn pending(doc: Document) -> (App, Arc<Document>) {
    let doc = Arc::new(doc);
    let app = App::new(
        Arc::clone(&doc),
        "bvbs".to_string(),
        "bvbs.x8x".to_string(),
        (80, 24),
    );
    (app, doc)
}

fn loaded(doc: Document) -> App {
    let (mut app, doc) = pending(doc);
    app.update(Msg::Loaded(Outline::build(&doc)));
    app
}

fn app(rel: &str) -> App {
    loaded(Document::open(std::path::Path::new(&fixture(rel))).unwrap())
}

fn press(app: &mut App, code: KeyCode) -> Vec<Cmd> {
    app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn frame(app: &App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| ui::draw(app, f)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn assert_frame(app: &App, expected: &[&str]) {
    let lines = frame(app);
    let printed = lines.join("\n");
    assert_eq!(lines.len(), expected.len(), "{printed}");
    for (index, want) in expected.iter().enumerate() {
        assert_eq!(lines[index], want.trim_end(), "line {index}\n{printed}");
    }
}

fn selected_oz(app: &App) -> String {
    app.outline.row(app.selected).unwrap().oz.clone()
}

#[test]
fn opening_shows_the_bill() {
    let app = app("3.3/bvbs_bauausfuehrung.x83");
    assert_frame(
        &app,
        &[
            " gaebboss   bvbs   GAEB 3.3 X83   27 items in 12 categories       ✓ no findings",
            "╭ Outline ───────────────────────╮╭ Bill ──────────────────────────────────────╮",
            "│▌ ▾ BVBS GAEB Bauausf.          ││ BVBS GAEB Muster                           │",
            "│    ▸ 001 Bauhauptgewerke       ││ BVBS GAEB Musterdatei 3.3                  │",
            "│    ▸ 002 Baunebengewerke       ││ BVBS GAEB Musterdatei Bauausführung        │",
            "│    ▸ 999 Stundenlohnarbeiten   ││                                            │",
            "│                                ││ Bill       BVBS GAEB Bauausf.              │",
            "│                                ││ Format     GAEB DA XML 3.3 X83             │",
            "│                                ││ Contents   12 categories, 27 items         │",
            "│                                ││ Markups    1                               │",
            "│                                ││ OZ shape   000.000.0000 + index            │",
            "│                                ││ Currency   EUR                             │",
            "│                                ││ Total      not priced                      │",
            "│                                ││                                            │",
            "│                                │╰────────────────────────────────────────────╯",
            "│                                │╭ XML ──────────────────────── 0x6b..0x18737 ╮",
            "│                                ││ <GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XM ┃",
            "│                                ││  <GAEBInfo>                                │",
            "│                                ││   <Version>3.3</Version>                   │",
            "│                                ││   <VersDate>2021-05</VersDate>             │",
            "│                                ││   <Date>2024-04-04</Date>                  │",
            "│                                ││   <Time>10:06:45</Time>                    │",
            "╰────────────────────────────────╯╰────────────────────────────────────────────╯",
            " browse   / search   ] next finding   y copy   ? keys   q quit",
        ],
    );
}

#[test]
fn walking_down_to_an_item_shows_its_fields_and_xml() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x83");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    assert_eq!(selected_oz(&app), "001.001.0010");
    assert_frame(
        &app,
        &[
            " gaebboss   bvbs   GAEB 3.3 X83   27 items in 12 categories       ✓ no findings",
            "╭ Outline ───────────────────────╮╭ Item 001.001.0010 ─────────────────────────╮",
            "│  ▾ BVBS GAEB Bauausf.          ││ Baustelleneinrichtung einrichten,          │",
            "│    ▾ 001 Bauhauptgewerke       ││ vorhalten, räumen                          │",
            "│    │ ▾ 001 Baustelleneinricht… ││  Standard   counts towards the total       │",
            "│▌   │ │   0010 Baustelleneinri… ││                                            │",
            "│    │ │   00101 Betreiben der … ││ 1.000 psch   not priced                    │",
            "│    │ │   0010A Betreiben der … ││ ID ID14EF2450                              │",
            "│    │ ▸ 002 Erdarbeiten         ││                                            │",
            "│    │ ▸ 003 Maurerarbeiten      ││ Long text ──────────────────────────────── │",
            "│    │ ▸ 004 Betonarbeiten       ││ Baustelleneinrichtung für sämtliche, in    │",
            "│    ▸ 002 Baunebengewerke       ││ der Leistungsbeschreibung aufgeführten     │",
            "│    ▸ 999 Stundenlohnarbeiten   ││ Leistungen einrichten, vorhalten und       │",
            "│                                ││ räumen, Vorhaltedauer 12 Wochen, Gerüste   │",
            "│                                │╰────────────────────────────────────────────╯",
            "│                                │╭ XML ──────────────────────── 0xde4..0x128b ╮",
            "│                                ││ <Item ID=\"ID14EF2450\" RNoPart=\"0010\">      ┃",
            "│                                ││  <Qty>1.000</Qty>                          │",
            "│                                ││  <QU>psch</QU>                             │",
            "│                                ││  <Description>                             │",
            "│                                ││   <CompleteText>                           │",
            "│                                ││    <DetailTxt>                             │",
            "╰────────────────────────────────╯╰────────────────────────────────────────────╯",
            " browse   / search   ] next finding   y copy   ? keys   q quit",
        ],
    );
}

#[test]
fn collapse_returns_to_the_parent_and_closes_it() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    assert_eq!(selected_oz(&app), "001.001");
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(selected_oz(&app), "001");
    press(&mut app, KeyCode::Char('h'));
    assert!(!app.is_expanded(app.selected));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.selected, ROOT);
    press(&mut app, KeyCode::Char('G'));
    assert_eq!(selected_oz(&app), "999");
    press(&mut app, KeyCode::Char('g'));
    assert_eq!(app.selected, ROOT);
}

#[test]
fn search_reveals_hits_and_cycles_through_them() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x83");
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "wasserhaltung");
    let prompt = ui::status_text(&app);
    assert!(prompt.starts_with(" search   /wasserhaltung█"), "{prompt}");
    assert!(
        prompt.contains(&format!("{} matches", app.search.hits.len())),
        "{prompt}"
    );
    press(&mut app, KeyCode::Enter);
    assert!(!app.search.hits.is_empty());
    let first = app.selected;
    assert!(app.outline.haystacks[first].contains("wasserhaltung"));
    assert!(app.visible_rows().contains(&first));
    assert!(ui::status_text(&app).contains(&format!("match 1 of {}", app.search.hits.len())));
    assert!(frame(&app).iter().any(|l| l.contains("Wasserhaltung")));
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(app.selected, first);
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "kein treffer xyz");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.selected, first);
    assert!(ui::status_text(&app).contains("Nothing matches \"kein treffer xyz\""));
}

#[test]
fn a_discounted_item_shows_its_discount_and_adds_up() {
    let mut app = app("3.1/Musterdatei_3.1_2008-001.X86");
    let index = app
        .outline
        .rows
        .iter()
        .position(|r| r.oz == "1.10.10.0060")
        .unwrap();
    app.select(index);
    let lines: Vec<String> = ui::inspector_lines(&app, 60)
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("×  20.000  less  10.00 %  =  720.00  ✓")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("Findings ─")),
        "{lines:?}"
    );
}

#[test]
fn findings_jump_to_the_flagged_row() {
    let text = std::fs::read_to_string(fixture("3.3/bvbs_bauausfuehrung.x84"))
        .unwrap()
        .replacen("<IT>5000.00</IT>", "<IT>4000.00</IT>", 1);
    let doc = Document::parse(text.as_bytes()).unwrap();
    let mut app = loaded(doc);
    press(&mut app, KeyCode::Char(']'));
    let flagged = app.outline.row(app.selected).unwrap();
    assert!(!flagged.findings.is_empty());
    assert!(frame(&app).iter().any(|l| l.contains('✗')));
    let lines: Vec<String> = ui::inspector_lines(&app, 60)
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert!(
        lines.iter().any(|l| l.starts_with("Findings ─")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("✗ adds up to")),
        "{lines:?}"
    );
    assert!(frame(&app)[0].contains("✗ 3"), "{:?}", frame(&app)[0]);
}

#[test]
fn yank_copies_through_a_command_and_reports_back() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    press(&mut app, KeyCode::Char('y'));
    assert!(ui::status_text(&app).starts_with(" copy "));
    let popup = frame(&app);
    assert!(popup.iter().any(|l| l.contains("╭ Copy ")), "{popup:?}");
    assert!(
        popup
            .iter()
            .any(|l| l.contains("jq query") && l.contains(".boq")),
        "{popup:?}"
    );
    assert!(
        popup
            .iter()
            .any(|l| l.contains("long text") && l.contains("none here")),
        "{popup:?}"
    );
    let cmds = press(&mut app, KeyCode::Char('q'));
    assert_eq!(
        cmds,
        vec![Cmd::Copy {
            target: YankTarget::Query,
            text: ".boq".to_string()
        }]
    );
    assert!(!app.yanking);
    app.update(Msg::Copied {
        target: YankTarget::Query,
        result: Err("no clipboard".to_string()),
    });
    assert!(ui::status_text(&app).ends_with("Could not copy jq query: no clipboard"));
    for _ in 0..gaebboss_tui::app::TOAST_TICKS {
        app.update(Msg::Tick);
    }
    assert!(app.toast.is_none());
    press(&mut app, KeyCode::Char('y'));
    let nothing = press(&mut app, KeyCode::Char('t'));
    assert!(nothing.is_empty());
    assert!(ui::status_text(&app).ends_with("This row has no long text"));
}

#[test]
fn tab_moves_focus_and_scrolls_the_focused_pane() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Inspector);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.inspector_scroll, 1);
    assert_eq!(app.selected, ROOT);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, KeyCode::PageDown);
    assert!(app.source_scroll > 0);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Tree);
}

#[test]
fn markdown_mode_and_resizing() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x83");
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "wasserhaltung");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('m'));
    assert!(app.markdown);
    let lines: Vec<String> = ui::inspector_lines(&app, 60)
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Long text as Markdown ─")),
        "{lines:?}"
    );
    let before = app.splits;
    app.update(Msg::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)));
    assert_eq!(app.splits.tree, before.tree + 5);
    for _ in 0..20 {
        app.update(Msg::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)));
    }
    assert_eq!(app.splits.right_top, 20);
}

#[test]
fn quit_on_q() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit);
}

#[test]
fn keys_wait_for_the_outline_but_quit_does_not() {
    let doc = Document::open(std::path::Path::new(&fixture(
        "3.3/bvbs_bauausfuehrung.x83",
    )))
    .unwrap();
    let (mut app, doc) = pending(doc);
    assert!(app.loading);
    assert_eq!(app.outline.rows.len(), 1);
    let lines = frame(&app);
    assert!(lines[0].contains("checking the bill"), "{lines:?}");
    assert!(lines[2].contains("BVBS GAEB Bauausf."), "{lines:?}");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('/'));
    assert!(!app.search.editing);
    app.update(Msg::Loaded(Outline::build(&doc)));
    assert!(!app.loading);
    assert!(!frame(&app).iter().any(|l| l.contains("checking the bill")));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(selected_oz(&app), "001");
    let (mut waiting, _) = pending(
        Document::open(std::path::Path::new(&fixture(
            "3.3/bvbs_bauausfuehrung.x83",
        )))
        .unwrap(),
    );
    press(&mut waiting, KeyCode::Char('q'));
    assert!(waiting.should_quit);
}

#[test]
fn without_a_native_clipboard_the_loop_falls_back_to_osc52() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    let cmds = app.update(Msg::NoNativeClipboard {
        target: YankTarget::Oz,
        text: "001".to_string(),
    });
    assert_eq!(
        cmds,
        vec![Cmd::Osc52 {
            target: YankTarget::Oz,
            text: "001".to_string()
        }]
    );
}

#[test]
fn question_mark_opens_the_keys_and_any_key_closes_them() {
    let mut app = app("3.3/bvbs_bauausfuehrung.x84");
    press(&mut app, KeyCode::Char('?'));
    assert!(app.help);
    let lines = frame(&app);
    assert!(lines.iter().any(|l| l.contains("╭ Keys ")), "{lines:?}");
    assert!(
        lines.iter().any(|l| l.contains("any key closes")),
        "{lines:?}"
    );
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.help);
    assert!(!app.should_quit);
}
