use gpui_kit::{ElementId, Role, TestAppContext};

use super::mount_live;

const SOURCE: &str = r#"
import { View, div } from "gpui-kit";
import { InputState, KgInput } from "gpui-component";
export default class App extends View {
  init() { this.field = InputState(); }
  render() {
    return div().size_full().child(
      new KgInput(this.field).aria_label("Title").absolute().left(8).top(8).w(240).h(36)
    );
  }
}
"#;

#[gpui_kit::test]
fn a_plugin_field_is_clicked_and_typed_through_the_driver(cx: &mut TestAppContext) {
    let (_dir, mut visual, _view, _runtime) = mount_live(cx, SOURCE);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let id = ElementId::Name("Title".into());
        let found = crate::ui_registry::find(window, &id).expect("the field is observed");
        assert_eq!(found.role(), Some(Role::TextInput));
        assert_eq!(found.label(), Some("Title"));
        assert_eq!(found.value(), Some(""));
        assert_eq!(found.focused(), Some(false));

        crate::ui_dispatch::click(window, &id, cx).expect("the field is clicked");
        crate::ui_dispatch::input(window, "Hi", cx).expect("the field takes text");
        window.draw(cx).clear(cx);

        let snapshot = crate::ui_dispatch::snapshot(window, 0);
        let field = snapshot
            .elements
            .iter()
            .find(|element| element.label.as_deref() == Some("Title"))
            .expect("the snapshot names the field");
        assert_eq!(field.value.as_deref(), Some("Hi"));
        assert_eq!(field.focused, Some(true));
    });
}
