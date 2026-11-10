use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, div,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, PreferenceOp, ProgramOp, ProjectId, ProjectKind, SettingsOp};
use kg_plugins::PluginHost;
use serde_json::json;

use super::AddProject;
use super::Kagan;
use crate::kagan::controls::{control, dispatch};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const STEPS: usize = 4;
const INSTALL_HINT: &str = "https://github.com/kagan-sh/kagan/blob/main/book/src/start.md";

// The guide shows only once the saved choice has loaded, and only when the choice is to show it.
pub(super) fn guide_visible(loaded: bool, pref: Option<bool>) -> bool {
    loaded && pref == Some(true)
}

// The guide leaves the empty window once every step is done. The optional plugins step is
// one of the four, so until it is done the guide stays open.
pub(super) fn hides_itself(pref: Option<bool>, replaying: bool, done: [bool; STEPS]) -> bool {
    pref == Some(true) && !replaying && done.iter().all(|done| *done)
}

impl Kagan {
    // Shows the guide now, or hides it. Replay asks for the guide, so auto-hide waits for it.
    pub(super) fn set_setup_guide(&mut self, show: bool, cx: &mut Context<Self>) {
        self.setup_replay = show;
        self.write_setup_guide(show, cx);
    }

    fn write_setup_guide(&mut self, show: bool, cx: &mut Context<Self>) {
        self.prefs.update(cx, |prefs, cx| {
            prefs.setup_guide = Some(show);
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

    // Runs once the preferences have loaded. A window that already holds a project or thread
    // is an upgrade, so it does not get the guide; a fresh window does.
    pub(super) fn settle_setup_guide(&mut self, cx: &mut Context<Self>) {
        let (loaded, unset) = {
            let prefs = self.prefs.read(cx);
            (prefs.loaded, prefs.setup_guide.is_none())
        };
        if !loaded || !unset {
            return;
        }
        let listing = Workspace::global(cx).read(cx).sidebar(cx);
        let used = !listing.threads.is_empty()
            || listing
                .projects
                .iter()
                .any(|project| project.kind == ProjectKind::Standard);
        self.write_setup_guide(!used, cx);
        self.scan_agents(cx);
    }

    // Hides the guide once every step is done. Runs on each change that can finish a step.
    pub(super) fn reconcile_setup_guide(&mut self, cx: &mut Context<Self>) {
        let (loaded, pref) = {
            let prefs = self.prefs.read(cx);
            (prefs.loaded, prefs.setup_guide)
        };
        if loaded && hides_itself(pref, self.setup_replay, done(self, cx)) {
            self.write_setup_guide(false, cx);
        }
    }

    // Looks for agents on the PATH. Only one look runs at a time.
    pub(super) fn scan_agents(&mut self, cx: &mut Context<Self>) {
        if self.agent_scan.is_some() {
            return;
        }
        let listed = Workspace::dispatch(Op::Program(ProgramOp::Discover), cx);
        self.agent_scan = Some(cx.spawn(async move |this, cx| {
            let agent_ready = match listed.await {
                Ok(OpResult::Programs(list)) => Some(list.agent().is_some()),
                _ => None,
            };
            _ = this.update(cx, |kagan, cx| {
                kagan.agent_scan = None;
                kagan.prefs.update(cx, |prefs, cx| {
                    prefs.agent_ready = agent_ready;
                    cx.notify();
                });
                kagan.reconcile_setup_guide(cx);
            });
        }));
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
    let (loaded, pref, agent_missing) = {
        let prefs = kagan.prefs.read(cx);
        (
            prefs.loaded,
            prefs.setup_guide,
            prefs.agent_ready == Some(false),
        )
    };
    let shown = guide_visible(loaded, pref);
    if !shown {
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
        let hint = index == 0 && agent_missing;
        body = body.child(step(index, title, note, action, done, hint, cx));
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
    hint: bool,
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
            .id(format!("setup-guide-done-{}", index + 1))
            .ui_container()
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
            0 => button.on_click(cx.listener(|this, _, window, cx| {
                this.show_start(ProjectId::personal(), window, cx);
            })),
            1 => button.on_click(dispatch(AddProject)),
            2 => button.on_click(cx.listener(|this, _, window, cx| {
                let project = this
                    .sidebar
                    .read(cx)
                    .listing
                    .projects
                    .iter()
                    .find(|project| project.kind == ProjectKind::Standard)
                    .map_or_else(ProjectId::personal, |project| project.id.clone());
                this.show_start(project, window, cx);
            })),
            _ => button.on_click(cx.listener(|_, _, _, cx| {
                Workspace::dispatch(Op::Settings(SettingsOp::Plugins), cx).detach();
            })),
        };
        control(id.clone(), Facts::button(action), button).into_any_element()
    };
    let mut text = v_flex()
        .flex_1()
        .min_w_0()
        .child(div().child(title.to_owned()))
        .child(
            div()
                .type_small(cx)
                .text_color(muted)
                .child(note.to_owned()),
        );
    if hint {
        text = text.child(
            div().id("setup-guide-install-hint").child(
                Button::new("setup-guide-install-hint-button")
                    .link()
                    .label("How to install an agent")
                    .on_click(|_, _, cx| cx.open_url(INSTALL_HINT)),
            ),
        );
    }
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
        .child(text)
        .child(trailing)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{guide_visible, hides_itself};

    #[test]
    fn the_guide_waits_for_the_saved_choice_to_load() {
        assert!(!guide_visible(false, Some(true)));
        assert!(!guide_visible(false, None));
        assert!(guide_visible(true, Some(true)));
        assert!(!guide_visible(true, None));
        assert!(!guide_visible(true, Some(false)));
    }

    #[test]
    fn the_guide_hides_itself_only_when_all_four_steps_are_done() {
        let all = [true; 4];
        assert!(hides_itself(Some(true), false, all));
        assert!(!hides_itself(Some(true), false, [true, true, true, false]));
    }

    #[test]
    fn a_replay_keeps_the_guide_open_even_when_it_is_all_done() {
        assert!(!hides_itself(Some(true), true, [true; 4]));
    }

    #[test]
    fn a_hidden_or_unsettled_guide_is_not_hidden_again() {
        assert!(!hides_itself(Some(false), false, [true; 4]));
        assert!(!hides_itself(None, false, [true; 4]));
    }
}
