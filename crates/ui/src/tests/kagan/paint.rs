use super::chord;
use crate::kagan::screen_read;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use gpui_kit::{InputEvent as _, Modifiers, MouseMoveEvent};
#[cfg(unix)]
use kg_core::Color;
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Op, OpResult, PreferenceOp, ThreadId};

use super::{
    click, count_repaints, grid_text, host_grid, input, line, open, run_shell, set_preference,
    split_command, wait_for_screen, wait_until,
};

#[cfg(unix)]
#[gpui_kit::test]
fn cells_paint_their_background_where_they_are(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    // A blue cell on row 2, a two-column red span on row 3, and a green cell four columns right.
    let script = "printf '\\033[2;2H\\033[44m \\033[0m\\033[3;2H\\033[41m  \\033[0m\\033[3;6H\\033[42m \\033[0m'\n";
    cx.update(|cx| Workspace::dispatch(input(&id, script), cx).detach());
    let red = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Red), cx));
    let green = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Green), cx));
    let blue = cx.update(|cx| crate::theme::terminal_color(Color::Ansi(kg_core::Ansi::Blue), cx));
    wait_until(cx, |cx| {
        let quads = painted(cx, window);
        [red, green, blue]
            .iter()
            .all(|color| quads.iter().any(|quad| quad.background == (*color).into()))
    });
    let quads = painted(cx, window);
    let place = |color: gpui_kit::Hsla| {
        let quad = quads
            .iter()
            .find(|quad| quad.background == color.into())
            .expect("a painted cell");
        (
            quad.bounds.origin.x.as_f32(),
            quad.bounds.origin.y.as_f32(),
            quad.bounds.size.width.as_f32(),
        )
    };
    let (red_x, red_y, span) = place(red);
    let (green_x, green_y, cell) = place(green);
    let (_, blue_y, _) = place(blue);
    // A two-column span is twice as wide as one cell.
    assert!(span > 1.5 * cell, "span {span} cell {cell}");
    // Quads snap to whole pixels, so each of the four columns may be a pixel off.
    assert!(
        (green_x - red_x - 4. * cell).abs() <= 4.,
        "red {red_x} green {green_x} cell {cell}"
    );
    assert!((green_y - red_y).abs() < 0.01);
    // Row three is below row two.
    assert!(red_y > blue_y, "red {red_y} blue {blue_y}");
}

pub(in crate::tests) fn present(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> bool {
    cx.update_window(window, |_, window, _| {
        !matches!(
            crate::ui_registry::locate(window, &ElementId::Name(name.to_owned().into()), None),
            crate::ui_registry::Located::Missing
        )
    })
    .unwrap()
}

pub(in crate::tests) fn shows(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .is_some_and(|element| element.visible)
    })
    .unwrap()
}

/// The value `kg ui` reports for an element, if it has one.
pub(in crate::tests) fn value_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.value.as_deref().map(str::to_owned))
    })
    .unwrap()
}

/// Whether `kg ui` reports an element as focused, if it reports focus at all.
pub(in crate::tests) fn focused_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.focused)
    })
    .unwrap()
}

/// The role `kg ui` reports for an element, if it has one.
pub(in crate::tests) fn role_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<gpui_kit::Role> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.role)
    })
    .unwrap()
}

/// Whether `kg ui` reports an element as selected, if it reports selection at all.
pub(in crate::tests) fn selected_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.selected)
    })
    .unwrap()
}

pub(in crate::tests) fn label(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> String {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label.as_deref().map(str::to_owned))
            .unwrap_or_default()
    })
    .unwrap()
}

pub(in crate::tests) fn row_top(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect("a row")
            .bounds
            .origin
            .y
            .as_f32()
    })
    .unwrap()
}

/// Whether a quad of `color` paints at the top edge of the named row.
pub(in crate::tests) fn quad_at_row(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    color: gpui_kit::Hsla,
) -> bool {
    let (top, scale) = cx
        .update_window(window, |_, window, _| {
            let row = crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
                .expect("a row");
            (row.bounds.origin.y.as_f32(), window.scale_factor())
        })
        .unwrap();
    painted(cx, window)
        .iter()
        .filter(|quad| quad.background == color.into())
        .any(|quad| (quad.bounds.origin.y.as_f32() - top * scale).abs() < scale)
}

pub(in crate::tests) fn highlighted_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> bool {
    let accent = cx.update(|cx| cx.theme().sidebar_accent);
    quad_at_row(cx, window, name, accent)
}

/// Moves the pointer over the named element, the way a person's pointer would.
pub(in crate::tests) fn point_at(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        let target = crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect("an element to point at")
            .bounds
            .center();
        window.dispatch_event(
            MouseMoveEvent {
                position: target,
                pressed_button: None,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn painted(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) -> Vec<gpui_kit::Quad> {
    cx.update_window(window, |_, window, _| window.painted_quads())
        .unwrap()
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_light_fjord_terminal_paints_its_dark_background(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(
            kg_domain::Op::Theme(kg_domain::ThemeOp::Set {
                theme: "fjord".into(),
                mode: kg_domain::Appearance::Light,
            }),
            cx,
        )
        .detach();
    });
    let terminal: gpui_kit::Hsla = gpui_kit::rgb(0x1a_1d24).into();
    wait_until(cx, |cx| {
        painted(cx, window)
            .iter()
            .any(|quad| quad.background == terminal.into())
    });
}

#[gpui_kit::test]
fn a_screen_for_another_thread_does_not_repaint_the_shown_one(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let shown = run_shell(cx);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", shown.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let view = cx
        .update(|cx| screen_read::terminal_view(kagan.read(cx)))
        .expect("a thread is shown");
    let (repaints, _subscription) = count_repaints(cx, &view);
    let reads = cx.update(|cx| view.read(cx).reads());

    cx.update(|cx| {
        Workspace::global(cx).update(cx, |_, cx| {
            cx.emit(WorkspaceEvent::Screen(other.clone(), None));
        });
    });
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| view.read(cx).reads()),
        reads,
        "the shown view read a grid it does not show"
    );
    assert_eq!(
        repaints.get(),
        0,
        "the shown view repainted for another thread's screen"
    );
}

#[gpui_kit::test]
fn a_screen_for_the_shown_thread_reads_and_repaints_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let view = cx
        .update(|cx| screen_read::terminal_view(kagan.read(cx)))
        .expect("a thread is shown");
    let (repaints, _subscription) = count_repaints(cx, &view);
    let reads = cx.update(|cx| view.read(cx).reads());

    // Typed text changes the host's screen, which sends the shown thread a screen event.
    cx.simulate_input(window, &split_command("ke", "yed"));
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains(&split_command("ke", "yed"))
    });
    assert!(
        cx.update(|cx| view.read(cx).reads()) > reads,
        "the shown view never read its screen"
    );
    assert!(repaints.get() > 0, "the shown view never repainted");
}

#[gpui_kit::test]
fn a_resize_reaches_the_host_at_the_size_the_view_fitted(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let before = host_dimensions(cx, &id);

    // The user's resize: the platform reports the new size, and the window lays out again.
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    // The host may pass through an intermediate size first, so wait for the fitted size itself.
    wait_until(cx, |cx| {
        let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));
        let now = host_dimensions(cx, &id);
        now != before && fitted == Some(now)
    });
    let fitted = cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx));
    assert_eq!(Some(host_dimensions(cx, &id)), fitted);
    let after = host_dimensions(cx, &id);
    assert!(
        after.0 > before.0 && after.1 > before.1,
        "{before:?} -> {after:?}"
    );
}

#[gpui_kit::test]
fn terminal_text_size_keys_save_and_refit_the_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::terminal_fitted(kagan.read(cx), cx))
            .is_some()
    });
    let before = host_dimensions(cx, &id);
    cx.simulate_keystrokes(window, &chord("Bigger"));
    wait_until(cx, |cx| host_dimensions(cx, &id) != before);
    let larger = host_dimensions(cx, &id);
    assert!(
        larger.0 < before.0 && larger.1 < before.1,
        "{before:?} -> {larger:?}"
    );
    let saved = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: "terminal-font-size".into(),
            }),
            cx,
        )
    });
    assert_eq!(
        cx.foreground_executor().block_test(saved).unwrap(),
        OpResult::Setting(serde_json::json!(13))
    );
    cx.simulate_keystrokes(window, &chord("Smaller"));
    wait_until(cx, |cx| host_dimensions(cx, &id) == before);
    if cfg!(target_os = "macos") {
        cx.simulate_keystrokes(window, "cmd-+");
        wait_until(cx, |cx| host_dimensions(cx, &id) == larger);
    }
    set_preference(cx, "terminal-font-size", serde_json::json!(20));
    wait_until(cx, |cx| host_dimensions(cx, &id).0 < larger.0);
    cx.simulate_keystrokes(window, &chord("ActualSize"));
    wait_until(cx, |cx| host_dimensions(cx, &id) == before);
}

pub(in crate::tests) fn host_dimensions(cx: &mut TestAppContext, id: &ThreadId) -> (u16, u16) {
    let grid = host_grid(cx, id);
    (
        u16::try_from(grid.lines[0].len()).unwrap(),
        u16::try_from(grid.lines.len()).unwrap(),
    )
}

#[gpui_kit::test]
fn switching_threads_shows_each_threads_own_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&first, &line(&split_command("fir", "st-out"))), cx).detach();
    });
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("first-out"));

    click(cx, window, &format!("thread-{}", second.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&second, &line(&split_command("bra", "vo-out"))), cx).detach();
    });
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("bravo-out"));
    assert!(!grid_text(cx, &kagan).contains("first-out"));

    click(cx, window, &format!("thread-{}", first.0));
    wait_for_screen(cx, &kagan, |cx| grid_text(cx, &kagan).contains("first-out"));
    assert!(!grid_text(cx, &kagan).contains("bravo-out"));
}

#[gpui_kit::test]
fn a_thread_keeps_its_screen_when_the_window_closes_and_reopens(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        Workspace::dispatch(input(&id, &line(&split_command("kee", "p-out"))), cx).detach();
    });
    wait_until(cx, |cx| {
        host_grid(cx, &id).lines.iter().any(|row| {
            row.iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .contains("keep-out")
        })
    });

    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let reopened = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    click(cx, window, &format!("thread-{}", id.0));
    wait_for_screen(cx, &reopened, |cx| {
        grid_text(cx, &reopened).contains("keep-out")
    });
}
