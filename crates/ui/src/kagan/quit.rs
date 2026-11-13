use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::base::actions::{Cancel, Confirm};
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, FocusHandle, Global, InteractiveElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div,
};

use super::show;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

struct QuitPrompt;
impl Global for QuitPrompt {}
struct Quitting;
impl Global for Quitting {}

static CONFIRMED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static APP: RefCell<Option<gpui_kit::AsyncApp>> = const { RefCell::new(None) };
    static REQUESTS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn prepare(cx: &mut App) {
    let app = cx.to_async();
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    #[cfg(target_os = "macos")]
    install_should_terminate();
    // The hook lives as long as the app, so it is detached rather than kept in a global.
    cx.on_window_closed(|cx, _| quit_if_none_left(cx)).detach();
}

// macOS keeps the app in the Dock when its last window closes. Elsewhere that close ends kagan
// as Quit does, without the prompt: no window is left to ask in.
fn quit_if_none_left(cx: &mut App) {
    if cfg!(target_os = "macos") {
        return;
    }
    // A plugin's window can outlive kagan's last window, so only kagan's own windows count.
    if super::kagan_windows(cx).is_empty()
        && !cx.has_global::<Quitting>()
        && !CONFIRMED.load(Ordering::SeqCst)
    {
        accept(cx);
    }
}

pub(crate) fn ask(cx: &mut App) {
    // The menu action runs inside the window update, so the dialog waits until that returns.
    cx.defer(present);
}

fn present(cx: &mut App) {
    if cx.has_global::<Quitting>() {
        return;
    }
    if cx.has_global::<QuitPrompt>() {
        let open = super::front(cx).is_some_and(|(handle, _)| {
            cx.update_window(handle, |_, window, cx| window.has_active_dialog(cx))
                .unwrap_or(false)
        });
        if open {
            return;
        }
        decline(cx);
    }
    let running = running(cx);
    // A preview has no one to answer the prompt, so it quits as it would with nothing running.
    if running == 0 || kg_core::Workspace::global(cx).read(cx).is_preview() {
        finish(cx);
        return;
    }
    if show(cx).is_err() {
        return;
    }
    let Some((handle, _)) = super::front(cx) else {
        return;
    };
    let description = consequence(running);
    cx.set_global(QuitPrompt);
    let focus = cx.focus_handle();
    let opened = cx.update_window(handle, move |_, window, cx| {
        let quit = focus.clone();
        window.open_alert_dialog(cx, move |dialog, _, cx| {
            let description = description.clone();
            dialog
                .confirm()
                .title("Quit kagan?")
                .description(description)
                .footer(quit_footer(&quit, cx))
                .on_ok(|_, _, cx| {
                    accept(cx);
                    true
                })
                .on_cancel(|_, _, cx| {
                    decline(cx);
                    true
                })
        });
        window.focus(&focus, cx);
    });
    if opened.is_err() {
        decline(cx);
    }
}

fn quit_footer(focus: &FocusHandle, cx: &App) -> DialogFooter {
    let quit = div()
        .id("quit-confirm")
        .ui_observe_as(Facts {
            focus: Some(focus.clone()),
            ..Facts::button("Quit")
        })
        .track_focus(focus)
        .tab_stop(true)
        .h_6()
        .min_w(gpui_kit::px(80.))
        .px_2()
        .flex()
        .items_center()
        .justify_center()
        .rounded(cx.theme().radius)
        .bg(cx.theme().danger)
        .text_color(cx.theme().danger_foreground)
        .type_body(cx)
        .focus(|quit| quit.border_1().border_color(cx.theme().ring))
        .child("Quit")
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(Confirm { secondary: false }), cx);
        });
    let cancel = div()
        .id("quit-cancel")
        .ui_observe_as(Facts::button("Cancel"))
        .child(
            Button::new("button-quit-cancel")
                .small()
                .outline()
                .label("Cancel")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(Cancel), cx);
                }),
        );
    DialogFooter::new().child(cancel).child(quit)
}

pub(crate) fn finish(cx: &mut App) {
    CONFIRMED.store(true, Ordering::SeqCst);
    #[cfg(test)]
    REQUESTS.with(|requests| requests.set(requests.get() + 1));
    cx.quit();
}

#[cfg(test)]
pub(crate) fn quit_requests() -> usize {
    REQUESTS.with(Cell::get)
}

pub(super) fn accept(cx: &mut App) {
    cx.set_global(Quitting);
    let stop = kg_core::Workspace::stop_for_quit(cx);
    cx.spawn(async move |cx| {
        let result = stop.await;
        cx.update(|cx| {
            decline(cx);
            cx.remove_global::<Quitting>();
            if let Err(error) = result {
                eprintln!("kg: could not stop programs before quitting: {error}");
            }
            finish(cx);
        });
    })
    .detach();
}

fn decline(cx: &mut App) {
    if cx.has_global::<QuitPrompt>() {
        cx.remove_global::<QuitPrompt>();
    }
}

fn running(cx: &App) -> usize {
    kg_core::Workspace::global(cx).read(cx).running_programs(cx)
}

pub(super) fn consequence(running: usize) -> String {
    if running == 1 {
        "1 running program will stop and resume when kagan opens again.".to_string()
    } else {
        format!("{running} running programs will stop and resume when kagan opens again.")
    }
}

#[cfg(target_os = "macos")]
fn install_should_terminate() {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(class) = AnyClass::get(c"GPUIApplicationDelegate") else {
        return;
    };
    // SAFETY: the class is GPUI's app delegate, the selector is AppKit's
    // terminate hook, and the function's ABI matches `Q@:@`.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            objc2::sel!(applicationShouldTerminate:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
                Imp,
            >(application_should_terminate),
            c"Q@:@".as_ptr(),
        )
    };
    if added.as_bool() {
        INSTALLED.store(true, Ordering::SeqCst);
    }
}

#[cfg(target_os = "macos")]
fn dock_reply() -> usize {
    if CONFIRMED.load(Ordering::SeqCst) {
        return 1;
    }
    let app = APP.with(|slot| slot.borrow().clone());
    let Some(app) = app else {
        return 1;
    };
    if app.update(|cx| cx.has_global::<Quitting>()) {
        return 0;
    }
    if app.update(|cx| running(cx)) == 0 {
        CONFIRMED.store(true, Ordering::SeqCst);
        return 1;
    }
    app.update(ask);
    0
}

// `AppKit` calls this on the main thread with the delegate and the application.
#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn application_should_terminate(
    _this: *mut objc2::runtime::AnyObject,
    _cmd: objc2::runtime::Sel,
    _sender: *mut objc2::runtime::AnyObject,
) -> usize {
    dock_reply()
}
