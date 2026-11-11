use gpui_kit::{Bounds, ElementId, point, px, size};

use super::observed;
use crate::ui_registry::ElementSnapshot;

fn element(id: &'static str, x: f32, y: f32) -> ElementSnapshot {
    let bounds = Bounds::new(point(px(x), px(y)), size(px(10.), px(10.)));
    ElementSnapshot {
        role: None,
        path: vec![ElementId::from(id)],
        value: None,
        bounds,
        clipped: bounds,
        visible: true,
        focused: None,
        disabled: None,
        selected: None,
        label: None,
        hint: None,
    }
}

#[test]
fn a_snapshot_lists_elements_top_to_bottom_then_left_to_right_whatever_order_they_came_in() {
    let listed = observed(&[
        element("right", 50., 0.),
        element("lower", 0., 40.),
        element("left", 0., 0.),
    ]);
    let at: Vec<_> = listed
        .iter()
        .map(|element| (element.bounds.y, element.bounds.x))
        .collect();
    assert_eq!(at, [(0., 0.), (0., 50.), (40., 0.)]);
}
