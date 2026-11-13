use gpui_kit::TestAppContext;

use super::{click_at, observed, seed_thread};

// The status starts with its word. A click turns on `compact`, so the element is built again with the new call.
const SOURCE: &str = r#"
import { View, div } from "gpui-kit";
import { ThreadStatus } from "gpui-component";
export default class App extends View {
  init() { this.compact = false; }
  render() {
    const status = this.compact
      ? new ThreadStatus("thr_swap").compact()
      : new ThreadStatus("thr_swap");
    return div().flex().size_full()
      .on_click((_, cx) => { this.compact = true; cx.notify(); })
      .child(status);
  }
}
"#;

#[gpui_kit::test]
fn compact_drops_the_word_and_keeps_the_mark_and_the_accessible_name(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    seed_thread(directory.path());
    std::fs::write(directory.path().join("main.js"), SOURCE).unwrap();
    let (_dir, mut visual, _view, _runtime) = super::mount_dir(cx, directory);
    let (with_word, label) = visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let found = observed(window, "thread-status").expect("the status is observed");
        (found.bounds.size.width, found.label)
    });
    assert_eq!(label.as_deref(), Some("Thread status: idle"));
    visual.update(|window, cx| {
        click_at(window, cx);
        window.draw(cx).clear(cx);
    });
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let found = observed(window, "thread-status").expect("the status is observed");
        assert!(found.visible);
        assert!(
            found.bounds.size.width < with_word,
            "compact is as wide as the word: {:?}",
            found.bounds
        );
        assert_eq!(found.label, label);
    });
}
