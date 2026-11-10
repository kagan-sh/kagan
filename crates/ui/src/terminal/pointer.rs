use gpui_kit::{
    ClipboardItem, Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollEvent, Window, px,
};
use kg_core::{Motion, Workspace};
use kg_domain::{Op, OpResult, PreferenceOp, SelectMode, ThreadOp, prefs};
use serde_json::Value;

use super::mouse::{Button, Phase, Spot};
use super::{TerminalView, dispatch, steps};

#[derive(Clone, Copy)]
pub(super) struct Geometry {
    pub(super) origin: Point<Pixels>,
    pub(super) cell: gpui_kit::Size<Pixels>,
}

pub(super) struct Pointer {
    pub(super) geometry: Option<Geometry>,
    pub(super) held: Option<Button>,
    pub(super) selecting: bool,
    pub(super) last: Option<(Spot, bool)>,
    pub(super) wheel: Pixels,
}

impl TerminalView {
    // A program that asked for mouse reports gets the press, unless Shift is held: then the press
    // selects here.
    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some(button) = button_of(event.button) else {
            return;
        };
        let (spot, right) = self.locate(event.position);
        self.pointer.last = Some((spot, right));
        // Cmd-click (Ctrl-click off macOS) on a link opens it instead of reaching the program.
        if button == Button::Left
            && event.modifiers.secondary(cx)
            && let Some(link) = self
                .grid
                .link_at(usize::from(spot.row), usize::from(spot.column))
        {
            Self::open_link(link, cx);
            return;
        }
        if self.grid.mouse.is_some() && !event.modifiers.shift {
            self.pointer.held = Some(button);
            self.report(Some(button), Phase::Press, spot, event.modifiers, cx);
        } else if button == Button::Left {
            self.pointer.selecting = true;
            let mode = match event.click_count {
                1 => SelectMode::Simple,
                2 => SelectMode::Word,
                _ => SelectMode::Line,
            };
            self.select(mode, spot, right, cx);
        }
    }

    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The release is the last word on where a selection ends.
        let (spot, right) = self.locate(event.position);
        if self.pointer.selecting {
            self.finish_selection(spot, right, cx);
        }
        self.pointer.selecting = false;
        let Some(button) = button_of(event.button) else {
            return;
        };
        if self.pointer.held != Some(button) {
            return;
        }
        self.pointer.held = None;
        self.report(Some(button), Phase::Release, spot, event.modifiers, cx);
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (spot, right) = self.locate(event.position);
        // No button is down, so a release went somewhere no listener heard it. End the press here.
        if event.pressed_button.is_none() {
            self.pointer.selecting = false;
            if let Some(button) = self.pointer.held.take() {
                self.report(Some(button), Phase::Release, spot, event.modifiers, cx);
            }
        }
        let previous = self.pointer.last.replace((spot, right));
        let reports = self
            .grid
            .mouse
            .is_some_and(|mouse| self.pointer.held.is_some() || mouse.motion == Motion::All);
        if reports && !event.modifiers.shift {
            // A report names a cell, so the side of the cell does not make a new one.
            if previous.map(|(cell, _)| cell) != Some(spot) {
                self.report(self.pointer.held, Phase::Move, spot, event.modifiers, cx);
            }
        } else if self.pointer.selecting && previous != Some((spot, right)) {
            // The side matters: a press in a character's left half and a drag into its right half
            // select that character.
            self.select(SelectMode::Extend, spot, right, cx);
        }
    }

    // Partial steps wait for the next event.
    pub(super) fn scrolled(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(geometry) = self.pointer.geometry else {
            return;
        };
        let height = geometry.cell.height;
        if height <= px(0.) {
            return;
        }
        let mut rest = self.pointer.wheel + event.delta.pixel_delta(height).y;
        let mut lines = 0_i32;
        while rest >= height {
            rest -= height;
            lines += 1;
        }
        while rest <= -height {
            rest += height;
            lines -= 1;
        }
        self.pointer.wheel = rest;
        if lines == 0 {
            return;
        }
        if self.grid.mouse.is_some() && !event.modifiers.shift {
            let (spot, _) = self.locate(event.position);
            let button = if lines > 0 {
                Button::WheelUp
            } else {
                Button::WheelDown
            };
            for _ in 0..lines.unsigned_abs() {
                self.report(Some(button), Phase::Press, spot, event.modifiers, cx);
            }
        } else {
            dispatch(
                ThreadOp::Scroll {
                    id: self.thread.clone(),
                    lines,
                    terminal: self.terminal,
                },
                cx,
            );
        }
    }

    fn report(
        &self,
        button: Option<Button>,
        phase: Phase,
        spot: Spot,
        mods: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let Some(mouse) = self.grid.mouse else {
            return;
        };
        if let Some(bytes) = super::mouse::report(button, phase, spot, mods, mouse) {
            dispatch(
                ThreadOp::Report {
                    id: self.thread.clone(),
                    bytes,
                    terminal: self.terminal,
                },
                cx,
            );
        }
    }

    // A link opens in the Browser tab when the person turned that on; otherwise the system browser.
    fn open_link(link: String, cx: &mut Context<Self>) {
        let setting = Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: prefs::TERMINAL_LINKS_BROWSER.into(),
            }),
            cx,
        );
        cx.spawn(async move |_, cx| {
            let in_tab = matches!(setting.await, Ok(OpResult::Setting(Value::Bool(true))));
            cx.update(|cx| {
                if in_tab {
                    crate::kagan::with_front(cx, |kagan, window, cx| {
                        kagan.open_link(&link, window, cx);
                    });
                } else {
                    cx.open_url(&link);
                }
            });
        })
        .detach();
    }

    // Copy on select: once the selection is saved, its text goes to the clipboard. Unset counts as on.
    // An empty selection, such as a plain click, copies nothing and leaves the clipboard alone.
    fn finish_selection(&self, spot: Spot, right: bool, cx: &mut Context<Self>) {
        let select = Workspace::dispatch(
            Op::Thread(ThreadOp::Select {
                id: self.thread.clone(),
                mode: SelectMode::Extend,
                column: spot.column,
                row: spot.row,
                right,
                terminal: self.terminal,
            }),
            cx,
        );
        let setting = Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: prefs::TERMINAL_COPY_ON_SELECT.into(),
            }),
            cx,
        );
        let thread = self.thread.clone();
        let terminal = self.terminal;
        cx.spawn(async move |_, cx| {
            let on = !matches!(setting.await, Ok(OpResult::Setting(Value::Bool(false))));
            if select.await.is_err() || !on {
                return;
            }
            let copied = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Thread(ThreadOp::Copy {
                        id: thread,
                        terminal,
                    }),
                    cx,
                )
            });
            if let Ok(OpResult::Text(text)) = copied.await
                && !text.is_empty()
            {
                cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    fn select(&self, mode: SelectMode, spot: Spot, right: bool, cx: &mut Context<Self>) {
        dispatch(
            ThreadOp::Select {
                id: self.thread.clone(),
                mode,
                column: spot.column,
                row: spot.row,
                right,
                terminal: self.terminal,
            },
            cx,
        );
    }

    fn locate(&self, at: Point<Pixels>) -> (Spot, bool) {
        let Some(geometry) = self.pointer.geometry else {
            return (Spot { column: 0, row: 0 }, false);
        };
        let local = at - geometry.origin;
        let cols = self.grid.lines.first().map_or(1, Vec::len);
        let rows = self.grid.lines.len().max(1);
        let column = cell_at(local.x, geometry.cell.width, cols);
        let row = cell_at(local.y, geometry.cell.height, rows);
        let right = local.x - geometry.cell.width * steps(column) > geometry.cell.width * 0.5;
        let spot = Spot {
            column: u16::try_from(column).unwrap_or(u16::MAX),
            row: u16::try_from(row).unwrap_or(u16::MAX),
        };
        (spot, right)
    }
}

fn button_of(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Middle => Some(Button::Middle),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Navigate(_) => None,
    }
}

pub(super) fn cell_at(distance: Pixels, step: Pixels, count: usize) -> usize {
    (0..count)
        .take_while(|index| step * steps(index + 1) <= distance)
        .count()
        .min(count.saturating_sub(1))
}
