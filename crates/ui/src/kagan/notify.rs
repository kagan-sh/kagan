use gpui_kit::{App, SystemNotification};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, PreferenceOp, ThreadId};

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
    cx.subscribe(&workspace, |workspace, event, cx| {
        let kg_core::WorkspaceEvent::Waiting(id) = event else {
            return;
        };
        if super::shows(id, cx) {
            return;
        }
        let title = workspace
            .read(cx)
            .thread(id)
            .map(|thread| thread.read(cx).record().title.clone())
            .unwrap_or_default();
        let id = id.clone();
        cx.spawn(async move |cx| post(id, title, cx).await).detach();
    })
    .detach();
}

async fn post(id: ThreadId, title: String, cx: &mut gpui_kit::AsyncApp) {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: kg_domain::prefs::NOTIFY.into(),
            }),
            cx,
        )
    });
    let on = match task.await {
        Ok(OpResult::Setting(value)) => value.as_bool().unwrap_or(true),
        _ => return,
    };
    if !on {
        return;
    }
    cx.update(|cx| {
        if super::shows(&id, cx) {
            return;
        }
        cx.show_system_notification(SystemNotification {
            tag: format!("kagan:thread:{id}").into(),
            title: title.into(),
            body: String::new().into(),
            actions: Vec::new(),
        });
    });
}
