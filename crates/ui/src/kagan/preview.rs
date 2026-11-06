use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, FocusHandle, Image, ImageFormat,
    PathPromptOptions, ScrollHandle, SharedString, Task, Window,
};
use kg_core::Workspace;
use kg_domain::{
    Op, OpError, OpResult, PreviewFile, PreviewImageFormat, PreviewKind, PreviewOp, ThreadId,
};

use super::Kagan;

mod render;

pub(super) const TAB: &str = "preview";
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;
const PIXEL_LIMIT: u64 = 40_000_000;

pub(super) struct Preview {
    /// The thread the request named, or `None` for a path the person picked.
    thread: Option<ThreadId>,
    path: String,
    file: Option<PreviewFile>,
    image: Option<Arc<Image>>,
    /// The sentence shown instead of the file when it cannot be shown.
    notice: Option<SharedString>,
    loading: bool,
    generation: u64,
    load: Task<()>,
    read: Task<()>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    window: AnyWindowHandle,
}

struct Expect {
    generation: u64,
    thread: Option<ThreadId>,
    path: String,
}

impl Kagan {
    pub(super) fn preview_file(
        &mut self,
        _: &super::Preview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.screen.review().is_some() {
            self.preview_review(window, cx);
            return;
        }
        if self.shown_thread(cx).is_none() {
            self.note = Some("Open a thread to preview a file.".into());
            cx.notify();
            return;
        }
        Self::pick_preview(window, cx);
    }

    pub(super) fn preview_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review() {
            let (thread, path) = (Some(review.thread.clone()), review.path.clone());
            self.show_preview(thread, path, window, cx);
        }
    }

    fn pick_preview(window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Preview a file".into()),
        });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            _ = cx.update_window(handle, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.show_preview(None, path.display().to_string(), window, cx);
                });
            });
        })
        .detach();
    }

    pub(crate) fn show_preview(
        &mut self,
        thread: Option<ThreadId>,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shown_thread(cx).is_none() {
            self.note = Some("Open a thread to preview a file.".into());
            cx.notify();
            return;
        }
        self.note = None;
        self.tabs.preview = Some(Preview {
            thread,
            path,
            file: None,
            image: None,
            notice: None,
            loading: false,
            generation: 0,
            load: Task::ready(()),
            read: Task::ready(()),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            window: window.window_handle(),
        });
        self.show_right_tab(TAB, window, cx);
        self.load_preview(cx);
    }

    pub(super) fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.preview = None;
        if self.right_tab.as_deref() == Some(TAB) {
            self.right_tab = self.default_right_tab(cx);
        }
        self.focus_right_tab(window, cx);
        cx.notify();
    }

    fn load_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.tabs.preview.as_mut() else {
            return;
        };
        preview.loading = true;
        preview.generation = preview.generation.wrapping_add(1);
        let expect = Expect {
            generation: preview.generation,
            thread: preview.thread.clone(),
            path: preview.path.clone(),
        };
        let shown = Workspace::dispatch(
            Op::Preview(PreviewOp::Show {
                thread_id: expect.thread.clone(),
                path: expect.path.clone(),
            }),
            cx,
        );
        let handle = preview.window;
        let load = cx.spawn(async move |this, cx| {
            let shown = shown.await;
            _ = cx.update_window(handle, |_, _, cx| {
                _ = this.update(cx, |this, cx| {
                    this.apply_preview(&expect, shown, cx);
                });
            });
        });
        if let Some(preview) = self.tabs.preview.as_mut() {
            preview.load = load;
        }
    }

    fn apply_preview(
        &mut self,
        expect: &Expect,
        shown: Result<OpResult, OpError>,
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self.tabs.preview.as_mut() else {
            return;
        };
        if preview.generation != expect.generation
            || preview.thread != expect.thread
            || preview.path != expect.path
        {
            return;
        }
        preview.loading = false;
        preview.image = None;
        match shown {
            Ok(OpResult::Preview(file)) => {
                preview.path.clone_from(&file.path);
                preview.notice = None;
                preview.file = Some(file);
                self.load_image(expect.generation, cx);
            }
            Ok(_) => {
                // Dispatch answers this Op with Preview, so any other result is a bug.
                preview.notice = Some("Could not read this file.".into());
            }
            Err(error) => {
                preview.notice = Some(format!("Could not read this file: {error}").into());
            }
        }
        cx.notify();
    }

    /// Reads the bytes of the image the Op named, after checking the path again.
    fn load_image(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(preview) = self.tabs.preview.as_ref() else {
            return;
        };
        let Some(file) = preview.file.as_ref() else {
            return;
        };
        let PreviewKind::Image { format, .. } = &file.kind else {
            return;
        };
        let (format, absolute, handle) = (*format, file.absolute.clone(), preview.window);
        let root = match &preview.thread {
            Some(thread) => match environment_path(thread, cx) {
                Some(root) => Some(root),
                None => return self.apply_image(generation, None, cx),
            },
            None => None,
        };
        let read = cx.background_spawn(async move {
            read_image(root.as_deref(), &absolute)
                .map(|bytes| Arc::new(Image::from_bytes(image_format(format), bytes)))
        });
        let load = cx.spawn(async move |this, cx| {
            let image = read.await;
            _ = cx.update_window(handle, |_, _, cx| {
                _ = this.update(cx, |this, cx| {
                    this.apply_image(generation, image, cx);
                });
            });
        });
        if let Some(preview) = self.tabs.preview.as_mut() {
            preview.read = load;
        }
    }

    fn apply_image(&mut self, generation: u64, image: Option<Arc<Image>>, cx: &mut Context<Self>) {
        let Some(preview) = self.tabs.preview.as_mut() else {
            return;
        };
        if preview.generation != generation {
            return;
        }
        if image.is_none() {
            preview.notice = Some("Could not read this image.".into());
        }
        preview.image = image;
        cx.notify();
    }
}

fn read_image(root: Option<&Path>, absolute: &Path) -> Option<Vec<u8>> {
    let absolute = std::fs::canonicalize(absolute).ok()?;
    if let Some(root) = root {
        let root = std::fs::canonicalize(root).ok()?;
        if !absolute.starts_with(root) {
            return None;
        }
    }
    if !std::fs::metadata(&absolute).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(&absolute).ok()?;
    let mut bytes = Vec::new();
    file.take(IMAGE_LIMIT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > IMAGE_LIMIT {
        return None;
    }
    let (width, height) = kg_core::image_size(&bytes)?;
    if u64::from(width) * u64::from(height) > PIXEL_LIMIT {
        return None;
    }
    Some(bytes)
}

fn environment_path(thread: &ThreadId, cx: &App) -> Option<PathBuf> {
    let workspace = Workspace::global(cx).read(cx);
    let id = workspace
        .thread(thread)?
        .read(cx)
        .record()
        .environment_id
        .clone()?;
    Some(workspace.environment(&id)?.read(cx).record().path.clone())
}

fn image_format(format: PreviewImageFormat) -> ImageFormat {
    match format {
        PreviewImageFormat::Png => ImageFormat::Png,
        PreviewImageFormat::Jpeg => ImageFormat::Jpeg,
    }
}

impl Preview {
    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests;
