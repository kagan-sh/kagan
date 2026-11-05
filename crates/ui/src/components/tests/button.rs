use gpui_kit::{ElementId, Role, TestAppContext};

use super::{mount_live, observed};

const SOURCE: &str = r#"
import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class App extends View {
  init(_props, _cx) { this.hits = 0; }
  render() {
    return div().size_full()
      .child(new KgButton("press").label("Press me").absolute().left(0).top(0).w(200).h(32)
        .on_click((_event, cx) => { this.hits++; cx.notify(); }))
      .child(new KgButton("held").label("Held").disabled(true).absolute().left(0).top(40).w(200).h(32)
        .on_click((_event, cx) => { this.hits += 10; cx.notify(); }))
      .child(`hits: ${this.hits}`);
  }
}
"#;

#[gpui_kit::test]
fn a_kg_button_is_found_named_and_clicked_through_kg_ui(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(cx, SOURCE);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let id = ElementId::Name("press".into());
        let found = crate::ui_registry::find(window, &id).expect("the button is observed");
        assert_eq!(found.role, Some(Role::Button));
        assert_eq!(found.label.as_deref(), Some("Press me"));
        assert_eq!(found.disabled, Some(false));
        assert!(found.visible);

        crate::ui_dispatch::click(window, &id, cx).expect("the button is clicked");
        window.draw(cx).clear(cx);
    });
    visual.update(|_, cx| {
        let tree = view.read(cx).snapshot().unwrap().debug_tree();
        assert!(tree.contains("hits: 1"), "{tree}");
    });
}

#[gpui_kit::test]
fn a_disabled_kg_button_reports_disabled_and_ignores_clicks(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(cx, SOURCE);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let id = ElementId::Name("held".into());
        let found = crate::ui_registry::find(window, &id).expect("the button is observed");
        assert_eq!(found.disabled, Some(true));

        let _ = crate::ui_dispatch::click(window, &id, cx);
        window.draw(cx).clear(cx);
    });
    visual.update(|_, cx| {
        let tree = view.read(cx).snapshot().unwrap().debug_tree();
        assert!(tree.contains("hits: 0"), "{tree}");
    });
}

const CONTRACT: &str = r#"
import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class App extends View {
  init(_props, _cx) { this.hits = 0; this.off = false; this.pressed = false; }
  render() {
    return div().size_full()
      .child(new KgButton("every").label("Every").tooltip("Hint").loading(false).size("small")
        .primary().outline().compact().disabled(false).selected(false)
        .on_click((_event, cx) => { this.hits++; cx.notify(); }))
      .child(new KgButton("loading").label("Loading").loading(true).absolute().left(0).top(40).w(200).h(32)
        .on_click((_event, cx) => { this.hits += 10; cx.notify(); }))
      .child(new KgButton("kids").absolute().left(0).top(80).w(200).h(32)
        .on_click((_event, cx) => { this.hits += 1000; cx.notify(); })
        .child(div().size_full().on_click((_event, cx) => { this.hits += 100; cx.notify(); })))
      .child(new KgButton("target").label("Target").disabled(this.off).absolute().left(0).top(120).w(200).h(32)
        .selected(this.pressed)
        .on_click((_event, cx) => { this.hits += 5; cx.notify(); }))
      .child(new KgButton("toggle").label("Toggle").absolute().left(0).top(160).w(200).h(32)
        .on_click((_event, cx) => { this.off = !this.off; this.pressed = !this.pressed; cx.notify(); }))
      .child(`hits: ${this.hits}`);
  }
}
"#;

#[gpui_kit::test]
fn every_button_call_of_the_pinned_contract_is_accepted(cx: &mut TestAppContext) {
    // `mount_live` fails the test when the script does not build, so every call above is declared.
    let (_dir, mut visual, _view, _runtime) = mount_live(cx, CONTRACT);
    visual.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn a_loading_kg_button_ignores_clicks(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(cx, CONTRACT);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        let _ = crate::ui_dispatch::click(window, &ElementId::Name("loading".into()), cx);
        window.draw(cx).clear(cx);
    });
    visual.update(|_, cx| {
        let tree = view.read(cx).snapshot().unwrap().debug_tree();
        assert!(tree.contains("hits: 0"), "{tree}");
    });
}

#[gpui_kit::test]
fn a_child_inside_a_kg_button_takes_the_click(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(cx, CONTRACT);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        crate::ui_dispatch::click(window, &ElementId::Name("kids".into()), cx).unwrap();
        window.draw(cx).clear(cx);
    });
    visual.update(|_, cx| {
        let tree = view.read(cx).snapshot().unwrap().debug_tree();
        assert!(
            tree.contains("hits: 100") || tree.contains("hits: 1100"),
            "{tree}"
        );
    });
}

#[gpui_kit::test]
fn disabled_and_selected_follow_the_script_across_a_change(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(cx, CONTRACT);
    let state = |visual: &mut gpui_kit::VisualTestContext| {
        visual.update(|window, _| {
            let found = crate::ui_registry::find(window, &ElementId::Name("target".into()))
                .expect("the target is observed");
            (found.disabled, found.selected)
        })
    };
    assert_eq!(state(&mut visual), (Some(false), Some(false)));
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        crate::ui_dispatch::click(window, &ElementId::Name("toggle".into()), cx).unwrap();
        window.draw(cx).clear(cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(state(&mut visual), (Some(true), Some(true)));
    let _ = view;
}

#[gpui_kit::test]
fn focus_moves_onto_a_kg_button_and_reports_it(cx: &mut TestAppContext) {
    let (_dir, mut visual, _view, _runtime) = mount_live(cx, CONTRACT);
    let focused = |visual: &mut gpui_kit::VisualTestContext| {
        visual.update(|window, _| {
            crate::ui_registry::find(window, &ElementId::Name("every".into()))
                .and_then(|found| found.focused)
        })
    };
    visual.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
        window.focus_next(cx);
        window.draw(cx).clear(cx);
    });
    assert_eq!(focused(&mut visual), Some(true));
}

const COMPACT: &str = r#"
import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class App extends View {
  render() {
    return div().size_full()
      .child(new KgButton("compact").label("Kg").primary().compact().absolute().left(0).top(0))
      .child(new KgButton("reversed").label("Kg").compact().primary().absolute().left(0).top(40))
      .child(new KgButton("plain").label("Kg").primary().absolute().left(0).top(80));
  }
}
"#;

const OUTLINE: &str = r#"
import { View, div } from "gpui-kit";
import { Button, KgButton } from "gpui-component";
export default class App extends View {
  render() {
    return div().size_full().flex_col()
      .child(new Button("pinned").label("Pinned").primary().outline())
      .child(new KgButton("kg").label("Kg").primary().outline())
      .child(new KgButton("reversed").label("Kg").outline().primary());
  }
}
"#;

/// The position and size of each named `KgButton`, as drawn: x, y, width, height.
fn placed(cx: &mut TestAppContext, source: &str, names: &[&str]) -> Vec<[f32; 4]> {
    let (_dir, mut visual, _view, _runtime) = mount_live(cx, source);
    visual.update(|window, _| {
        names
            .iter()
            .map(|name| {
                let bounds = observed(window, name)
                    .expect("a KgButton is observed")
                    .bounds;
                [
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                ]
            })
            .collect()
    })
}

fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[gpui_kit::test]
fn a_compact_kg_button_is_as_wide_in_either_call_order_and_narrower_than_plain(
    cx: &mut TestAppContext,
) {
    // Compact only shortens the horizontal padding. Absolute boxes size to their content, so width is the measure.
    let [compact, reversed, plain] = placed(cx, COMPACT, &["compact", "reversed", "plain"])
        .try_into()
        .unwrap();
    let compact_w = compact[2];
    assert!(
        same(reversed[2], compact_w),
        "compact before primary is {}pt, not {compact_w}pt",
        reversed[2]
    );
    assert!(
        plain[2] > compact_w + 1.0,
        "compact {compact_w}pt, plain {}pt",
        plain[2]
    );
}

#[gpui_kit::test]
fn an_outline_kg_button_is_as_tall_as_the_pinned_one_in_either_call_order(cx: &mut TestAppContext) {
    // The pinned Button is stacked first, so the KgButton below it starts at the pinned height.
    let [kg, reversed] = placed(cx, OUTLINE, &["kg", "reversed"]).try_into().unwrap();
    let [_, kg_y, _, kg_h] = kg;
    assert!(
        same(kg_h, kg_y),
        "outline KgButton is {kg_h}pt, pinned is {kg_y}pt"
    );
    assert!(
        same(reversed[1], kg_y + kg_h),
        "the column stacks without gaps"
    );
    assert!(
        same(reversed[3], kg_h),
        "outline before primary is {}pt, not {kg_h}pt",
        reversed[3]
    );
}
