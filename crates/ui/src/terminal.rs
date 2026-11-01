use crate::theme::TypeScale as _;
mod keys;
mod mouse;
mod pointer;

use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, BorderStyle, Bounds, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Font,
    FontStyle, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Pixels, Render, SharedString, StrikethroughStyle, Styled as _,
    Subscription, TextAlign, TextRun, UnderlineStyle, Window, actions, canvas, div, fill, outline,
    point, px, size,
};
use kg_core::{Cell, Color, Grid, Weight, Workspace, WorkspaceEvent};
use kg_domain::{Op, OpResult, TERMINAL_COLS, TERMINAL_ROWS, ThreadId, ThreadOp};

use crate::theme::terminal_color;
use crate::ui_observe::UiObserveExt as _;
use pointer::{Geometry, Pointer};

actions!(
    terminal,
    [Paste, CopySelection, ScrollPageUp, ScrollPageDown]
);

/// The person sent keys to the program. The window holds questions while this keeps coming.
pub(crate) struct Typed;

pub(crate) const CONTEXT: &str = "Terminal";
const LINE_HEIGHT: f32 = 1.3;
// A window thinner than this keeps the program's size. The host shrinks the program to any size
// it is given, and growing back would not restore the lines that were cut.
const MIN_FIT_COLS: u16 = 20;
const MIN_FIT_ROWS: u16 = 5;

/// Whether a window of this size may resize the program. Sizes past the host's limits are clamped.
fn fits_program(cols: u16, rows: u16) -> bool {
    cols >= MIN_FIT_COLS && rows >= MIN_FIT_ROWS
}

/// Whether a window of this size should resize the program now: it fits, and it is not the size
/// already sent.
fn is_new_fit(sent: Option<(u16, u16)>, cols: u16, rows: u16) -> bool {
    fits_program(cols, rows) && sent != Some((cols, rows))
}

/// One running thread's screen. It draws the host's grid and sends keys, pointer and size back.
pub(crate) struct TerminalView {
    thread: ThreadId,
    /// An extra terminal's number; `None` is the program's terminal.
    terminal: Option<u32>,
    grid: Rc<Grid>,
    size: Option<(u16, u16)>,
    font_size: u8,
    focus: FocusHandle,
    pointer: Pointer,
    _screens: Subscription,
    /// Why the last input or paste was not sent, until one is sent.
    notice: Option<String>,
    #[cfg(test)]
    reads: usize,
}

impl EventEmitter<Typed> for TerminalView {}

impl TerminalView {
    pub(crate) fn new(
        thread: ThreadId,
        terminal: Option<u32>,
        font_size: u8,
        cx: &mut Context<Self>,
    ) -> Self {
        let screens = cx.subscribe(&Workspace::global(cx), |this, _, event, cx| {
            if let WorkspaceEvent::Screen(id, terminal) = event
                && *id == this.thread
                && *terminal == this.terminal
            {
                this.refresh(cx);
            }
        });
        let mut this = Self {
            thread,
            terminal,
            grid: Rc::default(),
            size: None,
            font_size,
            focus: cx.focus_handle(),
            pointer: Pointer {
                geometry: None,
                held: None,
                selecting: false,
                last: None,
                wheel: px(0.),
            },
            _screens: screens,
            notice: None,
            #[cfg(test)]
            reads: 0,
        };
        this.refresh(cx);
        this
    }

    pub(crate) fn thread(&self) -> &ThreadId {
        &self.thread
    }

    pub(crate) fn set_font_size(&mut self, size: u8, cx: &mut Context<Self>) {
        if self.font_size != size {
            self.font_size = size;
            cx.notify();
        }
    }

    #[cfg(test)]
    pub(crate) fn focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    /// The size this view last asked the program to take.
    #[cfg(test)]
    pub(crate) fn fitted(&self) -> Option<(u16, u16)> {
        self.size
    }

    #[cfg(test)]
    pub(crate) fn grid(&self) -> &Grid {
        &self.grid
    }

    /// How many times this view has read the host's grid.
    #[cfg(test)]
    pub(crate) fn reads(&self) -> usize {
        self.reads
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        {
            self.reads += 1;
        }
        let Some(grid) = Workspace::global(cx)
            .read(cx)
            .grid(&self.thread, self.terminal)
        else {
            return;
        };
        if *self.grid != grid {
            self.grid = Rc::new(grid);
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = keys::bytes(&event.keystroke, self.grid.app_cursor) {
            cx.stop_propagation();
            self.send(text, cx);
        }
    }

    fn paste_menu(
        &mut self,
        _: &gpui_kit::component::input::Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste(&Paste, window, cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            Self::submit(
                ThreadOp::Paste {
                    id: self.thread.clone(),
                    text: Some(text),
                    file: None,
                    terminal: self.terminal,
                },
                cx,
            );
            cx.emit(Typed);
        }
    }

    fn copy_menu(
        &mut self,
        _: &gpui_kit::component::input::Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy(&CopySelection, window, cx);
    }

    fn copy(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        let copied = Workspace::dispatch(
            Op::Thread(ThreadOp::Copy {
                id: self.thread.clone(),
                terminal: self.terminal,
            }),
            cx,
        );
        cx.spawn(async move |_, cx| {
            if let Ok(OpResult::Text(text)) = copied.await {
                cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    fn page_up(&mut self, _: &ScrollPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(true, cx);
    }

    fn page_down(&mut self, _: &ScrollPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(false, cx);
    }

    // A page keeps one line of the last one in view.
    fn scroll_page(&self, up: bool, cx: &mut Context<Self>) {
        let page = i32::try_from(self.grid.lines.len().max(2) - 1).unwrap_or(1);
        let lines = if up { page } else { -page };
        dispatch(
            ThreadOp::Scroll {
                id: self.thread.clone(),
                lines,
                terminal: self.terminal,
            },
            cx,
        );
    }

    fn send(&mut self, text: String, cx: &mut Context<Self>) {
        Self::submit(
            ThreadOp::Input {
                id: self.thread.clone(),
                text,
                terminal: self.terminal,
            },
            cx,
        );
        cx.emit(Typed);
    }

    // Input and paste can be refused, for example by a program that has ended. The person is told why.
    fn submit(op: ThreadOp, cx: &mut Context<Self>) {
        let sent = Workspace::dispatch(Op::Thread(op), cx);
        cx.spawn(async move |view, cx| {
            let notice = sent.await.err().map(|error| error.to_string());
            view.update(cx, |view, cx| {
                view.notice = notice;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    // The last client to resize sets the size, so a newly shown terminal always sends its own.
    fn fit(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        if !is_new_fit(self.size, cols, rows) {
            return;
        }
        self.size = Some((cols, rows));
        let resize = ThreadOp::Resize {
            id: self.thread.clone(),
            cols,
            rows,
            terminal: self.terminal,
        };
        Workspace::dispatch(Op::Thread(resize), cx).detach();
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let grid = self.grid.clone();
        let focus = self.focus.clone();
        // An extra terminal closes when its shell ends, so it is live while it is shown.
        let live = self.terminal.is_some()
            || Workspace::global(cx)
                .read(cx)
                .thread(&self.thread)
                .is_some_and(|thread| thread.read(cx).running());
        let font = gpui_kit::font(cx.theme().mono_font_family.clone());
        let font_size = px(f32::from(self.font_size));
        div()
            .id("terminal")
            .ui_container()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .bg(terminal_color(Color::Background, cx))
            .pl_4()
            .pt_2()
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::paste_menu))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::copy_menu))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_any_mouse_down(cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll(cx.listener(Self::scrolled))
            .child(
                div().id("terminal-grid").ui_container().size_full().child(
                    canvas(
                        move |bounds, window, cx| {
                            let cell = cell_size(&font, font_size, window);
                            let (cols, rows) = dimensions(bounds.size, cell);
                            view.update(cx, |view, cx| {
                                view.pointer.geometry = Some(Geometry {
                                    origin: bounds.origin,
                                    cell,
                                });
                                view.fit(cols, rows, cx);
                            });
                            (font, cell)
                        },
                        move |bounds, (font, cell), window, cx| {
                            paint(&grid, bounds, cell, window, cx);
                            if draws_cursor(live, grid.cursor.is_some())
                                && let Some(cursor) = grid.cursor
                            {
                                let origin = bounds.origin
                                    + point(
                                        cell.width * steps(cursor.column),
                                        cell.height * steps(cursor.line),
                                    );
                                let bounds = Bounds::new(origin, cell);
                                let color = crate::theme::cursor_color(cx).opacity(0.6);
                                window.paint_quad(
                                    if focus.is_focused(window) && window.is_window_active() {
                                        fill(bounds, color)
                                    } else {
                                        outline(bounds, color, BorderStyle::Solid)
                                    },
                                );
                            }
                            paint_glyphs(&grid, bounds, &font, font_size, cell, window, cx);
                        },
                    )
                    .size_full(),
                ),
            )
            .children(self.notice.as_ref().map(|notice| {
                div()
                    .id("terminal-notice")
                    .ui_container()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .px_4()
                    .py_2()
                    .bg(cx.theme().muted)
                    .type_small()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Not sent: {notice}."))
            }))
    }
}

/// Sends a thread's operation without waiting for it: the window shows what the host reports.
pub(super) fn dispatch(op: ThreadOp, cx: &mut App) {
    Workspace::dispatch(Op::Thread(op), cx).detach();
}

fn cell_size(font: &Font, font_size: Pixels, window: &Window) -> gpui_kit::Size<Pixels> {
    let text = window.text_system();
    let advance = text
        .advance(text.resolve_font(font), font_size, 'm')
        .map(|advance| advance.width)
        .ok();
    cell_size_from(advance, font_size)
}

/// A cell is one measured `m` wide. A font that measures nothing gets 0.6 em.
fn cell_size_from(advance: Option<Pixels>, font_size: Pixels) -> gpui_kit::Size<Pixels> {
    let width = advance
        .filter(|width| *width > px(0.))
        .unwrap_or(font_size * 0.6);
    size(width, font_size * LINE_HEIGHT)
}

fn dimensions(area: gpui_kit::Size<Pixels>, cell: gpui_kit::Size<Pixels>) -> (u16, u16) {
    let fits = |length: Pixels, step: Pixels, max: u16| {
        (1..=max)
            .take_while(|count| step * f32::from(*count) <= length)
            .last()
            .unwrap_or(0)
    };
    (
        fits(area.width, cell.width, *TERMINAL_COLS.end()).max(*TERMINAL_COLS.start()),
        fits(area.height, cell.height, *TERMINAL_ROWS.end()).max(*TERMINAL_ROWS.start()),
    )
}

pub(super) fn steps(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Style {
    fg: Color,
    weight: Weight,
    italic: bool,
    underline: bool,
    strikeout: bool,
}

impl Style {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            weight: cell.weight,
            italic: cell.italic,
            underline: cell.underline,
            strikeout: cell.strikeout,
        }
    }
}

/// A line's text with its styles, as byte lengths.
fn runs(line: &[Cell]) -> (String, Vec<(usize, Style)>) {
    let mut text = String::new();
    let mut runs: Vec<(usize, Style)> = Vec::new();
    for cell in line {
        let start = text.len();
        text.push(cell.c);
        text.extend(&cell.zerowidth);
        let len = text.len() - start;
        let style = Style::of(cell);
        match runs.last_mut() {
            Some((run, last)) if *last == style => *run += len,
            _ => runs.push((len, style)),
        }
    }
    (text, runs)
}

/// Columns whose background is not the window's: (first column, columns, color).
fn backgrounds(line: &[Cell]) -> Vec<(usize, usize, Color)> {
    let mut spans: Vec<(usize, usize, Color)> = Vec::new();
    for (column, cell) in line.iter().enumerate() {
        if cell.bg == Color::Background {
            continue;
        }
        match spans.last_mut() {
            Some((start, len, color)) if *color == cell.bg && *start + *len == column => {
                *len += 1;
            }
            _ => spans.push((column, 1, cell.bg)),
        }
    }
    spans
}

fn text_run(len: usize, style: Style, font: &Font, cx: &App) -> TextRun {
    let color = terminal_color(style.fg, cx);
    TextRun {
        len,
        font: Font {
            weight: if style.weight == Weight::Bold {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            style: if style.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            ..font.clone()
        },
        color: if style.weight == Weight::Dim {
            color.opacity(0.6)
        } else {
            color
        },
        background_color: None,
        underline: style.underline.then_some(UnderlineStyle {
            thickness: px(1.),
            color: None,
            wavy: false,
        }),
        strikethrough: style.strikeout.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        }),
    }
}

pub(crate) fn draws_cursor(live: bool, has_cursor: bool) -> bool {
    live && has_cursor
}

fn paint(
    grid: &Grid,
    bounds: Bounds<Pixels>,
    cell: gpui_kit::Size<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let at = |line: usize, column: usize| {
        bounds.origin + point(cell.width * steps(column), cell.height * steps(line))
    };
    for (index, line) in grid.lines.iter().enumerate() {
        for (column, columns, color) in backgrounds(line) {
            let width = cell.width * steps(columns);
            let area = Bounds::new(at(index, column), size(width, cell.height));
            window.paint_quad(fill(area, terminal_color(color, cx)));
        }
    }
    for (index, columns) in &grid.selection {
        let width = cell.width * steps(columns.len());
        let area = Bounds::new(at(*index, columns.start), size(width, cell.height));
        window.paint_quad(fill(area, cx.theme().selection));
    }
}

fn paint_glyphs(
    grid: &Grid,
    bounds: Bounds<Pixels>,
    font: &Font,
    font_size: Pixels,
    cell: gpui_kit::Size<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    for (index, line) in grid.lines.iter().enumerate() {
        let (text, styles) = runs(line);
        let styles: Vec<TextRun> = styles
            .into_iter()
            .map(|(len, style)| text_run(len, style, font, cx))
            .collect();
        let shaped = window.text_system().shape_line(
            SharedString::from(text),
            font_size,
            &styles,
            Some(cell.width),
        );
        let origin = bounds.origin + point(px(0.), cell.height * steps(index));
        let _ = shaped.paint(origin, cell.height, TextAlign::Left, None, window, cx);
    }
}

#[cfg(test)]
mod tests;
