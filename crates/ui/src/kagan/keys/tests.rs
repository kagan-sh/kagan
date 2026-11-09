use super::chord_bindings;
use gpui_kit::TestAppContext;
use kg_domain::{Keymap, SHORTCUTS};

#[gpui_kit::test]
fn every_shortcut_builds_an_action(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(
            chord_bindings(&Keymap::default(), cx).len(),
            SHORTCUTS.len() + 1
        );
        for shortcut in SHORTCUTS {
            assert!(
                cx.build_action(shortcut.action, None).is_ok(),
                "{}",
                shortcut.action
            );
        }
    });
}
