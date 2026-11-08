use gpui_kit::{FontStyle, FontWeight, TestAppContext, font, px, size};
use kg_core::{Cell, Color, Weight};

use super::pointer::cell_at;
use super::{
    Style, backgrounds, cell_size_from, dimensions, fits_program, is_new_fit, runs, steps, text_run,
};

#[test]
fn an_ended_terminal_keeps_its_cursor_out_of_the_paint() {
    assert!(!super::draws_cursor(false, true));
    assert!(super::draws_cursor(true, true));
    assert!(!super::draws_cursor(true, false));
}

#[test]
fn a_fit_is_sent_only_when_the_window_fits_and_the_size_changes() {
    assert!(is_new_fit(None, 80, 24));
    assert!(is_new_fit(Some((80, 24)), 81, 24));
    assert!(!is_new_fit(Some((80, 24)), 80, 24));
    assert!(!is_new_fit(None, 19, 24));
    assert!(!is_new_fit(None, 80, 4));
}

fn cell(c: char) -> Cell {
    Cell {
        c,
        zerowidth: Vec::new(),
        fg: Color::Foreground,
        bg: Color::Background,
        weight: Weight::Normal,
        italic: false,
        underline: false,
        strikeout: false,
    }
}

#[test]
fn the_area_holds_whole_cells_within_the_host_limits() {
    let cell = size(px(10.), px(20.));
    assert_eq!(dimensions(size(px(805.), px(479.)), cell), (80, 23));
    assert_eq!(dimensions(size(px(800.), px(480.)), cell), (80, 24));
    assert_eq!(dimensions(size(px(0.), px(0.)), cell), (2, 1));
    assert_eq!(dimensions(size(px(1e6), px(1e6)), cell), (1000, 500));
}

#[test]
fn steps_count_columns() {
    assert!((steps(3) - 3.).abs() < f32::EPSILON);
    assert!((steps(usize::MAX) - f32::from(u16::MAX)).abs() < f32::EPSILON);
}

#[test]
fn cells_of_one_style_are_one_run() {
    let mut bold = cell('b');
    bold.weight = Weight::Bold;
    let mut accent = cell('e');
    accent.zerowidth = vec!['\u{301}'];
    let line = [cell('a'), cell('a'), bold, accent];
    let (text, styles) = runs(&line);
    assert_eq!(text, "aabe\u{301}");
    let lengths: Vec<usize> = styles.iter().map(|(len, _)| *len).collect();
    assert_eq!(lengths, [2, 1, 3]);
    assert_eq!(styles[1].1.weight, Weight::Bold);
    assert_eq!(styles[2].1, Style::of(&cell('x')));
}

#[test]
fn every_style_field_splits_a_run() {
    let mut italic = cell('i');
    italic.italic = true;
    let mut underline = cell('u');
    underline.underline = true;
    let mut strikeout = cell('s');
    strikeout.strikeout = true;
    let mut red = cell('r');
    red.fg = Color::Rgb(1, 0, 0);
    let line = [cell('a'), italic, underline, strikeout, red, cell('b')];
    assert_eq!(runs(&line).1.len(), 6);
}

#[test]
fn only_colored_backgrounds_are_painted() {
    let tinted = |c| {
        let mut cell = cell(c);
        cell.bg = Color::Rgb(9, 9, 9);
        cell
    };
    let mut other = cell('o');
    other.bg = Color::Rgb(1, 1, 1);
    let line = [
        cell('a'),
        tinted('b'),
        tinted('c'),
        other,
        cell('d'),
        tinted('e'),
    ];
    assert_eq!(
        backgrounds(&line),
        [
            (1, 2, Color::Rgb(9, 9, 9)),
            (3, 1, Color::Rgb(1, 1, 1)),
            (5, 1, Color::Rgb(9, 9, 9)),
        ]
    );
}

fn near(length: gpui_kit::Pixels, want: f32) -> bool {
    (length.as_f32() - want).abs() < 0.001
}

#[test]
fn a_measured_advance_is_the_cell_width() {
    let cell = cell_size_from(Some(px(8.)), px(10.));
    assert!(near(cell.width, 8.));
    assert!(near(cell.height, 13.));
}

#[test]
fn a_font_that_measures_nothing_gets_six_tenths_of_its_size() {
    for advance in [None, Some(px(0.)), Some(px(-3.))] {
        let cell = cell_size_from(advance, px(10.));
        assert!(near(cell.width, 6.), "{advance:?}");
    }
}

fn style(weight: Weight, italic: bool, decorated: bool) -> Style {
    Style {
        fg: Color::Rgb(255, 0, 0),
        weight,
        italic,
        underline: decorated,
        strikeout: decorated,
    }
}

fn init_theme(cx: &mut TestAppContext) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let workspace = cx.update(|cx| {
        kg_core::Workspace::open(
            dir.path().into(),
            std::env::current_exe().unwrap(),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(workspace).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_shell::init(cx);
        crate::theme::init_in(dir.path().join("themes"), cx);
    });
    dir
}

#[gpui_kit::test]
fn a_bold_italic_run_is_drawn_so(cx: &mut TestAppContext) {
    let _data = init_theme(cx);
    let family = font("Inter");
    let run = cx.update(|cx| text_run(3, style(Weight::Bold, true, true), &family, cx));
    assert_eq!(run.len, 3);
    assert_eq!(run.font.weight, FontWeight::BOLD);
    assert_eq!(run.font.style, FontStyle::Italic);
    assert!(run.underline.is_some());
    assert!(run.strikethrough.is_some());
}

#[gpui_kit::test]
fn a_plain_run_is_regular_and_undecorated(cx: &mut TestAppContext) {
    let _data = init_theme(cx);
    let family = font("Inter");
    let run = cx.update(|cx| text_run(1, style(Weight::Normal, false, false), &family, cx));
    assert_eq!(run.font.weight, FontWeight::NORMAL);
    assert_eq!(run.font.style, FontStyle::Normal);
    assert!(run.underline.is_none());
    assert!(run.strikethrough.is_none());
    assert!((run.color.a - 1.).abs() < 0.001);
}

#[gpui_kit::test]
fn a_dim_run_is_faded(cx: &mut TestAppContext) {
    let _data = init_theme(cx);
    let family = font("Inter");
    let run = cx.update(|cx| text_run(1, style(Weight::Dim, false, false), &family, cx));
    assert_eq!(run.font.weight, FontWeight::NORMAL);
    assert!((run.color.a - 0.6).abs() < 0.001, "{}", run.color.a);
}

#[test]
fn a_thin_window_keeps_the_programs_size() {
    assert!(fits_program(80, 24));
    assert!(!fits_program(19, 24));
    assert!(!fits_program(80, 4));
    assert!(!fits_program(0, 0));
}

#[test]
fn a_point_falls_in_the_cell_it_is_over_and_stays_on_the_grid() {
    let step = px(10.);
    assert_eq!(cell_at(px(0.), step, 5), 0);
    assert_eq!(cell_at(px(9.9), step, 5), 0);
    assert_eq!(cell_at(px(10.), step, 5), 1);
    assert_eq!(cell_at(px(999.), step, 5), 4);
}
