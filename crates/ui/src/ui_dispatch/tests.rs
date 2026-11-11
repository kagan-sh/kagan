use std::cmp::Ordering;

use kg_domain::{UiBounds, UiElement};

use super::reading_order;

fn element(id: &str, x: f32, y: f32) -> UiElement {
    UiElement {
        id: id.into(),
        path: vec![id.into()],
        role: None,
        label: None,
        value: None,
        bounds: UiBounds {
            x,
            y,
            width: 10.,
            height: 10.,
        },
        focused: None,
        disabled: None,
        selected: None,
        visible: true,
        hint: None,
    }
}

#[test]
fn a_snapshot_lists_elements_top_to_bottom_then_left_to_right() {
    let mut elements = [
        element("right", 50., 0.),
        element("lower", 0., 40.),
        element("left", 0., 0.),
    ];
    elements.sort_by(reading_order);
    let ids: Vec<_> = elements.iter().map(|element| element.id.as_str()).collect();
    assert_eq!(ids, ["left", "right", "lower"]);
    assert_eq!(reading_order(&elements[0], &elements[0]), Ordering::Equal);
}
