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
    let handle = cx
        .update(|cx| window_at(cx, index))
        .ok_or_else(|| OpError::NotFound(format!("window {index}")))?;
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
    sender: &async_channel::Sender<Result<OpResult, OpError>>,
    window: &gpui_kit::Window,
    path: &Path,
) {
    if claimed.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = captured(window, path, true);
    let _ = sender.try_send(result); // The caller may have stopped waiting; then no one reads the result.
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
    let handle = cx
        .update(|cx| window_at(cx, index))
        .ok_or_else(|| OpError::NotFound(format!("window {index}")))?;
    let visible = cx
        .update_window(handle, |_, window, cx| {
            window.refresh();
            crate::ui_dispatch::render_frame(window, cx);
            window.is_visible()
        })
        .map_err(|_| closed())?;
    if !visible {
        return cx
            .update_window(handle, |_, window, _| captured(window, &path, false))
            .map_err(|_| closed())?;
    }
    let (sender, receiver) = async_channel::bounded(1);
    let claimed = Arc::new(AtomicBool::new(false));
    cx.update_window(handle, {
        let sender = sender.clone();
        let claimed = Arc::clone(&claimed);
        let path = path.clone();
        move |_, window, _| {
            window.refresh();
            window.on_next_frame(move |window, _| {
                window.refresh();
                let sender = sender.clone();
                let claimed = Arc::clone(&claimed);
                window.on_next_frame(move |window, _| {
                    publish(&claimed, &sender, window, &path);
                });
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
            let result = cx
                .update_window(handle, |_, window, _| captured(window, &path, false))
                .unwrap_or_else(|_| Err(closed()));
            let _ = sender.try_send(result); // The caller may have stopped waiting; then no one reads the result.
        }
    })
    .detach();
    receiver
        .recv()
        .await
        .map_err(|_| OpError::Failed("screenshot cancelled".into()))?
}

fn captured(window: &gpui_kit::Window, path: &Path, current: bool) -> Result<OpResult, OpError> {
    capture_window(window, path).map(|()| OpResult::Capture {
        path: path.display().to_string(),
        current,
    })
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
    let mut handles = cx.windows();
    handles.sort_by_key(AnyWindowHandle::window_id);
    handles.get(index as usize).copied()
}

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

fn capture_window(window: &gpui_kit::Window, path: &Path) -> Result<(), OpError> {
    #[cfg(target_os = "macos")]
    {
        macos::capture(window, path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::capture(window, path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window, path);
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

    pub(super) fn capture(window: &Window, path: &Path) -> Result<(), OpError> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSView;

        let _marker = MainThreadMarker::new()
            .ok_or_else(|| OpError::Failed("screenshot must run on the main thread".into()))?;
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| OpError::Failed(error.to_string()))?;
        let number = match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => {
                // SAFETY: GPUI's AppKit handle points at the content NSView for this window.
                let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast() };
                view.window()
                    .ok_or_else(|| OpError::Failed("window has no NSWindow".into()))?
                    .windowNumber()
            }
            _ => return Err(OpError::Failed("not an AppKit window".into())),
        };
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
mod windows {
    use std::path::Path;

    use gpui_kit::Window;
    use kg_domain::OpError;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn capture(window: &Window, path: &Path) -> Result<(), OpError> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
            DeleteDC, DeleteObject, GetDIBits, GetWindowDC, HGDIOBJ, ReleaseDC, SelectObject,
        };
        use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT};

        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| OpError::Failed(error.to_string()))?;
        let hwnd = match handle.as_raw() {
            RawWindowHandle::Win32(win32) => HWND(win32.hwnd.get() as _),
            _ => return Err(OpError::Failed("not a Win32 window".into())),
        };
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: `hwnd` is GPUI's live window handle and `rect` is a writable RECT.
        unsafe { GetWindowRect(hwnd, &raw mut rect) }
            .map_err(|error| OpError::Failed(error.to_string()))?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err(OpError::Failed("window has no size".into()));
        }
        // SAFETY: `hwnd` is a live window handle; the DC is released below.
        let hdc = unsafe { GetWindowDC(Some(hwnd)) };
        if hdc.is_invalid() {
            return Err(OpError::Failed("GetWindowDC failed".into()));
        }
        // SAFETY: `hdc` was checked above and the memory DC is deleted below.
        let mem_dc = unsafe { CreateCompatibleDC(Some(hdc)) };
        // SAFETY: both handles are live and the bitmap is deleted below.
        let bitmap = unsafe { CreateCompatibleBitmap(hdc, width, height) };
        // SAFETY: both handles are live and the bitmap is compatible with the DC.
        let _ = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) }; // Returns the old object, which is not needed.
        let printed =
            // SAFETY: `hwnd` is live and `mem_dc` is a compatible memory DC.
            unsafe { PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
        let header = BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>())
                .map_err(|_| OpError::Failed("bitmap header is too large".into()))?,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };
        let stride = (width.cast_unsigned() as usize) * 4;
        let mut pixels = vec![0u8; stride * (height.cast_unsigned() as usize)];
        // SAFETY: `pixels` is writable for `height` rows of `stride` bytes and `info` describes that buffer.
        let lines = unsafe {
            GetDIBits(
                mem_dc,
                bitmap,
                0,
                height.cast_unsigned(),
                Some(pixels.as_mut_ptr().cast()),
                &raw mut info,
                windows::Win32::Graphics::Gdi::DIB_RGB_COLORS,
            )
        };
        // SAFETY: each handle was created above and is released exactly once here.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bitmap.0)); // Cleanup: nothing to undo if it fails.
            let _ = DeleteDC(mem_dc); // Cleanup: nothing to undo if it fails.
            let _ = ReleaseDC(Some(hwnd), hdc); // Cleanup: nothing to undo if it fails.
        }
        printed
            .ok()
            .map_err(|error| OpError::Failed(error.to_string()))?;
        if lines == 0 {
            return Err(OpError::Failed("GetDIBits failed".into()));
        }
        for chunk in pixels.chunks_mut(4) {
            chunk.swap(0, 2);
        }
        let image =
            image::RgbaImage::from_raw(width.cast_unsigned(), height.cast_unsigned(), pixels)
                .ok_or_else(|| OpError::Failed("screenshot buffer is invalid".into()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image
            .save(path)
            .map_err(|error| OpError::Failed(error.to_string()))?;
        Ok(())
    }
}
