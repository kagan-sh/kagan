use super::{Kagan, Toast};

use gpui_kit::{Context, Window};
use kg_core::Workspace;
use kg_domain::{Op, OpResult};

// Either separator counts, so a Windows path splits the same way as a slash path.
pub(super) fn directory_and_name(path: &str) -> (&str, &str) {
    path.rsplit_once(['/', '\\'])
        .map_or(("", path), |(directory, name)| (directory, name))
}

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

#[cfg(test)]
mod tests {
    use super::directory_and_name;

    #[test]
    fn a_path_splits_on_slash_or_backslash() {
        assert_eq!(directory_and_name("src/main.rs"), ("src", "main.rs"));
        assert_eq!(directory_and_name("a/b/c"), ("a/b", "c"));
        assert_eq!(directory_and_name(r"src\main.rs"), ("src", "main.rs"));
        assert_eq!(directory_and_name("main.rs"), ("", "main.rs"));
    }
}
