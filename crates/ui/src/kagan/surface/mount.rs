use crate::theme::{SpaceStep, TypeScale as _, space};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use kg_domain::{PluginId, SurfacePlacement};
use kg_plugins::{MountError, PluginHost};
use serde_json::Value;

use super::right::plugin_body;
use super::{ClosePage, Declared, Expect, Focus, Slot};
use crate::kagan::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::{Facts, Located, locate};

pub(in crate::kagan) fn mount(
    declared: &Declared,
    details: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Slot {
    let host = PluginHost::global(cx);
    let web = host.read(cx).web_file(&declared.plugin, &declared.id);
    if web.is_some() {
        let loaded = host.update(cx, |host, cx| host.load_plugin(&declared.plugin, cx));
        let (web, error) = match loaded {
            Ok(()) => (web, None),
            Err(error) => (None, Some(mount_error(&error))),
        };
        return Slot {
            id: declared.surface_id(),
            plugin: declared.plugin.clone(),
            contribution: declared.id.clone(),
            title: declared.title.clone(),
            details,
            view: None,
            _snapshot_watch: None,
            web,
            error,
            generation: host.read(cx).generation(&declared.plugin).unwrap_or(0),
            focus: cx.focus_handle(),
        };
    }
    let mounted = host.update(cx, |host, cx| {
        host.mount_panel(&declared.plugin, &declared.id, window, cx)
    });
    let (view, error) = match mounted {
        Ok(view) => (Some(view), None),
        Err(error) => (None, Some(mount_error(&error))),
    };
    let snapshot_watch = view.as_ref().map(|view| {
        std::rc::Rc::new(
            cx.subscribe(view, |_, _, _: &gpui_shell::SnapshotPublished, cx| {
                cx.notify();
            }),
        )
    });
    Slot {
        id: declared.surface_id(),
        plugin: declared.plugin.clone(),
        contribution: declared.id.clone(),
        title: declared.title.clone(),
        details,
        view,
        _snapshot_watch: snapshot_watch,
        web: None,
        error,
        generation: host.read(cx).generation(&declared.plugin).unwrap_or(0),
        focus: cx.focus_handle(),
    }
}

pub(super) fn matching_slot<'a>(
    tool: Option<&'a Slot>,
    page: Option<&'a Slot>,
    right: &'a [Slot],
    declared: &Declared,
) -> Option<&'a Slot> {
    let same = |slot: &&Slot| slot.contribution == declared.id && slot.plugin == declared.plugin;
    match declared.placement {
        SurfacePlacement::Left => tool.filter(same),
        SurfacePlacement::Center => page.filter(same),
        SurfacePlacement::Right => right.iter().find(same),
    }
}

pub(super) fn focus_for(placement: SurfacePlacement, open: bool) -> Focus {
    match (placement, open) {
        (SurfacePlacement::Center, true) => Focus::Page,
        (SurfacePlacement::Center, false) => Focus::Work,
        (SurfacePlacement::Left, _) => Focus::Tool,
        (SurfacePlacement::Right, _) => Focus::Right,
    }
}

pub(super) fn body_id(id: &str) -> String {
    format!("{id}-body")
}

pub(super) fn expect_of(
    declared: &Declared,
    open: bool,
    plugin: Option<PluginId>,
    props: Option<Value>,
) -> Expect {
    Expect {
        id: body_id(&declared.surface_id()),
        present: open,
        plugin: if open { plugin } else { None },
        props: if open { props } else { None },
        focus: focus_for(declared.placement, open),
    }
}

pub(super) fn holds_focus(slot: Option<&Slot>, window: &Window, cx: &App) -> bool {
    slot.is_some_and(|slot| slot.focus.contains_focused(window, cx))
}

pub(in crate::kagan) fn mount_error(error: &MountError) -> String {
    match error {
        MountError::Missing(text) | MountError::Failed(text) => text.clone(),
    }
}

pub(in crate::kagan) fn painted(window: &Window, id: &str) -> bool {
    match locate(window, &ElementId::Name(id.into()), None) {
        Located::One(element) => {
            element.visible
                && f32::from(element.bounds.size.width) > 0.
                && f32::from(element.bounds.size.height) > 0.
        }
        Located::Missing | Located::Ambiguous(_) => false,
    }
}

pub(in crate::kagan) fn tool_region(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Option<AnyElement> {
    kagan.tool.as_ref().map(|slot| {
        div()
            .id("surface-tool")
            .child(
                div()
                    .id(ElementId::Name(body_id(&slot.id).into()))
                    .ui_container()
                    .track_focus(&slot.focus)
                    .child(plugin_body(slot, false, window, cx)),
            )
            .into_any_element()
    })
}

pub(super) fn center_body(slot: &Slot, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let id = ElementId::Name(format!("surface-scroll-{}", slot.id).into());
    let body = plugin_body(slot, true, window, cx);
    if slot.web.is_none() {
        div()
            .size_full()
            .p(px(space(SpaceStep::Lg, cx)))
            .ui_container()
            .overflow_y_scrollbar()
            .id(id)
            .child(body)
            .into_any_element()
    } else {
        div()
            .size_full()
            .overflow_hidden()
            .id(id)
            .child(body)
            .into_any_element()
    }
}

pub(in crate::kagan) fn page(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let Some(slot) = kagan.screen.page() else {
        return div().into_any_element();
    };
    let focus = kagan.surface_close_focus.clone();
    let title = slot.title.clone();
    v_flex()
        .id("surface-page")
        .ui_container()
        .size_full()
        .min_h_0()
        .key_context("Page")
        .child(
            h_flex()
                .px(px(space(SpaceStep::Lg, cx)))
                .py(px(space(SpaceStep::Sm, cx)))
                .gap(px(space(SpaceStep::Sm, cx)))
                .justify_between()
                .items_center()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("surface-title")
                        .ui_container()
                        .flex_1()
                        .min_w_0()
                        .type_title(cx)
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_ellipsis()
                        .child(title.clone()),
                )
                .child(
                    div()
                        .id("surface-close")
                        .key_context("SurfaceClose")
                        .tab_stop(true)
                        .px(px(space(SpaceStep::Sm, cx)))
                        .py(px(space(SpaceStep::Xs, cx)))
                        .rounded(cx.theme().radius)
                        .type_body(cx)
                        .focus(|close| close.border_1().border_color(cx.theme().ring))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_page(&ClosePage, window, cx);
                        }))
                        .child("Close")
                        .ui_observe_as(Facts {
                            focus: Some(focus.clone()),
                            ..Facts::button("Close")
                        }),
                ),
        )
        .child(
            div().flex_1().min_h_0().child(
                div()
                    .id(ElementId::Name(body_id(&slot.id).into()))
                    .size_full()
                    .min_h_0()
                    .ui_container()
                    .track_focus(&slot.focus)
                    .child(center_body(slot, window, cx)),
            ),
        )
        .into_any_element()
}
