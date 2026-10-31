use gpui_kit::{
    App, ElementId, InputEvent, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent, Window,
    point,
};
use kg_domain::OpError;

pub(crate) fn render_frame(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

#[cfg(test)]
pub(crate) fn click(window: &mut Window, id: &ElementId, cx: &mut App) -> Result<(), OpError> {
    click_within(window, id, None, cx)
}

pub(crate) fn click_within(
    window: &mut Window,
    id: &ElementId,
    within: Option<&ElementId>,
    cx: &mut App,
) -> Result<(), OpError> {
    bring_into_view(window, id, within, cx);
    if let crate::ui_registry::Located::One(target) = crate::ui_registry::locate(window, id, within)
        && !target.visible()
    {
        move_pointer(window, target.bounds().center(), None, cx);
    }
    let position = locate(window, id, within)?;
    click_at(window, position, cx);
    Ok(())
}

fn bring_into_view(window: &mut Window, id: &ElementId, within: Option<&ElementId>, cx: &mut App) {
    render_frame(window, cx);
    let mut previous = None;
    while let crate::ui_registry::Located::One(target) =
        crate::ui_registry::locate(window, id, within)
    {
        if target.visible() || previous == Some(target.bounds()) {
            break;
        }
        previous = Some(target.bounds());
        let ancestor = snapshots(window)
            .into_iter()
            .filter(|element| {
                element.visible()
                    && target.path().starts_with(element.path())
                    && element.path().len() < target.path().len()
            })
            .max_by_key(|element| element.path().len());
        let Some(ancestor) = ancestor else { break };
        let position = ancestor.clipped().center();
        let target_position = target.bounds().center();
        move_pointer(window, position, None, cx);
        window.dispatch_event(
            ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(
                    position.x - target_position.x,
                    position.y - target_position.y,
                )),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        render_frame(window, cx);
    }
}

pub(crate) fn scroll_within(
    window: &mut Window,
    id: &ElementId,
    within: Option<&ElementId>,
    delta: ScrollDelta,
    cx: &mut App,
) -> Result<(), OpError> {
    bring_into_view(window, id, within, cx);
    let position = locate(window, id, within)?;
    move_pointer(window, position, None, cx);
    window.dispatch_event(
        ScrollEvent {
            position,
            delta,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    render_frame(window, cx);
    Ok(())
}

pub(crate) fn press(window: &mut Window, key: &str, cx: &mut App) -> Result<(), OpError> {
    let key = kg_core::parse_key(key).map_err(|error| OpError::Invalid(error.to_string()))?;
    render_frame(window, cx);
    press_key(window, key, cx);
    render_frame(window, cx);
    Ok(())
}

pub(crate) fn input(window: &mut Window, text: &str, cx: &mut App) -> Result<(), OpError> {
    render_frame(window, cx);
    for character in text.chars() {
        let text = character.to_string();
        let mut key =
            kg_core::parse_key(&text).map_err(|error| OpError::Invalid(error.to_string()))?;
        key.key_char = Some(text);
        window.dispatch_keystroke(key, cx);
        render_frame(window, cx);
    }
    Ok(())
}

pub(crate) fn snapshot(window: &Window, index: u32) -> kg_domain::UiSnapshot {
    kg_domain::UiSnapshot {
        window: index,
        elements: observed_elements(window),
    }
}

fn observed_elements(window: &Window) -> Vec<kg_domain::UiElement> {
    snapshots(window)
        .iter()
        .map(|element| {
            let path = element.path().iter().map(|id| format!("{id:?}")).collect();
            let id = element
                .path()
                .last()
                .map(|id| format!("{id:?}"))
                .unwrap_or_default();
            let bounds = element.bounds();
            kg_domain::UiElement {
                id,
                path,
                role: element.role().map(|role| format!("{role:?}")),
                label: element.label().map(str::to_owned),
                value: element.value().map(str::to_owned),
                bounds: kg_domain::UiBounds {
                    x: bounds.origin.x.as_f32(),
                    y: bounds.origin.y.as_f32(),
                    width: bounds.size.width.as_f32(),
                    height: bounds.size.height.as_f32(),
                },
                focused: element.focused(),
                disabled: element.disabled(),
                selected: element.selected(),
                visible: element.visible(),
            }
        })
        .collect()
}

fn snapshots(window: &gpui_kit::Window) -> Vec<crate::ui_registry::ElementSnapshot> {
    crate::ui_registry::snapshots(window)
}

fn locate(
    window: &Window,
    id: &ElementId,
    within: Option<&ElementId>,
) -> Result<Point<Pixels>, OpError> {
    let target = match crate::ui_registry::locate(window, id, within) {
        crate::ui_registry::Located::Missing => {
            return Err(OpError::NotFound(format!(
                "element {id:?} is not in the latest frame"
            )));
        }
        crate::ui_registry::Located::Ambiguous(paths) => {
            return Err(OpError::Invalid(format!(
                "{} elements match {id:?}: {}; pick one with --within <id>",
                paths.len(),
                paths.join("; ")
            )));
        }
        crate::ui_registry::Located::One(target) => target,
    };
    if !target.visible() {
        return Err(OpError::NotFound(format!("element {id:?} is not visible")));
    }
    Ok(target.clipped().center())
}

pub(crate) fn click_at(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    move_pointer(window, position, None, cx);
    mouse_down(window, position, MouseButton::Left, 1, cx);
    mouse_up(window, position, MouseButton::Left, 1, cx);
}

fn move_pointer(
    window: &mut Window,
    position: Point<Pixels>,
    pressed_button: Option<MouseButton>,
    cx: &mut App,
) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    render_frame(window, cx);
}

fn mouse_down(
    window: &mut Window,
    position: Point<Pixels>,
    button: MouseButton,
    click_count: usize,
    cx: &mut App,
) {
    window.dispatch_event(
        MouseDownEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    render_frame(window, cx);
}

fn mouse_up(
    window: &mut Window,
    position: Point<Pixels>,
    button: MouseButton,
    click_count: usize,
    cx: &mut App,
) {
    window.dispatch_event(
        MouseUpEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count,
        }
        .to_platform_input(),
        cx,
    );
    render_frame(window, cx);
}

fn press_key(window: &mut Window, key: Keystroke, cx: &mut App) {
    let key = if matches!(key.key.as_str(), "enter" | "tab") {
        window.dispatch_event(
            KeyDownEvent {
                keystroke: key.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        key
    } else {
        let key = key.with_simulated_ime();
        window.dispatch_keystroke(key.clone(), cx);
        key
    };
    window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
}
