use gpui_kit::{App, Context, Window};
use kg_domain::{Op, PluginId, PreferenceOp, Slot};
use kg_plugins::{PluginHost, Setting};
use serde_json::json;

use super::Kagan;
use super::appearance::{density_row, mode_group, terminal_size, theme_group};
use super::fonts::font_row;
use super::general::{
    agents, branch_group, copy_on_select_row, environment, links_row, plugin_status_row,
    setup_guide_group, usage_row,
};
use super::indexes;
use super::installed;
use super::nav::Category;
use super::plugins;
use super::row::{Group, group, save};

pub(super) fn groups(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> Vec<Group> {
    let prefs = kagan.prefs.read(cx);
    let (agent, notify) = (prefs.agent_integration, prefs.notify);
    let (confirm_archive, plugin_status) = (prefs.confirm_archive, prefs.plugin_status);
    let (copy_on_select, links_in_tab) = (prefs.copy_on_select, prefs.links_in_tab);
    let usage_limits = prefs.usage_limits;
    let mut appearance = theme_group(window, cx);
    appearance.key = "appearance".into();
    appearance.title = "Appearance".into();
    appearance.rows.extend(mode_group(cx).rows);
    appearance.rows.push(density_row(cx));
    appearance.rows.push(font_row(
        "ui-font",
        "UI font",
        "Menus, lists and text. Default is Inter.",
        &kagan.settings.ui_font,
        cx,
    ));
    appearance.rows.push(font_row(
        "terminal-font",
        "Terminal font",
        "The terminal, code and diffs. Default is JetBrains Mono.",
        &kagan.settings.terminal_font,
        cx,
    ));
    appearance.rows.push(terminal_size(kagan));
    let mut out = vec![
        within(
            agents(kagan, agent, notify, confirm_archive),
            Category::General,
        ),
        within(branch_group(kagan, cx), Category::General),
        within(
            group(
                "terminal",
                "Terminal",
                vec![copy_on_select_row(copy_on_select), links_row(links_in_tab)],
            ),
            Category::General,
        ),
        within(setup_guide_group(cx), Category::General),
        within(super::command::group(kagan, cx), Category::General),
        within(environment(kagan), Category::General),
        within(appearance, Category::Appearance),
    ];
    out.extend(
        super::keyboard::groups(kagan, window, cx)
            .into_iter()
            .map(|group| within(group, Category::Keyboard)),
    );
    let mut installed = installed::group(kagan, window, cx);
    installed.title = "Installed".into();
    let slots = Slot::ALL.map(|slot| super::slots::row(kagan, slot, cx));
    out.push(within(installed, Category::Installed));
    out.push(within(indexes::group(kagan, cx), Category::Installed));
    if let Some(suggested) = super::suggested::group(cx) {
        out.push(within(suggested, Category::Installed));
    }
    out.push(within(
        group("usage", "Usage", vec![usage_row(usage_limits)]),
        Category::General,
    ));
    out.push(within(
        group(
            "activity",
            "Activity",
            vec![plugin_status_row(plugin_status)],
        ),
        Category::Installed,
    ));
    out.push(within(
        group("slots", "Replace parts", slots.into()),
        Category::Installed,
    ));
    out.push(within(super::updates::group(kagan, cx), Category::Updates));
    for (id, _) in plugin_entries(cx) {
        for group in plugins::groups(kagan, &id, cx) {
            out.push(within(group, Category::Plugin(id.clone())));
        }
    }
    headed(out)
}

pub(super) fn set_flag(key: &str, on: bool, cx: &mut App) {
    save(
        Op::Preference(PreferenceOp::Set {
            key: key.into(),
            value: json!(on),
        }),
        cx,
    );
}

fn within(mut group: Group, section: Category) -> Group {
    group.section = section;
    group
}

// A section with one group needs no heading: the section name already says it.
fn headed(mut out: Vec<Group>) -> Vec<Group> {
    for index in 0..out.len() {
        let section = out[index].section.clone();
        let groups = out.iter().filter(|group| group.section == section).count();
        out[index].headed = groups > 1;
    }
    out
}

pub(super) fn plugin_entries(cx: &App) -> Vec<(PluginId, String)> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    let views: Vec<PluginId> = kg_plugins::contributions(cx)
        .map(|list| {
            list.into_iter()
                .filter(|(_, ui)| ui.settings.as_deref().is_some())
                .map(|(id, _)| id)
                .collect()
        })
        .unwrap_or_default();
    let host = host.read(cx);
    host.status()
        .iter()
        .filter(|status| status.enabled)
        .filter(|status| {
            views.contains(&status.id)
                || host
                    .settings(&status.id)
                    .iter()
                    .any(|(_, setting, _)| !matches!(setting, Setting::Provider { .. }))
        })
        .map(|status| (status.id.clone(), status.name.as_str().to_owned()))
        .collect()
}
