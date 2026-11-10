use gpui_kit::{App, SystemNotification};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{NotifyLevel, NotifyReason, Op, OpResult, PreferenceOp, ThreadId, ThreadNotify};

pub(super) fn init(cx: &mut App) {
    cx.set_app_identity("kagan.kg", "Kagan");
    cx.on_system_notification_response(|response, cx| {
        let Some(thread) = response.tag.strip_prefix("kagan:thread:") else {
            return;
        };
        let id = ThreadId(thread.into());
        if Workspace::global(cx).read(cx).thread(&id).is_none() {
            return;
        }
        let opened = Workspace::dispatch(Op::Open, cx);
        cx.spawn(async move |cx| {
            if opened.await.is_ok() {
                cx.update(|cx| super::window::open_thread(&id, cx));
            }
        })
        .detach();
    });
}

pub(super) fn watch(cx: &mut App) {
    let workspace = Workspace::global(cx);
    cx.subscribe(&workspace, |workspace, event, cx| match event {
        WorkspaceEvent::Notice(id, reason) => {
            if super::shows(id, cx) {
                return;
            }
            let thread = workspace.read(cx).thread(id).map(|thread| {
                let thread = thread.read(cx).record();
                (thread.title.clone(), thread.notify)
            });
            let Some((title, level)) = thread else {
                return;
            };
            let (id, reason) = (id.clone(), *reason);
            cx.spawn(async move |cx| post(id, title, level, reason, cx).await)
                .detach();
        }
        WorkspaceEvent::NotifyTest => {
            cx.show_system_notification(SystemNotification {
                tag: "kagan:test".into(),
                title: "Kagan test".into(),
                body: "Notifications from kagan reach you here.".into(),
                actions: Vec::new(),
            });
        }
        _ => {}
    })
    .detach();
}

// Opening a thread clears its notice.
pub(super) fn clear(id: &ThreadId, cx: &App) {
    cx.dismiss_system_notification(&tag(id));
}

fn tag(id: &ThreadId) -> String {
    format!("kagan:thread:{id}")
}

async fn post(
    id: ThreadId,
    title: String,
    level: ThreadNotify,
    reason: NotifyReason,
    cx: &mut gpui_kit::AsyncApp,
) {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: kg_domain::prefs::NOTIFY.into(),
            }),
            cx,
        )
    });
    let global = match task.await {
        Ok(OpResult::Setting(value)) => serde_json::from_value(value).unwrap_or_default(),
        _ => NotifyLevel::default(),
    };
    if !level.posts(global, reason) {
        return;
    }
    cx.update(|cx| {
        if super::shows(&id, cx) {
            return;
        }
        cx.show_system_notification(SystemNotification {
            tag: tag(&id).into(),
            title: title.into(),
            body: String::new().into(),
            actions: Vec::new(),
        });
    });
}
