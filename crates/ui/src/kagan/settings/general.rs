use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, ElementId, IntoElement, ParentElement as _, Role, Styled as _, div, px,
};
use kg_domain::{NotifyLevel, Op, PreferenceOp};
use serde_json::json;

use super::Kagan;
use super::pages::set_flag;
use super::row::{Group, Row, group, row, save, switch};
use crate::kagan::controls::control;
use crate::theme::radius_md;
use crate::ui_registry::Facts;

pub(super) fn setup_guide_group(cx: &mut Context<Kagan>) -> Group {
    group(
        "setup-guide",
        "Setup guide",
        vec![row(
            "setup-guide",
            "Setup guide",
            "Show the first steps again on the empty window.",
            control(
                "setup-guide-show",
                Facts::button("Show setup guide"),
                Button::new("setup-guide-show-button")
                    .small()
                    .outline()
                    .label("Show setup guide")
                    .on_click(cx.listener(|this, _, _, cx| this.set_setup_guide(true, cx))),
            )
            .into_any_element(),
        )],
    )
}

pub(super) fn branch_group(kagan: &Kagan, cx: &App) -> Group {
    let input = kagan.settings.branch_prefix.clone();
    let text = input.read(cx).value().trim().to_owned();
    let valid = kg_domain::prefs::valid_branch_prefix(&text);
    let danger = cx.theme().danger;
    let note = if valid {
        "Goes in front of the branch names kagan makes. It must end in / or -, such as bb/. Empty adds nothing.".to_owned()
    } else {
        format!(
            "Not a valid branch start. {}",
            kg_domain::prefs::BRANCH_PREFIX_RULE
        )
    };
    let field = control(
        "branch-prefix",
        Facts::input("Branch prefix", &input, cx),
        Input::new(&input).small().w(px(220.)),
    );
    let field = div()
        .rounded(px(radius_md(cx)))
        .when(!valid, |frame| frame.border_1().border_color(danger))
        .child(field.into_any_element());
    let mut prefix = row(
        "branch-prefix",
        "Branch prefix",
        &note,
        field.into_any_element(),
    );
    if !valid {
        prefix.note_content = Some(div().text_color(danger).child(note).into_any_element());
    }
    group("branches", "Branches", vec![prefix])
}

pub(super) fn general(agent: bool, notify: NotifyLevel, confirm_archive: bool) -> Group {
    group(
        "agents",
        "Agents",
        vec![
            row(
                "agent-integration",
                "Agent integration",
                "Off keeps kagan’s tools out of the agents it starts.",
                switch(
                    "agent-integration",
                    "Agent integration",
                    agent,
                    |on, _, cx| {
                        set_flag(kg_domain::prefs::AGENT_INTEGRATION, on, cx);
                    },
                ),
            ),
            notify_row(notify),
            row(
                "confirm-archive",
                "Ask before archiving",
                "Ask before a thread moves to Archived.",
                switch(
                    "confirm-archive",
                    "Ask before archiving",
                    confirm_archive,
                    |on, _, cx| {
                        set_flag(kg_domain::prefs::CONFIRM_ARCHIVE, on, cx);
                    },
                ),
            ),
        ],
    )
}

pub(super) fn notify_row(level: NotifyLevel) -> Row {
    let choices = [
        (NotifyLevel::All, "all", "All"),
        (NotifyLevel::Asks, "asks", "Only when it asks me"),
        (NotifyLevel::Off, "off", "Off"),
    ];
    let control = h_flex()
        .gap_1()
        .children(choices.map(|(choice, id, label)| {
            let button = Button::new(ElementId::Name(format!("button-notify-{id}").into()))
                .small()
                .label(label)
                .on_click(move |_, _, cx| {
                    save(
                        Op::Preference(PreferenceOp::Set {
                            key: kg_domain::prefs::NOTIFY.into(),
                            value: json!(choice),
                        }),
                        cx,
                    );
                });
            let button = if choice == level {
                button.primary()
            } else {
                button.outline()
            };
            control(
                format!("notify-{id}"),
                Facts {
                    role: Role::RadioButton,
                    selected: Some(choice == level),
                    ..Facts::button(label)
                },
                button,
            )
        }));
    Row {
        wide: true,
        ..row(
            "notify",
            "Notifications",
            "Post a system notification for a thread that is off screen.",
            control.into_any_element(),
        )
    }
}

// The agent rules row sits in the Agents group: a second group in General would add a heading.
pub(super) fn agents(
    kagan: &Kagan,
    agent: bool,
    notify: NotifyLevel,
    confirm_archive: bool,
) -> Group {
    let mut group = general(agent, notify, confirm_archive);
    let mut rules = row(
        "agent-rules",
        "Agent rules",
        "Text each agent is told when it starts. Projects add their own from the project menu.",
        kagan.settings.rules.clone().into_any_element(),
    );
    rules.wide = true;
    group.rows.push(rules);
    group
}

pub(super) fn plugin_status_row(on: bool) -> Row {
    row(
        "plugin-status",
        "Plugin status",
        "Show items that plugins add to the activity strip. Off hides them all; each plugin can still be turned off in Installed.",
        switch("plugin-status", "Plugin status", on, |on, _, cx| {
            set_flag(kg_domain::prefs::PLUGIN_STATUS, on, cx);
        }),
    )
}

pub(super) fn copy_on_select_row(on: bool) -> Row {
    row(
        "copy-on-select",
        "Copy on select",
        "Selecting terminal text copies it.",
        switch("copy-on-select", "Copy on select", on, |on, _, cx| {
            set_flag(kg_domain::prefs::TERMINAL_COPY_ON_SELECT, on, cx);
        }),
    )
}

pub(super) fn links_row(on: bool) -> Row {
    row(
        "terminal-links",
        "Open links in the Browser tab",
        "Click a link in a thread with Cmd, or Ctrl off macOS, to open it in the Browser tab.",
        switch(
            "terminal-links",
            "Open links in the Browser tab",
            on,
            |on, _, cx| {
                set_flag(kg_domain::prefs::TERMINAL_LINKS_BROWSER, on, cx);
            },
        ),
    )
}

pub(super) fn environment(kagan: &Kagan) -> Group {
    group(
        "environment",
        "Environment variables",
        vec![row(
            "environment-variables",
            "Variables",
            "Set once for every program kagan starts. A value is hidden after you set it.",
            kagan.settings.variables.clone().into_any_element(),
        )],
    )
}
