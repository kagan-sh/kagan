use super::*;
use crate::{Cell, Color, Weight};

fn found(text: &str) -> Vec<&str> {
    links(text).into_iter().map(|range| &text[range]).collect()
}

#[test]
fn finds_links_and_leaves_out_what_ends_a_sentence() {
    assert_eq!(
        found("Local: http://localhost:3000/, Network: https://10.0.0.2:3000."),
        ["http://localhost:3000/", "https://10.0.0.2:3000"]
    );
    assert_eq!(
        found("(see https://example.com/a_(b))"),
        ["https://example.com/a_(b)"]
    );
    assert_eq!(
        found("\"http://127.0.0.1:8080\""),
        ["http://127.0.0.1:8080"]
    );
    assert_eq!(
        found("<https://x.test/path?q=1>"),
        ["https://x.test/path?q=1"]
    );
}

#[test]
fn text_without_a_host_is_not_a_link() {
    assert!(found("http:// and ftp://host and https").is_empty());
}

fn row(text: &str) -> Vec<Cell> {
    text.chars()
        .map(|c| Cell {
            c,
            zerowidth: Vec::new(),
            fg: Color::Foreground,
            bg: Color::Background,
            weight: Weight::Normal,
            italic: false,
            underline: false,
            strikeout: false,
        })
        .collect()
}

#[test]
fn a_click_finds_the_link_under_it_across_a_wrapped_row() {
    let grid = Grid {
        lines: vec![row("go http://loc"), row("alhost:5173/ ok"), row("plain")],
        wrapped: vec![true, false, false],
        ..Grid::default()
    };
    assert_eq!(
        grid.link_at(0, 5).as_deref(),
        Some("http://localhost:5173/")
    );
    assert_eq!(
        grid.link_at(1, 2).as_deref(),
        Some("http://localhost:5173/")
    );
    assert_eq!(grid.link_at(1, 14), None);
    assert_eq!(grid.link_at(0, 0), None);
    assert_eq!(grid.link_at(2, 1), None);
}

#[test]
fn a_hyperlink_the_program_set_wins_and_only_web_links_open() {
    let grid = Grid {
        lines: vec![row("docs file")],
        wrapped: vec![false],
        hyperlinks: vec![
            (0, 0..4, "https://docs.test/".into()),
            (0, 5..9, "file:///etc/passwd".into()),
        ],
        ..Grid::default()
    };
    assert_eq!(grid.link_at(0, 2).as_deref(), Some("https://docs.test/"));
    assert_eq!(grid.link_at(0, 6), None);
}

#[test]
fn the_screen_reports_hyperlinks_and_wrapped_rows() {
    let mut screen = crate::screen::Screen::new(12, 4);
    screen.feed(b"\x1b]8;;https://a.test/\x1b\\docs\x1b]8;;\x1b\\ x\r\nhttp://localhost:3000/\r\n");
    let grid = screen.grid();
    assert_eq!(grid.hyperlinks, [(0, 0..4, "https://a.test/".to_owned())]);
    assert_eq!(grid.link_at(0, 1).as_deref(), Some("https://a.test/"));
    assert_eq!(grid.wrapped[..2], [false, true]);
    assert_eq!(
        grid.link_at(2, 3).as_deref(),
        Some("http://localhost:3000/")
    );
}
