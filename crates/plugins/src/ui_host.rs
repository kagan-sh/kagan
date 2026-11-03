use std::collections::BTreeMap;
use std::future::Future;

use async_channel::Sender;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, Global, Pixels, Size,
    WindowId,
};
use gpui_shell::{HostArguments, HostError, HostModule, HostValue, with_current_app};
use kg_core::Workspace;
use kg_domain::{DiffSide, Op, PluginId, ProjectId, ThreadId, ThreadOp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::area::{from_host, to_host};

/// Same cap as `kg thread paste` (`crates/kg/src/development.rs`).
const PASTE_CAP: usize = 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<ThreadId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<ProjectId>,
    focused: bool,
    theme: String,
    #[serde(skip)]
    props: BTreeMap<PluginId, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToastAction {
    label: String,
    command: String,
}

impl ToastAction {
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }
}

pub enum HostUiEvent {
    OpenThread {
        id: ThreadId,
        reply: Sender<Result<(), String>>,
    },
    OpenPanel {
        plugin: PluginId,
        panel: String,
        props: Value,
        reply: Sender<Result<(), String>>,
    },
    Toast {
        plugin: PluginId,
        message: String,
        action: Option<ToastAction>,
        reply: Sender<Result<(), String>>,
    },
    Comment {
        plugin: PluginId,
        comment: ReviewComment,
        reply: Sender<Result<(), String>>,
    },
}

/// A note a review plugin adds to the review on screen. The host keeps it with its own notes.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewComment {
    pub path: String,
    #[serde(default)]
    pub side: DiffSide,
    pub start: u32,
    pub end: u32,
    pub text: String,
}

pub struct HostUi {
    windows: BTreeMap<WindowId, HostContext>,
    front: Option<WindowId>,
    waiting: BTreeMap<PluginId, Vec<Sender<HostValue>>>,
    // Kept apart from `HostContext`, which `set_context` rebuilds on every change of the window.
    overlays: BTreeMap<WindowId, Size<Pixels>>,
}

struct HostUiGlobal(Entity<HostUi>);
impl Global for HostUiGlobal {}
impl EventEmitter<HostUiEvent> for HostUi {}

impl HostUi {
    pub(crate) fn init(cx: &mut App) {
        let ui = cx.new(|_| Self {
            windows: BTreeMap::new(),
            front: None,
            waiting: BTreeMap::new(),
            overlays: BTreeMap::new(),
        });
        cx.set_global(HostUiGlobal(ui));
    }

    #[must_use]
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<HostUiGlobal>()
            .map(|global| global.0.clone())
    }

    pub fn set_context(
        &mut self,
        window: AnyWindowHandle,
        thread: Option<ThreadId>,
        project: Option<ProjectId>,
        focused: bool,
        follows: &[PluginId],
        cx: &mut Context<Self>,
    ) {
        let id = window.window_id();
        let mut props = self
            .windows
            .get(&id)
            .filter(|current| current.thread_id == thread)
            .map(|current| current.props.clone())
            .unwrap_or_default();
        if let Some(thread) = thread.as_ref() {
            for plugin in follows {
                let value = props
                    .entry(plugin.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Value::Object(map) = value {
                    map.insert("threadId".into(), Value::String(thread.0.clone()));
                }
            }
        }
        let context = HostContext {
            thread_id: thread,
            project_id: project,
            focused,
            theme: if cx.theme().is_dark() {
                "dark"
            } else {
                "light"
            }
            .into(),
            props,
        };
        if self.windows.get(&id) == Some(&context) {
            return;
        }
        self.windows.insert(id, context);
        if focused || self.front.is_none() {
            self.front = Some(id);
        }
        self.changed();
        cx.notify();
    }

    #[must_use]
    pub fn targets(&self, window: WindowId) -> bool {
        self.front == Some(window)
    }

    /// The thread and project a window's plugin context names, or `None` when the window is not
    /// known to the host.
    #[must_use]
    pub fn window_thread(&self, window: WindowId) -> Option<(Option<ThreadId>, Option<ProjectId>)> {
        self.windows
            .get(&window)
            .map(|context| (context.thread_id.clone(), context.project_id.clone()))
    }

    /// Whether `window` already told overlays this size, in pixels.
    #[must_use]
    pub fn overlay_is(&self, window: WindowId, size: Size<Pixels>) -> bool {
        self.overlays.get(&window) == Some(&size)
    }

    /// The size of the area overlays draw over, read by plugins as `context().overlay`.
    pub fn set_overlay(&mut self, window: WindowId, size: Size<Pixels>) {
        if self.overlay_is(window, size) {
            return;
        }
        self.overlays.insert(window, size);
        self.changed();
    }

    pub fn remove_window(&mut self, id: WindowId, cx: &mut Context<Self>) {
        self.windows.remove(&id);
        self.overlays.remove(&id);
        if self.front == Some(id) {
            self.front = self.windows.keys().next().copied();
        }
        self.changed();
        cx.notify();
    }

    pub(crate) fn unload(&mut self, plugin: &PluginId) {
        self.waiting.remove(plugin);
    }

    /// Delivers props to that plugin in `window` and wakes `onContextChanged`. Other plugins
    /// do not see them, and the plugin reads them while that window is in front.
    pub fn deliver(&mut self, window: WindowId, plugin: &PluginId, props: Value) {
        if let Some(context) = self.windows.get_mut(&window) {
            if props.is_null() {
                context.props.remove(plugin);
            } else {
                context.props.insert(plugin.clone(), props);
            }
        }
        self.changed();
    }

    /// Wakes `onContextChanged` for one plugin, as when the thread list it draws has changed.
    pub fn wake(&mut self, plugin: &PluginId) {
        if let Some(waiters) = self.waiting.remove(plugin) {
            let context = self.context_for(plugin);
            for waiting in waiters {
                let _ = waiting.try_send(context.clone());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn push_waiter(&mut self, plugin: PluginId, sender: Sender<HostValue>) {
        self.waiting.entry(plugin).or_default().push(sender);
    }

    pub(crate) fn context_for(&self, plugin: &PluginId) -> HostValue {
        let Some((id, context)) = self
            .front
            .and_then(|id| self.windows.get(&id).map(|context| (id, context)))
        else {
            return to_host(serde_json::json!({"focused":false,"theme":"light"}));
        };
        let mut value = serde_json::to_value(context).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            if let Some(props) = context.props.get(plugin) {
                object.insert("props".into(), props.clone());
            }
            if let Some(size) = self.overlays.get(&id) {
                object.insert(
                    "overlay".into(),
                    serde_json::json!({
                        "width": f32::from(size.width),
                        "height": f32::from(size.height),
                    }),
                );
            }
        }
        to_host(value)
    }

    fn changed(&mut self) {
        let waiting = std::mem::take(&mut self.waiting);
        for (plugin, waiters) in waiting {
            let context = self.context_for(&plugin);
            for waiting in waiters {
                let _ = waiting.try_send(context.clone());
            }
        }
    }
}

fn app<R>(
    run: impl FnOnce(Entity<HostUi>, &mut App) -> Result<R, HostError>,
) -> Result<R, HostError> {
    with_current_app(|cx| {
        let ui = HostUi::global(cx)
            .ok_or_else(|| HostError::new("the plugin UI host is unavailable"))?;
        run(ui, cx)
    })
    .ok_or_else(|| HostError::new("the application context is unavailable"))?
}

pub(crate) fn extend(module: HostModule, plugin: &PluginId) -> HostModule {
    let declarations = format!(
        "{}\n/** `overlay` is the size in pixels of the front window's center, which a `ui.overlay` view covers. That view may only paint: boxes, text, images, svgs and paths with styles and motion; a handler, hover style, cursor, scroll area, other component or window call fails it. */\nexport function context(): {{ threadId?: string; projectId?: string; focused: boolean; theme: 'light' | 'dark'; props?: unknown; overlay?: {{ width: number; height: number }} }};\nexport function onContextChanged(): Promise<ReturnType<typeof context>>;\nexport function threadOnScreen(threadId: string): boolean;\nexport function openThread(threadId: string): Promise<void>;\n/** Shows a panel this plugin declares, placed left or center. Resolves when it is on screen. */\nexport function openPanel(panelId: string, props?: unknown): Promise<void>;\nexport function openUrl(url: string): Promise<void>;\nexport function toast(message: string, action?: {{ label: string; command: string }}): Promise<void>;\n/** Adds a note to the review on screen, when this plugin draws it. kagan's footer sends or discards it. */\nexport function addComment(comment: {{ path: string; side?: 'old' | 'new'; start: number; end: number; text: string }}): Promise<void>;\n/** Pastes text into a thread's program terminal, without pressing Enter. At most 1 MiB. */\nexport function pasteIntoThread(threadId: string, text: string): Promise<void>;\n/** Writes a PNG to the clipboard. `pngBase64` is standard base64, at most 8 MiB decoded. Needs `powers.clipboard.write`. On Linux it fails: png clipboard is not available on this platform. On Linux it fails: png clipboard is not available on this platform. */\nexport function writeClipboardPng(pngBase64: string): void;",
        module.declared().unwrap_or("")
    );
    let module = context_functions(module.declarations(declarations), plugin);
    let panels = plugin.clone();
    let toasts = plugin.clone();
    let comments = plugin.clone();
    let images = plugin.clone();
    module
        .async_function("openThread", open_thread)
        .async_function("openPanel", move |arguments| open_panel(&panels, arguments))
        .async_function("openUrl", open_url)
        .async_function("toast", move |arguments| toast(&toasts, arguments))
        .async_function("addComment", move |arguments| {
            add_comment(&comments, arguments)
        })
        .async_function("pasteIntoThread", paste_into_thread)
        .function("writeClipboardPng", move |arguments| {
            crate::clipboard::write(&images, arguments)
        })
}

fn add_comment(
    plugin: &PluginId,
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let comment: ReviewComment = serde_json::from_value(from_host(arguments.value(0)?))
        .map_err(|error| HostError::new(error.to_string()))?;
    if comment.text.trim().is_empty() || comment.start == 0 || comment.end < comment.start {
        return Err(HostError::new(
            "a comment needs text and lines from start to end, counted from 1",
        ));
    }
    let (reply, replied) = async_channel::bounded(1);
    let plugin = plugin.clone();
    app(|ui, cx| {
        ui.update(cx, |_, cx| {
            cx.emit(HostUiEvent::Comment {
                plugin,
                comment,
                reply,
            });
        });
        Ok(())
    })?;
    Ok(accepted(replied))
}

fn open_thread(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let (reply, replied) = async_channel::bounded(1);
    app(|ui, cx| queue_thread(&ui, id, reply, cx))?;
    Ok(accepted(replied))
}

fn paste_into_thread(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let text = arguments.string(1)?.to_owned();
    if text.is_empty() {
        return Err(HostError::new("paste needs text"));
    }
    if text.len() > PASTE_CAP {
        return Err(HostError::new(format!(
            "paste is {} bytes; the cap is 1 MiB",
            text.len()
        )));
    }
    let receive = with_current_app(|cx| {
        if Workspace::global(cx).read(cx).thread(&id).is_none() {
            return Err(HostError::new("thread does not exist"));
        }
        let (sender, receive) = async_channel::bounded(1);
        let task = Workspace::dispatch(
            Op::Thread(ThreadOp::Paste {
                id,
                text: Some(text),
                file: None,
                terminal: None,
            }),
            cx,
        );
        cx.spawn(async move |_| {
            let _ = sender.send(task.await).await;
        })
        .detach();
        Ok(receive)
    })
    .ok_or_else(|| HostError::new("the application context is unavailable"))??;
    Ok(async move {
        receive
            .recv()
            .await
            .map_err(|_| HostError::new("the plugin was unloaded"))?
            .map_err(|error| HostError::new(error.to_string()))?;
        Ok(HostValue::Null)
    })
}

fn queue_thread(
    ui: &Entity<HostUi>,
    id: ThreadId,
    reply: Sender<Result<(), String>>,
    cx: &mut App,
) -> Result<(), HostError> {
    if kg_core::Workspace::global(cx)
        .read(cx)
        .thread(&id)
        .is_none()
    {
        return Err(HostError::new("thread does not exist"));
    }
    ui.update(cx, |_, cx| cx.emit(HostUiEvent::OpenThread { id, reply }));
    Ok(())
}

fn open_panel(
    plugin: &PluginId,
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let panel = arguments.string(0)?.to_owned();
    let props = arguments.get(1).map_or(Value::Null, from_host);
    let (reply, replied) = async_channel::bounded(1);
    let plugin = plugin.clone();
    app(|ui, cx| {
        ui.update(cx, |_, cx| {
            cx.emit(HostUiEvent::OpenPanel {
                plugin,
                panel,
                props,
                reply,
            });
        });
        Ok(())
    })?;
    Ok(accepted(replied))
}

fn open_url(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let url = arguments.string(0)?;
    http_url(url)?;
    with_current_app(|cx| cx.open_url(url))
        .ok_or_else(|| HostError::new("the application context is unavailable"))?;
    Ok(async { Ok(HostValue::Null) })
}

fn http_url(url: &str) -> Result<(), HostError> {
    let parsed = url::Url::parse(url).map_err(|error| HostError::new(error.to_string()))?;
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return Err(HostError::new("openUrl needs an http or https URL"));
    }
    Ok(())
}

fn toast(
    plugin: &PluginId,
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let message = arguments.string(0)?.to_owned();
    let action = toast_action(arguments)?;
    let (reply, replied) = async_channel::bounded(1);
    let plugin = plugin.clone();
    app(|ui, cx| {
        let message = match crate::PluginHost::try_global(cx) {
            Some(host) => host.read(cx).secrets.redact(&plugin, &message),
            None => message,
        };
        send_toast(&ui, plugin, message, action, reply, cx)
    })?;
    Ok(accepted(replied))
}

fn toast_action(arguments: &HostArguments) -> Result<Option<ToastAction>, HostError> {
    arguments
        .get(1)
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value(from_host(value)))
        .transpose()
        .map_err(|error| HostError::new(error.to_string()))
}

fn send_toast(
    ui: &Entity<HostUi>,
    plugin: PluginId,
    message: String,
    action: Option<ToastAction>,
    reply: Sender<Result<(), String>>,
    cx: &mut App,
) -> Result<(), HostError> {
    if let Some(action) = &action
        && !declared_command(&plugin, action, cx)
    {
        return Err(HostError::new(
            "toast action needs a declared plugin command",
        ));
    }
    ui.update(cx, |_, cx| {
        cx.emit(HostUiEvent::Toast {
            plugin,
            message,
            action,
            reply,
        });
    });
    Ok(())
}

fn declared_command(plugin: &PluginId, action: &ToastAction, cx: &App) -> bool {
    crate::PluginHost::global(cx)
        .read(cx)
        .commands()
        .iter()
        .any(|command| command.plugin() == plugin && command.id() == action.command())
}

fn context_functions(module: HostModule, plugin: &PluginId) -> HostModule {
    let waiting = plugin.clone();
    module
        .function("context", {
            let plugin = plugin.clone();
            move |_| app(|ui, cx| Ok(ui.read(cx).context_for(&plugin)))
        })
        .function("threadOnScreen", |arguments| {
            let id = arguments.string(0)?;
            app(|ui, cx| {
                Ok(ui
                    .read(cx)
                    .windows
                    .values()
                    .any(|context| {
                        context
                            .thread_id
                            .as_ref()
                            .is_some_and(|thread| thread.0 == id)
                    })
                    .into())
            })
        })
        .async_function("onContextChanged", move |_| {
            let (sender, receiver) = async_channel::bounded(1);
            app(|ui, cx| {
                ui.update(cx, |ui, _| {
                    ui.waiting.entry(waiting.clone()).or_default().push(sender);
                });
                Ok(())
            })?;
            Ok(async move {
                receiver
                    .recv()
                    .await
                    .map_err(|_| HostError::new("the plugin was unloaded"))
            })
        })
}

async fn accepted(
    receiver: async_channel::Receiver<Result<(), String>>,
) -> Result<HostValue, HostError> {
    receiver
        .recv()
        .await
        .map_err(|_| HostError::new("no kagan window is open"))?
        .map_err(HostError::new)?;
    Ok(HostValue::Null)
}

#[cfg(test)]
mod url_tests {
    use super::http_url;

    #[test]
    fn open_url_accepts_only_an_http_address_with_a_host() {
        assert!(http_url("https://example.com").is_ok());
        assert!(http_url("http://example.com/path").is_ok());
        assert!(http_url("notaurl").is_err());
        assert!(http_url("ftp://example.com").is_err());
    }
}
