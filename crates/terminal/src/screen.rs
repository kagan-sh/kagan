use std::fmt::Write as _;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, GridCell, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

pub const MIN_COLS: u16 = 2;
pub const MAX_COLS: u16 = 1000;
pub const MAX_ROWS: u16 = 500;
/// The whole saved replay: history, the newlines that push it into scrollback, and the visible
/// redraw. History is dropped first. A visible redraw that does not fit is not stored.
pub const SNAPSHOT_MAX: usize = 4 * 1024 * 1024;
/// Lines kept above the screen, per program.
const SCROLLBACK: usize = 10_000;

/// `replay` is bytes `Screen::feed` already accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub cols: u16,
    pub rows: u16,
    pub replay: Vec<u8>,
    pub history_dropped: u32,
    pub screen_kept: bool,
}

/// How a selection starts. `Extend` moves the end of the selection that is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    Simple,
    Word,
    Line,
    Block,
    Extend,
}

/// A change to the screen that waits for the reader or a settle, so a client never waits on the
/// reader's lock.
#[derive(Clone, Copy)]
pub(crate) enum Edit {
    Resize(u16, u16),
    Scroll(i32),
    Bottom,
    Select {
        mode: SelectMode,
        column: u16,
        row: u16,
        right: bool,
    },
}

const FOREGROUND: Rgb = Rgb {
    r: 0xff,
    g: 0xff,
    b: 0xff,
};
const BACKGROUND: Rgb = Rgb {
    r: 0x0b,
    g: 0x0a,
    b: 0x09,
};

const ATTRIBUTES: [(Flags, u8); 7] = [
    (Flags::BOLD, 1),
    (Flags::DIM, 2),
    (Flags::ITALIC, 3),
    (Flags::ALL_UNDERLINES, 4),
    (Flags::INVERSE, 7),
    (Flags::HIDDEN, 8),
    (Flags::STRIKEOUT, 9),
];

const MODES: [(TermMode, u16); 8] = [
    (TermMode::APP_CURSOR, 1),
    (TermMode::MOUSE_REPORT_CLICK, 1000),
    (TermMode::MOUSE_DRAG, 1002),
    (TermMode::MOUSE_MOTION, 1003),
    (TermMode::FOCUS_IN_OUT, 1004),
    (TermMode::UTF8_MOUSE, 1005),
    (TermMode::SGR_MOUSE, 1006),
    (TermMode::BRACKETED_PASTE, 2004),
];

struct Events(Sender<Event>);

impl EventListener for Events {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

pub(crate) struct Screen {
    term: Term<Events>,
    processor: Processor,
    events: Receiver<Event>,
    title: Option<String>,
}

impl Screen {
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
        let (sender, events) = mpsc::channel();
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &size(cols, rows), Events(sender)),
            processor: Processor::new(),
            events,
            title: None,
        }
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.processor.advance(&mut self.term, bytes);
        self.replies()
    }

    // A program that holds the screen (`ESC[?2026h`) and never lets go must not freeze it:
    // vte leaves ending the hold to its caller. Returns what the program asked inside it.
    pub(crate) fn expire_hold(&mut self) -> Option<Vec<u8>> {
        let due = self
            .processor
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|at| at <= Instant::now());
        due.then(|| {
            self.processor.stop_sync(&mut self.term);
            self.replies()
        })
    }

    // Ends a hold the program never let go of, so the text it sent is on the screen.
    pub(crate) fn end_hold(&mut self) -> Vec<u8> {
        if self.processor.sync_timeout().sync_timeout().is_some() {
            self.processor.stop_sync(&mut self.term);
        }
        self.replies()
    }

    fn replies(&mut self) -> Vec<u8> {
        let events: Vec<Event> = self.events.try_iter().collect();
        events
            .into_iter()
            .filter_map(|event| self.note(event))
            .flat_map(String::into_bytes)
            .collect()
    }

    fn note(&mut self, event: Event) -> Option<String> {
        match event {
            Event::Title(title) => {
                self.title = Some(title);
                None
            }
            Event::ResetTitle => {
                self.title = None;
                None
            }
            event => answer(event),
        }
    }

    pub(crate) fn apply(&mut self, edit: Edit) {
        match edit {
            Edit::Resize(cols, rows) => self.term.resize(size(cols, rows)),
            Edit::Scroll(lines) => self.term.scroll_display(Scroll::Delta(lines)),
            Edit::Bottom => self.term.scroll_display(Scroll::Bottom),
            Edit::Select {
                mode,
                column,
                row,
                right,
            } => self.select(mode, column, row, right),
        }
    }

    // The view's top row is a history line while the view is scrolled back.
    fn select(&mut self, mode: SelectMode, column: u16, row: u16, right: bool) {
        let offset = i32::try_from(self.term.grid().display_offset()).unwrap_or_default();
        let last_row = self.term.screen_lines().saturating_sub(1);
        let last_column = self.term.columns().saturating_sub(1);
        let point = Point::new(
            Line(i32::try_from(usize::from(row).min(last_row)).unwrap_or_default() - offset),
            Column(usize::from(column).min(last_column)),
        );
        let side = if right { Side::Right } else { Side::Left };
        let kind = match mode {
            SelectMode::Extend => {
                if let Some(selection) = self.term.selection.as_mut() {
                    selection.update(point, side);
                }
                return;
            }
            SelectMode::Simple => SelectionType::Simple,
            SelectMode::Word => SelectionType::Semantic,
            SelectMode::Line => SelectionType::Lines,
            SelectMode::Block => SelectionType::Block,
        };
        self.term.selection = Some(Selection::new(kind, point, side));
    }

    // A line selection gets a newline after its last line from alacritty. Copying it should not
    // press Enter in a shell, so that one newline goes. Blank lines in the selection stay.
    pub(crate) fn selection_text(&self) -> Option<String> {
        let lines = self
            .term
            .selection
            .as_ref()
            .is_some_and(|selection| selection.ty == SelectionType::Lines);
        let text = self.term.selection_to_string()?;
        match text.strip_suffix('\n') {
            Some(rest) if lines => Some(rest.to_owned()),
            _ => Some(text),
        }
    }

    pub(crate) fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Every line in the history and on the screen, as plain text.
    pub(crate) fn transcript(&self) -> String {
        let history = i32::try_from(self.term.grid().history_size()).unwrap_or(i32::MAX);
        let last_row = i32::try_from(self.term.screen_lines()).unwrap_or(i32::MAX) - 1;
        let last_column = self.term.columns().saturating_sub(1);
        self.term.bounds_to_string(
            Point::new(Line(-history), Column(0)),
            Point::new(Line(last_row), Column(last_column)),
        )
    }

    // A row whose last cell wraps continues on the next row, so the two are one line.
    pub(crate) fn kept(&self) -> crate::Kept {
        let grid = self.term.grid();
        let history = grid.history_size();
        let screen = self.term.screen_lines();
        let first = -i32::try_from(history).unwrap_or(i32::MAX);
        let last_row = i32::try_from(screen).unwrap_or(i32::MAX) - 1;
        let last_column = Column(self.term.columns().saturating_sub(1));
        let mut lines: Vec<crate::KeptLine> = Vec::new();
        let mut wrapped = false;
        for (row, line) in (first..=last_row).enumerate() {
            let text = self.term.bounds_to_string(
                Point::new(Line(line), Column(0)),
                Point::new(Line(line), last_column),
            );
            match lines.last_mut() {
                Some(open) if wrapped => open.text.push_str(&text),
                _ => lines.push(crate::KeptLine { text, row }),
            }
            wrapped = grid[Line(line)][last_column]
                .flags
                .contains(Flags::WRAPLINE);
        }
        crate::Kept {
            lines,
            rows: history + screen,
            on_screen: screen,
        }
    }

    pub(crate) fn grid(&self) -> crate::Grid {
        crate::grid::grid(&self.term)
    }

    pub(crate) fn draw(&self) -> String {
        let grid = self.term.grid();
        // Erase first: a client may be showing a screen of another size.
        let mut out = String::from("\x1b[?25l\x1b[0m\x1b[2J");
        for (flag, code) in MODES {
            let set = if self.term.mode().contains(flag) {
                'h'
            } else {
                'l'
            };
            let _ = write!(out, "\x1b[?{code}{set}");
        }
        out.push_str(if self.term.mode().contains(TermMode::APP_KEYPAD) {
            "\x1b="
        } else {
            "\x1b>"
        });
        let mut style = (
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
            Flags::empty(),
        );
        out.push_str(&sgr(style));
        for (row, line) in (0..).map(Line).take(grid.screen_lines()).enumerate() {
            let _ = write!(out, "\x1b[{};1H", row + 1);
            for column in (0..grid.columns()).map(Column) {
                let cell = &grid[line][column];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let wanted = (cell.fg, cell.bg, painted(cell.flags));
                if wanted != style {
                    style = wanted;
                    out.push_str(&sgr(style));
                }
                out.push(cell.c);
                cell.zerowidth()
                    .into_iter()
                    .flatten()
                    .for_each(|c| out.push(*c));
                // The client may count a symbol's width differently. Placing what follows keeps
                // a disagreement to that one symbol.
                if !cell.c.is_ascii() {
                    let next = column.0
                        + if cell.flags.contains(Flags::WIDE_CHAR) {
                            3
                        } else {
                            2
                        };
                    let _ = write!(out, "\x1b[{};{next}H", row + 1);
                }
            }
        }
        let cursor = grid.cursor.point;
        let _ = write!(out, "\x1b[{};{}H", cursor.line.0 + 1, cursor.column.0 + 1);
        if self.term.mode().contains(TermMode::SHOW_CURSOR) {
            out.push_str("\x1b[?25h");
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Snapshot {
        self.snapshot_within(SNAPSHOT_MAX)
    }

    pub(crate) fn snapshot_within(&self, limit: usize) -> Snapshot {
        let cols = u16::try_from(self.term.columns()).unwrap_or(MAX_COLS);
        let rows = u16::try_from(self.term.screen_lines()).unwrap_or(MAX_ROWS);
        let visible = self.visible_replay();
        let padding = usize::from(rows.saturating_sub(1)).saturating_mul(2);
        if visible.len().saturating_add(padding) > limit {
            return Snapshot {
                cols,
                rows,
                replay: Vec::new(),
                history_dropped: 0,
                screen_kept: false,
            };
        }
        let mut lines = self.history_rows();
        let mut size = packed(&lines, padding);
        let mut dropped = 0;
        while dropped < lines.len() && size + visible.len() > limit {
            size -= lines[dropped].len();
            dropped += 1;
            if dropped == lines.len() {
                size -= padding;
            }
        }
        lines.drain(..dropped);
        let mut replay = Vec::with_capacity(size + visible.len());
        for line in &lines {
            replay.extend_from_slice(line);
        }
        if !lines.is_empty() {
            for _ in 1..rows {
                replay.extend_from_slice(b"\r\n");
            }
        }
        replay.extend_from_slice(&visible);
        Snapshot {
            cols,
            rows,
            replay,
            history_dropped: u32::try_from(dropped).unwrap_or(u32::MAX),
            screen_kept: true,
        }
    }

    // Visible rows as text the parser wraps the same way. `draw` places every row with a
    // cursor address, so a wrapped line looks like a hard break after reload.
    fn visible_replay(&self) -> Vec<u8> {
        let grid = self.term.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        let mut out = Vec::new();
        out.extend_from_slice(b"\x1b[H\x1b[2J\x1b[0m");
        let mut style = (
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
            Flags::empty(),
        );
        for row in 0..rows {
            let Ok(index) = i32::try_from(row) else {
                break;
            };
            push_row(&mut out, grid, Line(index), cols, &mut style);
            if row + 1 == rows && out.ends_with(b"\r\n") {
                out.truncate(out.len() - 2);
            }
        }
        out
    }

    fn history_rows(&self) -> Vec<Vec<u8>> {
        let grid = self.term.grid();
        let cols = grid.columns();
        let history = grid.history_size();
        let mut style = (
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
            Flags::empty(),
        );
        let mut lines = Vec::with_capacity(history);
        for age in (1..=history).rev() {
            let Ok(age) = i32::try_from(age) else {
                break;
            };
            let mut line = Vec::new();
            push_row(&mut line, grid, Line(-age), cols, &mut style);
            lines.push(line);
        }
        lines
    }
}

fn packed(lines: &[Vec<u8>], padding: usize) -> usize {
    lines.iter().map(Vec::len).sum::<usize>() + if lines.is_empty() { 0 } else { padding }
}

fn push_row(
    out: &mut Vec<u8>,
    grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
    line: Line,
    cols: usize,
    style: &mut (Color, Color, Flags),
) {
    let wrapped = cols > 0 && grid[line][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    let end = if wrapped {
        cols
    } else {
        used_end(grid, line, cols)
    };
    for column in 0..end {
        let cell = &grid[line][Column(column)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let wanted = (cell.fg, cell.bg, painted(cell.flags));
        if wanted != *style {
            *style = wanted;
            out.extend(sgr(*style).into_bytes());
        }
        let mut encoded = [0; 4];
        out.extend(cell.c.encode_utf8(&mut encoded).as_bytes());
        if let Some(extra) = cell.zerowidth() {
            for c in extra {
                let mut encoded = [0; 4];
                out.extend(c.encode_utf8(&mut encoded).as_bytes());
            }
        }
    }
    if !wrapped {
        out.extend_from_slice(b"\r\n");
    }
}

fn used_end(
    grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
    line: Line,
    cols: usize,
) -> usize {
    let mut end = 0;
    for column in 0..cols {
        let cell = &grid[line][Column(column)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        if !cell.is_empty() {
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            end = column + width;
        }
    }
    end.min(cols)
}

fn painted(flags: Flags) -> Flags {
    flags.intersection(
        Flags::BOLD
            | Flags::DIM
            | Flags::ITALIC
            | Flags::ALL_UNDERLINES
            | Flags::INVERSE
            | Flags::HIDDEN
            | Flags::STRIKEOUT,
    )
}

fn size(cols: u16, rows: u16) -> TermSize {
    TermSize::new(usize::from(cols), usize::from(rows))
}

// A client never answers the program: this is the only place a query is answered.
// Nothing here knows pixels or the clipboard, so those requests get no reply.
fn answer(event: Event) -> Option<String> {
    match event {
        Event::PtyWrite(text) => Some(text),
        Event::ColorRequest(index, format) => match index {
            256 | 258 => Some(format(FOREGROUND)),
            257 => Some(format(BACKGROUND)),
            _ => None,
        },
        Event::TextAreaSizeRequest(_)
        | Event::ClipboardLoad(..)
        | Event::ClipboardStore(..)
        | Event::MouseCursorDirty
        | Event::Title(_)
        | Event::ResetTitle
        | Event::CursorBlinkingChange
        | Event::Wakeup
        | Event::Bell
        | Event::Exit
        | Event::ChildExit(_) => None,
    }
}

fn sgr((fg, bg, flags): (Color, Color, Flags)) -> String {
    let attributes = ATTRIBUTES
        .iter()
        .filter(|(flag, _)| flags.intersects(*flag))
        .map(|(_, code)| code.to_string());
    let codes: Vec<String> = ["0".to_owned()]
        .into_iter()
        .chain(attributes)
        .chain([color(fg, false), color(bg, true)])
        .collect();
    format!("\x1b[{}m", codes.join(";"))
}

fn color(color: Color, background: bool) -> String {
    let base = if background { 40 } else { 30 };
    match color {
        Color::Named(name) => match name as usize {
            index @ 0..8 => (base + index).to_string(),
            index @ 8..16 => (base + 60 + index - 8).to_string(),
            index @ 259..267 => (base + index - 259).to_string(),
            _ => (base + 9).to_string(),
        },
        Color::Indexed(index) => format!("{};5;{index}", base + 8),
        Color::Spec(Rgb { r, g, b }) => format!("{};2;{r};{g};{b}", base + 8),
    }
}

#[cfg(test)]
mod tests;
