use super::{Kagan, Toast};

use gpui_kit::{Context, Window};
use kg_core::Workspace;
use kg_domain::{Op, OpResult};

impl Kagan {
    pub(super) fn open_answer(op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(op, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| match result {
                Ok(OpResult::Files(paths)) => {
                    for url in paths
                        .iter()
                        .filter_map(|path| url::Url::from_file_path(path).ok())
                    {
                        cx.open_url(url.as_str());
                    }
                }
                Ok(_) => {}
                Err(error) => this.show_toast(Toast::said(error.to_string()), cx),
            });
        })
        .detach();
    }
}
