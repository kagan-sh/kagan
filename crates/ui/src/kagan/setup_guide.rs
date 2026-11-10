use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, div,
};
use kg_core::Workspace;
use kg_domain::{Op, PreferenceOp, ProjectKind, SettingsOp};
use kg_plugins::PluginHost;
use serde_json::json;

use super::AddProject;
use super::Kagan;
use crate::kagan::controls::{control, dispatch};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const STEPS: usize = 4;

impl Kagan {
    pub(super) fn set_setup_guide(&mut self, show: bool, cx: &mut Context<Self>) {
        self.prefs.update(cx, |prefs, cx| {
            prefs.setup_guide = show;
            cx.notify();
        });
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: kg_domain::prefs::SETUP_GUIDE.into(),
                value: json!(show),
            }),
            cx,
        )
        .detach();
    }
}

// Each step is done when its thing exists. The agent step is read when the window opens.
fn done(kagan: &Kagan, cx: &App) -> [bool; STEPS] {
    let listing = &kagan.sidebar.read(cx).listing;
    let plugins = PluginHost::try_global(cx).is_some_and(|host| !host.read(cx).status().is_empty());
    [
        kagan.prefs.read(cx).agent_ready == Some(true),
        listing
            .projects
            .iter()
            .any(|project| project.kind == ProjectKind::Standard),
        !listing.threads.is_empty(),
        plugins,
    ]
}

pub(super) fn panel(kagan: &Kagan, cx: &mut Context<Kagan>) -> Option<AnyElement> {
    if !kagan.prefs.read(cx).setup_guide {
        return None;
    }
    let done = done(kagan, cx);
    let finished = done.iter().filter(|done| **done).count();
    let rows = [
        (
            "Pick an agent",
            "Install Claude Code or Codex, then choose it when you start a thread.",
            "Choose agent",
        ),
        (
            "Add a project",
            "Point kagan at a folder that holds your code.",
            "Add project…",
        ),
        (
            "Start a thread",
            "A thread runs one agent in one folder.",
            "New thread",
        ),
        (
            "Add plugins (optional)",
            "Plugins add panels and commands. Suggestions are in Installed.",
            "Browse plugins",
        ),
    ];
    let mut body = v_flex().gap_2();
    for (index, ((title, note, action), done)) in rows.into_iter().zip(done).enumerate() {
        body = body.child(step(index, title, note, action, done, cx));
    }
    Some(
        v_flex()
            .id("setup-guide")
            .ui_container()
            .w_full()
            .max_w(gpui_kit::px(460.))
            .p_4()
            .gap_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child("Set up kagan"),
                            )
                            .child(
                                div()
                                    .type_small(cx)
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{finished} of {STEPS} done")),
                            ),
                    )
                    .child(control(
                        "setup-guide-dismiss",
                        Facts::button("Hide"),
                        Button::new("setup-guide-dismiss-button")
                            .small()
                            .ghost()
                            .label("Hide")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_setup_guide(false, cx)),
                            ),
                    )),
            )
            .child(body)
            .into_any_element(),
    )
}

fn step(
    index: usize,
    title: &str,
    note: &str,
    action: &str,
    done: bool,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let state = if done {
        Icon::new(IconName::Check)
            .small()
            .text_color(cx.theme().success)
            .into_any_element()
    } else {
        div()
            .w(gpui_kit::px(14.))
            .type_small(cx)
            .text_color(muted)
            .child(format!("{}", index + 1))
            .into_any_element()
    };
    let trailing = if done {
        div()
            .type_small(cx)
            .text_color(muted)
            .child("Done")
            .into_any_element()
    } else {
        let id = format!("setup-guide-step-{}", index + 1);
        let button = Button::new(format!("{id}-button"))
            .small()
            .outline()
            .label(action);
        let button = match index {
            0 | 2 => button
                .on_click(cx.listener(|this, _, window, cx| this.new_personal_thread(window, cx))),
            1 => button.on_click(dispatch(AddProject)),
            _ => button.on_click(cx.listener(|_, _, _, cx| {
                Workspace::dispatch(Op::Settings(SettingsOp::Plugins), cx).detach();
            })),
        };
        control(id.clone(), Facts::button(action), button).into_any_element()
    };
    h_flex()
        .gap_3()
        .items_center()
        .child(
            div()
                .w(gpui_kit::px(20.))
                .flex()
                .justify_center()
                .child(state),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().child(title.to_owned()))
                .child(
                    div()
                        .type_small(cx)
                        .text_color(muted)
                        .child(note.to_owned()),
                ),
        )
        .child(trailing)
        .into_any_element()
}
