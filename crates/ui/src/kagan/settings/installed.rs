use crate::kagan::dispatch;
use crate::theme::TypeScale as _;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Context, ElementId, Focusable as _, InteractiveElement, IntoElement, ParentElement as _, Role,
    Styled as _, Window, div,
};
use kg_domain::{Op, PluginId, PluginOp, is_git_source};
use kg_plugins::{PluginHost, PluginStatus};

use super::Kagan;
use super::row::{Group, Row, row, switch};
use crate::kagan::controls::{act, answer, control, dispatch};
use crate::kagan::plugin_actions::{
    CheckPluginUpdates, InstallPlugin, ReloadPlugin, RemovePlugin, ShowPluginLogs,
    ToggleInstallForm, TogglePluginDetails, TogglePluginEnabled, UpdateAllPlugins, UpdatePlugin,
};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn group(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> Group {
    let statuses = PluginHost::try_global(cx)
        .map(|host| host.read(cx).status())
        .unwrap_or_default();
    let installing =
        PluginHost::try_global(cx).is_some_and(|host| host.read(cx).installing().is_some());
    let stoppable = PluginHost::try_global(cx).is_some_and(|host| host.read(cx).stoppable());
    let mut rows = if installing {
        vec![install_waiting_row(stoppable)]
    } else {
        vec![install_row(kagan)]
    };
    if statuses
        .iter()
        .any(|status| kg_domain::is_git_source(&status.source))
    {
        rows.push(updates_row(kagan));
    }
    if kagan.plugins.form && !installing {
        rows.push(form_row(kagan, cx));
    }
    if statuses.is_empty() {
        rows.push(Row {
            note_content: Some(
                v_flex()
                    .child("Plugins add panels, pages, commands and tools. Install one from a folder or git, or make one with")
                    .child(div().whitespace_nowrap().font_family(cx.theme().mono_font_family.clone()).child("kg plugin new <name>."))
                    .into_any_element(),
            ),
            ..row(
                "none",
                "No plugins installed",
                "Plugins add panels, pages, commands and tools. Install one from a folder or git, or make one with kg plugin new <name>.",
                div().into_any_element(),
            )
        });
    }
    for status in &statuses {
        rows.extend(plugin_rows(kagan, status, window, cx));
    }
    super::row::group("installed", "Installed", rows)
}

fn install_row(kagan: &Kagan) -> Row {
    let label = if kagan.plugins.form {
        "Cancel"
    } else {
        "Install plugin…"
    };
    row(
        "install",
        "Install a plugin",
        "A folder on this computer, or a git address with a ref: git:<url>@<ref>.",
        control(
            "plugin-install-open",
            Facts::button(label),
            Button::new("button-plugin-install-open")
                .small()
                .outline()
                .label(label)
                .on_click(dispatch(ToggleInstallForm)),
        )
        .into_any_element(),
    )
}

fn updates_row(kagan: &Kagan) -> Row {
    let busy = kagan.plugins.busy;
    let check = control(
        "plugin-updates-check",
        Facts {
            disabled: busy,
            ..Facts::button("Check for updates")
        },
        Button::new("button-plugin-updates-check")
            .small()
            .outline()
            .label("Check for updates")
            .disabled(busy)
            .on_click(dispatch(CheckPluginUpdates)),
    )
    .into_any_element();
    let found = kagan.plugins.updates.as_ref();
    let behind: Vec<&str> = found
        .map(|found| {
            found
                .plugins
                .iter()
                .map(|plugin| plugin.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    if let Some(note) = &kagan.plugins.note {
        return row("plugin-updates", "Plugin updates", note, check);
    }
    if !behind.is_empty() {
        let all = control(
            "plugin-updates-all",
            Facts {
                disabled: busy,
                ..Facts::button("Update all")
            },
            Button::new("button-plugin-updates-all")
                .small()
                .outline()
                .label("Update all")
                .disabled(busy)
                .on_click(dispatch(UpdateAllPlugins)),
        )
        .into_any_element();
        return row(
            "plugin-updates",
            "Plugin updates",
            &format!("Behind their ref: {}.", behind.join(", ")),
            all,
        );
    }
    let note = match found {
        Some(found) if !found.failed.is_empty() => found.failed.join(" "),
        Some(_) => "Every git plugin is up to date.".to_owned(),
        None => "Looks for a newer commit on each git plugin's ref. A plugin pinned to a commit is skipped.".to_owned(),
    };
    row("plugin-updates", "Plugin updates", &note, check)
}

fn form_row(kagan: &Kagan, cx: &mut Context<Kagan>) -> Row {
    let field = kagan.plugins.source.focus_handle(cx);
    let source = div()
        .id("plugin-source")
        .ui_observe_as(Facts::input(
            "Plugin folder or git address",
            &kagan.plugins.source,
            cx,
        ))
        .flex_1()
        .min_w_0()
        .child(
            Input::new(&kagan.plugins.source)
                .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                .small(),
        );
    let install = control(
        "plugin-install",
        Facts::button("Install"),
        Button::new("button-plugin-install")
            .small()
            .primary()
            .label("Install")
            .on_click(move |_, window, cx| {
                window.focus(&field, cx);
                window.dispatch_action(Box::new(InstallPlugin), cx);
            }),
    );
    let choose = control(
        "plugin-choose-folder",
        Facts::button("Choose…"),
        Button::new("button-plugin-choose-folder")
            .small()
            .outline()
            .label("Choose…")
            .on_click(cx.listener(|this, _, _, cx| this.choose_plugin_folder(cx))),
    );
    Row {
        wide: true,
        bare: true,
        ..row(
            "install-form",
            "Folder or git address",
            "",
            div()
                .id("plugin-install-form")
                .key_context("PluginInstall")
                .on_action(cx.listener(|this, _: &InstallPlugin, _, cx| this.install_plugin(cx)))
                .child(h_flex().gap_2().child(source).child(choose).child(install))
                .into_any_element(),
        )
    }
}

fn plugin_rows(
    kagan: &Kagan,
    status: &PluginStatus,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Vec<Row> {
    let id = status.id.clone();
    let open = kagan.plugins.expanded.as_ref() == Some(&id);
    let version = status
        .version
        .as_deref()
        .map_or(String::new(), |version| format!(" {version}"));
    let mut parts = Vec::new();
    if status.error.as_deref().is_some() {
        parts.push("Failed".to_owned());
    } else if !status.enabled {
        parts.push("Off".to_owned());
    }
    if status.slow {
        parts.push("slow".to_owned());
    }
    if let Some(note) = status.note.as_deref() {
        parts.push(note.to_owned());
    }
    let addition = addition_note(status, cx);
    if !addition.is_empty() {
        parts.push(addition);
    }
    let note = parts.join(" · ");
    let focus = super::super::tab_stop(
        window,
        cx,
        ElementId::Name(format!("plugin-row-focus-{}", id.0).into()),
    );
    if kagan.plugins.focus.as_ref() == Some(&id) {
        let focus = focus.clone();
        cx.defer_in(window, move |this, window, cx| {
            this.plugins.focus = None;
            window.focus(&focus, cx);
        });
    }
    let switch_focus = focus.clone();
    let toggle = switch(
        &format!("plugin-{}", id.0),
        status.name.as_str(),
        status.enabled,
        move |_, window, cx| {
            window.focus(&switch_focus, cx);
            window.dispatch_action(Box::new(TogglePluginEnabled), cx);
        },
    );
    let details = act(
        format!("plugin-details-{}", id.0),
        if open { "Hide details" } else { "Details…" },
        focus.clone(),
        TogglePluginDetails,
    );
    let anchor = div()
        .id(ElementId::Name(format!("plugin-row-{}", id.0).into()))
        .ui_observe_as(Facts {
            role: Role::ListBoxOption,
            focus: Some(focus.clone()),
            ..Facts::button(status.name.as_str().to_owned())
        })
        .track_focus(&focus)
        .tab_stop(true)
        .key_context(if is_git_source(&status.source) {
            "PluginRow PluginUpdate"
        } else {
            "PluginRow"
        })
        .focus(|row| row.border_1().border_color(cx.theme().ring))
        .rounded(cx.theme().radius)
        .px_1()
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child("•");
    let anchor = row_keys(anchor, &id, is_git_source(&status.source), cx);
    let head = h_flex()
        .gap_2()
        .items_center()
        .child(anchor)
        .child(details)
        .child(toggle);
    let head = row(
        &id.0,
        &format!("{}{version}", status.name.as_str()),
        &note,
        head.into_any_element(),
    );
    let mut rows = vec![head];
    if open {
        rows.push(details_row(status, &focus));
        if let Some((logged, text)) = &kagan.plugins.logs
            && *logged == id
        {
            rows.push(log_row(logged, text, cx));
        }
    }
    rows
}

fn addition_note(status: &PluginStatus, cx: &mut Context<Kagan>) -> String {
    if !status.enabled {
        return String::new();
    }
    let id = status.id.clone();
    let mut parts = Vec::new();
    if let Ok(list) = kg_plugins::contributions(cx)
        && let Some((_, ui)) = list.iter().find(|(plugin, _)| plugin == &id)
    {
        for panel in &ui.panels {
            match (panel.replaces, panel.placement) {
                (Some(slot), _) => parts.push(format!("offers a {}", slot.name())),
                (None, _) => parts.push(panel.title.clone()),
            }
        }
    }
    if let Some(host) = PluginHost::try_global(cx) {
        let host = host.read(cx);
        let commands = host
            .commands()
            .iter()
            .filter(|command| command.plugin == id)
            .count();
        let settings = host.settings(&id).len();
        if settings > 0 {
            parts.push(format!(
                "{settings} {}",
                if settings == 1 { "setting" } else { "settings" }
            ));
        }
        if commands > 0 {
            parts.push(format!(
                "{commands} {}",
                if commands == 1 { "command" } else { "commands" }
            ));
        }
    }
    match parts.len() {
        0 => String::new(),
        1 => format!("Adds {}.", parts[0]),
        _ => {
            let last = parts.pop().unwrap_or_default();
            format!("Adds {} and {last}.", parts.join(", "))
        }
    }
}

fn row_keys<E: InteractiveElement>(row: E, id: &PluginId, git: bool, cx: &mut Context<Kagan>) -> E {
    let row = answer::<TogglePluginDetails, _>(row, id, cx, |this, id, cx| {
        this.toggle_details(id, cx);
    });
    let row = answer::<TogglePluginEnabled, _>(row, id, cx, |this, id, cx| {
        this.toggle_plugin_enabled(id, cx);
    });
    let row = if git {
        answer::<UpdatePlugin, _>(row, id, cx, |this, id, cx| {
            let op = PluginOp::Update {
                target: kg_domain::UpdateTarget::One(id.clone()),
                thread_id: None,
            };
            this.plugin_op(op, "update the plugin", |_, _, _| {}, cx);
        })
    } else {
        row
    };
    let row = answer::<ReloadPlugin, _>(row, id, cx, |this, id, cx| {
        let op = PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        };
        this.plugin_op(op, "reload the plugin", |_, _, _| {}, cx);
    });
    let row = answer::<ShowPluginLogs, _>(row, id, cx, |this, id, cx| {
        this.show_plugin_logs(id, cx);
    });
    answer::<RemovePlugin, _>(row, id, cx, |this, id, cx| {
        let op = PluginOp::Remove {
            id: id.clone(),
            thread_id: None,
        };
        this.plugin_op(op, "remove the plugin", |_, _, _| {}, cx);
    })
}

fn details_row(status: &PluginStatus, focus: &gpui_kit::FocusHandle) -> Row {
    let id = status.id.clone();
    let powers = if status.powers.is_empty() {
        "no powers".to_owned()
    } else {
        status.powers.join("; ")
    };
    let mut note = format!("{} · {powers}", status.source);
    if let Some(error) = status.error.as_deref() {
        note = format!("{note} · {error}");
    }
    if let Some(extra) = status.note.as_deref() {
        note = format!("{note} · {extra}");
    }
    let git = is_git_source(&status.source);
    let mut actions = h_flex().gap_2().flex_wrap();
    if git {
        actions = actions.child(act(
            format!("plugin-update-{}", id.0),
            "Update",
            focus.clone(),
            UpdatePlugin,
        ));
    }
    let actions = actions
        .child(act(
            format!("plugin-reload-{}", id.0),
            "Reload",
            focus.clone(),
            ReloadPlugin,
        ))
        .child(act(
            format!("plugin-logs-{}", id.0),
            "Logs",
            focus.clone(),
            ShowPluginLogs,
        ))
        .child(act(
            format!("plugin-remove-{}", id.0),
            "Remove…",
            focus.clone(),
            RemovePlugin,
        ));
    Row {
        wide: true,
        ..row(
            &format!("{}-details", id.0),
            "Details",
            &note,
            actions.into_any_element(),
        )
    }
}

fn log_row(id: &PluginId, text: &str, cx: &mut Context<Kagan>) -> Row {
    let body = if text.is_empty() {
        div()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child("No log lines yet.")
            .into_any_element()
    } else {
        v_flex()
            .gap_0p5()
            .children(
                text.lines()
                    .map(|line| div().type_small(cx).child(line.to_owned())),
            )
            .into_any_element()
    };
    let body = div()
        .id(ElementId::Name(format!("plugin-log-{}", id.0).into()))
        .ui_observe_as(Facts {
            role: Role::Label,
            value: Some(text.to_owned().into()),
            ..Facts::button("Recent log")
        })
        .child(body)
        .into_any_element();
    Row {
        wide: true,
        ..row(
            "log",
            "Recent log",
            "The last lines this plugin wrote.",
            body,
        )
    }
}

fn install_waiting_row(stoppable: bool) -> Row {
    if !stoppable {
        return row(
            "install-waiting",
            "Installing plugin…",
            "",
            div().into_any_element(),
        );
    }
    row(
        "install-waiting",
        "Installing plugin…",
        "Cancel stops the download. A git address gives up after 2 minutes.",
        control(
            "plugin-install-cancel",
            Facts::button("Cancel"),
            Button::new("button-plugin-install-cancel")
                .small()
                .outline()
                .label("Cancel")
                .on_click(|_, _, cx| {
                    dispatch::logged(Op::Plugin(PluginOp::CancelInstall), cx);
                }),
        )
        .into_any_element(),
    )
}

// What an update-all run did, in words the person can act on.
pub(in crate::kagan) fn update_outcome(
    updated: &[PluginId],
    failed: &[String],
    needs_approval: &[PluginId],
) -> String {
    let mut parts = Vec::new();
    if !updated.is_empty() {
        let names: Vec<&str> = updated.iter().map(|id| id.0.as_str()).collect();
        parts.push(format!("Updated {}.", names.join(", ")));
    }
    parts.extend(failed.iter().map(|line| format!("Not updated: {line}.")));
    for id in needs_approval {
        parts.push(format!(
            "{} needs approval for new powers: update it alone to approve them.",
            id.0
        ));
    }
    if parts.is_empty() {
        "Nothing to update.".to_owned()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use kg_domain::PluginId;

    use super::update_outcome;

    #[test]
    fn an_update_outcome_names_each_failure_and_each_plugin_that_needs_approval() {
        let note = update_outcome(
            &[PluginId("dev.test.a".into())],
            &["dev.test.b: remote is gone".into()],
            &[PluginId("dev.test.c".into())],
        );
        assert!(note.contains("dev.test.b: remote is gone"), "{note}");
        assert!(note.contains("dev.test.c"), "{note}");
        assert!(note.contains("approve"), "{note}");
    }
}
