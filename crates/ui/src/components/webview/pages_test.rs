use gpui_kit::{TestAppContext, WeakEntity};
use gpui_shell::ScriptView;

use super::super::{Held, Pages};
use super::{held, sync_pages};

#[gpui_kit::test]
fn a_page_whose_view_is_gone_is_dropped_on_sync(cx: &mut TestAppContext) {
    let owner: WeakEntity<ScriptView> = WeakEntity::new_invalid();
    let entry: Held = held(
        owner,
        "gone",
        "https://example.com",
        &[],
        None,
        None,
        Some("held".into()),
    );
    cx.update(|cx| {
        cx.set_global(Pages { pages: vec![entry] });
        sync_pages(&[], cx);
        assert!(cx.global::<Pages>().pages.is_empty());
    });
}
