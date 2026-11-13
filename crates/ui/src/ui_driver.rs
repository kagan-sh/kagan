use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, App, AppContext as _, ElementId, ScrollDelta, Task, point, px};
use kg_core::{UiJob, Workspace};
use kg_domain::{BrowserOp, OpError, OpResult, SurfaceOp, UI_PERSON_ONLY, UI_UNAVAILABLE, UiOp};

pub(crate) fn install(cx: &mut App) {
    Workspace::global(cx).update(cx, |workspace, _| {
        workspace.set_ui_driver(Rc::new(|job, cx| match job {
            UiJob::NewTheme { id, from } => crate::theme::new_theme(&id, from.as_deref(), cx),
            UiJob::Preview { thread, path } => {
                crate::show_preview(thread, path, cx);
                Task::ready(Ok(OpResult::Done))
            }
            UiJob::Browser(op) => browser(op, cx),
            UiJob::Op(UiOp::Screenshot { window, path })
                if crate::presentation::is_offscreen(cx) =>
            {
                Task::ready(capture_offscreen(window, &path, cx))
            }
            UiJob::Op(UiOp::Screenshot { window, path }) => {
                cx.spawn(async move |cx| capture_after_present(window, path, cx).await)
            }
            UiJob::Op(op) => cx.spawn(async move |cx| drive(op, cx)),
            UiJob::Surface(op) => cx.spawn(async move |cx| drive_surface(op, cx)),
        }));
    });
}

fn browser(op: BrowserOp, cx: &mut App) -> Task<Result<OpResult, OpError>> {
    cx.spawn(async move |cx| crate::kagan::browser::agent::run(op, cx).await)
}

fn closed() -> OpError {
    OpError::Failed("window closed".into())
}

fn drive(op: UiOp, cx: &mut gpui_kit::AsyncApp) -> Result<OpResult, OpError> {
    let _armed = crate::synthetic::arm();
    let index = window_index(&op);
    let handle = cx.update(|cx| window_at(cx, index).ok_or_else(|| missing_window(cx, index)))?;
    let result = cx
        .update_window(handle, |_, window, cx| {
            crate::synthetic::with_active(|| run_op(op, window, cx))
        })
        .map_err(|_| closed())?;
    if crate::synthetic::take_person_blocked() {
        return Err(OpError::Invalid(UI_PERSON_ONLY.into()));
    }
    result
}

fn drive_surface(op: SurfaceOp, cx: &mut gpui_kit::AsyncApp) -> Result<OpResult, OpError> {
    cx.update(|cx| {
        let Some((handle, kagan)) = crate::kagan::front(cx) else {
            return Err(OpError::Failed(UI_UNAVAILABLE.into()));
        };
        match cx.update_window(handle, |_, window, cx| {
            let step = kagan.update(cx, |kagan, cx| kagan.commit(op, None, window, cx))?;
            match step {
                crate::kagan::SurfaceCommit::Ready(result) => Ok(*result),
                crate::kagan::SurfaceCommit::Paint => {
                    crate::kagan::paint_then_settle(&kagan, window, cx)
                }
            }
        }) {
            Ok(inner) => inner,
            Err(_) => Err(closed()),
        }
    })
}

fn publish(
    claimed: &AtomicBool,
    sender: &async_channel::Sender<Result<(Target, bool), OpError>>,
    window: &gpui_kit::Window,
) {
    if claimed.swap(true, Ordering::SeqCst) {
        return;
    }
    let target = window_target(window).map(|target| (target, true));
    let _ = sender.try_send(target); // The caller may have stopped waiting; then no one reads the result.
}

// A covered, minimized, or sleeping window stops its display link, so
// `on_next_frame` never runs. Capture the last presented buffer instead of
// waiting for a frame that will not come. If a visible window still does not
// present, capture that buffer rather than failing the screenshot.
async fn capture_after_present(
    index: u32,
    path: PathBuf,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<OpResult, OpError> {
    let handle = cx.update(|cx| window_at(cx, index).ok_or_else(|| missing_window(cx, index)))?;
    let visible = cx
        .update_window(handle, |_, window, cx| {
            window.refresh();
            crate::ui_dispatch::render_frame(window, cx);
            window.is_visible()
        })
        .map_err(|_| closed())?;
    if !visible {
        let target = target_in(handle, cx)?;
        return shoot(target, path, false, cx).await;
    }
    let (sender, receiver) = async_channel::bounded(1);
    let claimed = Arc::new(AtomicBool::new(false));
    cx.update_window(handle, {
        let sender = sender.clone();
        let claimed = Arc::clone(&claimed);
        move |_, window, _| {
            window.refresh();
            window.on_next_frame(move |window, _| {
                window.refresh();
                let sender = sender.clone();
                let claimed = Arc::clone(&claimed);
                window.on_next_frame(move |window, _| publish(&claimed, &sender, window));
            });
        }
    })
    .map_err(|_| closed())?;
    cx.spawn({
        let sender = sender.clone();
        let claimed = Arc::clone(&claimed);
        async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if claimed.swap(true, Ordering::SeqCst) {
                return;
            }
            let target = target_in(handle, cx).map(|target| (target, false));
            let _ = sender.try_send(target); // The caller may have stopped waiting; then no one reads the result.
        }
    })
    .detach();
    let (target, current) = receiver
        .recv()
        .await
        .map_err(|_| OpError::Failed("screenshot cancelled".into()))??;
    shoot(target, path, current, cx).await
}

// The native id is read on the window's thread; the capture itself runs off it.
fn target_in(handle: AnyWindowHandle, cx: &mut gpui_kit::AsyncApp) -> Result<Target, OpError> {
    cx.update_window(handle, |_, window, _| window_target(window))
        .map_err(|_| closed())?
}

async fn shoot(
    target: Target,
    path: PathBuf,
    current: bool,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<OpResult, OpError> {
    cx.background_executor()
        .spawn(async move { captured(target, &path, current) })
        .await
}

fn captured(target: Target, path: &Path, current: bool) -> Result<OpResult, OpError> {
    capture_target(target, path).map(|()| OpResult::Capture {
        path: path.display().to_string(),
        current,
    })
}

// An offscreen window is never presented, so there is no frame to wait for: render the scene now
// and write what it drew.
fn capture_offscreen(index: u32, path: &Path, cx: &mut App) -> Result<OpResult, OpError> {
    #[cfg(target_os = "macos")]
    {
        let handle = window_at(cx, index).ok_or_else(|| missing_window(cx, index))?;
        let image = cx
            .update_window(handle, |_, window, cx| {
                crate::ui_dispatch::render_frame(window, cx);
                window.render_to_image()
            })
            .map_err(|_| closed())?
            .map_err(|error| OpError::Failed(error.to_string()))?;
        write_png(&image, path)?;
        Ok(OpResult::Capture {
            path: path.display().to_string(),
            current: true,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (index, path, cx);
        Err(OpError::Failed(kg_domain::UI_SCREENSHOT_UNAVAILABLE.into()))
    }
}

// The image is staged beside the destination and renamed over it only once it is written, as the
// window capture does, so an older file is never reported as the new capture.
#[cfg(target_os = "macos")]
fn write_png(image: &image::RgbaImage, path: &Path) -> Result<(), OpError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(folder)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let staged = folder.join(format!("kg-capture-{}-{nanos}.png", std::process::id()));
    let written = image
        .save_with_format(&staged, image::ImageFormat::Png)
        .map_err(|error| OpError::Failed(error.to_string()));
    if written.is_err() {
        let _ = std::fs::remove_file(&staged); // Best effort: the error to report is `written`.
        return written;
    }
    std::fs::rename(&staged, path).map_err(OpError::from)
}

fn window_index(op: &UiOp) -> u32 {
    match op {
        UiOp::Snapshot { window }
        | UiOp::Click { window, .. }
        | UiOp::Type { window, .. }
        | UiOp::Key { window, .. }
        | UiOp::Scroll { window, .. }
        | UiOp::Drag { window, .. }
        | UiOp::Drop { window }
        | UiOp::Screenshot { window, .. } => *window,
    }
}

fn window_at(cx: &App, index: u32) -> Option<AnyWindowHandle> {
    crate::kagan::kagan_windows(cx).get(index as usize).copied()
}

fn missing_window(cx: &App, index: u32) -> OpError {
    OpError::NotFound(no_window_message(
        index,
        crate::kagan::kagan_windows(cx).len(),
    ))
}

// An offscreen app starts with no window until `kg open` makes one, so say that when there is none.
fn no_window_message(index: u32, open: usize) -> String {
    match open {
        0 => "no window yet: run kg open".into(),
        open => format!(
            "no window {index}; the open windows are numbered 0 to {}",
            open - 1
        ),
    }
}

#[cfg(test)]
mod tests;

fn run_op(op: UiOp, window: &mut gpui_kit::Window, cx: &mut App) -> Result<OpResult, OpError> {
    crate::ui_dispatch::render_frame(window, cx);
    match op {
        UiOp::Snapshot { window: index } => {
            Ok(OpResult::Ui(crate::ui_dispatch::snapshot(window, index)))
        }
        UiOp::Click {
            element_id, within, ..
        } => {
            let element_id = ElementId::from(element_id);
            let within = within.map(ElementId::from);
            crate::ui_dispatch::click_within(window, &element_id, within.as_ref(), cx)?;
            flush(window, cx);
            Ok(OpResult::Done)
        }
        UiOp::Type { text, .. } => {
            crate::ui_dispatch::input(window, &text, cx)?;
            Ok(OpResult::Done)
        }
        UiOp::Key { keystroke, .. } => {
            crate::ui_dispatch::press(window, &keystroke, cx)?;
            flush(window, cx);
            Ok(OpResult::Done)
        }
        UiOp::Scroll {
            element_id,
            within,
            dy,
            ..
        } => {
            let delta = ScrollDelta::Pixels(point(px(0.), px(dy)));
            let element_id = ElementId::from(element_id);
            let within = within.map(ElementId::from);
            crate::ui_dispatch::scroll_within(window, &element_id, within.as_ref(), delta, cx)?;
            Ok(OpResult::Done)
        }
        UiOp::Drag {
            from,
            to,
            at,
            within,
            hold,
            ..
        } => {
            let within = within.map(ElementId::from);
            crate::ui_dispatch::drag_within(
                window,
                &ElementId::from(from),
                &ElementId::from(to),
                within.as_ref(),
                at,
                hold,
                cx,
            )?;
            flush(window, cx);
            Ok(OpResult::Done)
        }
        UiOp::Drop { .. } => {
            crate::ui_dispatch::drop_held(window, cx)?;
            flush(window, cx);
            Ok(OpResult::Done)
        }
        UiOp::Screenshot { .. } => Err(OpError::Failed(UI_UNAVAILABLE.into())),
    }
}

fn flush(window: &mut gpui_kit::Window, cx: &mut App) {
    for _ in 0..8 {
        crate::ui_dispatch::render_frame(window, cx);
    }
}

type Target = isize;

fn window_target(window: &gpui_kit::Window) -> Result<Target, OpError> {
    #[cfg(target_os = "macos")]
    {
        macos::number(window)
    }
    #[cfg(target_os = "windows")]
    {
        windows::hwnd(window)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        Err(OpError::Failed(kg_domain::UI_SCREENSHOT_UNAVAILABLE.into()))
    }
}

fn capture_target(target: Target, path: &Path) -> Result<(), OpError> {
    #[cfg(target_os = "macos")]
    {
        macos::capture(target, path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::capture(target, path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (target, path);
        Err(OpError::Failed(kg_domain::UI_SCREENSHOT_UNAVAILABLE.into()))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::{Command, ExitStatus, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    use gpui_kit::Window;
    use kg_domain::OpError;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn number(window: &Window) -> Result<isize, OpError> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSView;

        let _marker = MainThreadMarker::new()
            .ok_or_else(|| OpError::Failed("screenshot must run on the main thread".into()))?;
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| OpError::Failed(error.to_string()))?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => {
                // SAFETY: GPUI's AppKit handle points at the content NSView for this window.
                let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast() };
                Ok(view
                    .window()
                    .ok_or_else(|| OpError::Failed("window has no NSWindow".into()))?
                    .windowNumber())
            }
            _ => Err(OpError::Failed("not an AppKit window".into())),
        }
    }

    pub(super) fn capture(number: isize, path: &Path) -> Result<(), OpError> {
        run(OsStr::new("screencapture"), number, path)
    }

    fn run(program: &OsStr, number: isize, path: &Path) -> Result<(), OpError> {
        let folder = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        std::fs::create_dir_all(folder)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let staged = folder.join(format!("kg-capture-{}-{nanos}.png", std::process::id()));
        let output = Command::new(program)
            .args(["-x", "-o", "-l", &number.to_string()])
            .arg(&staged)
            .output()
            .map_err(|error| OpError::Failed(error.to_string()));
        let finished = output.and_then(|output| finish(&output, &staged, path));
        if finished.is_err() {
            let _ = std::fs::remove_file(&staged); // Best effort: the staged file is temporary, and the error to report is `finished`.
        }
        finished
    }

    fn finish(output: &Output, staged: &Path, path: &Path) -> Result<(), OpError> {
        if !output.status.success() {
            return Err(OpError::Failed(failure(&output.stderr, output.status)));
        }
        if !staged.is_file() {
            return Err(OpError::Failed(format!(
                "screencapture wrote no image to {}",
                path.display()
            )));
        }
        std::fs::rename(staged, path).map_err(OpError::from)
    }

    fn failure(stderr: &[u8], status: ExitStatus) -> String {
        let reason = String::from_utf8_lossy(stderr);
        let reason = reason.trim();
        if reason.is_empty() {
            format!("screencapture failed: {status}")
        } else {
            format!("screencapture failed: {reason}")
        }
    }

    #[cfg(test)]
    mod tests;
}

#[cfg(target_os = "windows")]
mod windows;
