use std::sync::OnceLock;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::{AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _, div};
use kg_plugins::PluginHost;
use serde::Deserialize;

use super::Kagan;
use super::row::{Group, Row, group as row_group, row};
use crate::kagan::controls::control;
use crate::theme::TypeScale as _;
use crate::ui_registry::Facts;

#[derive(Deserialize)]
pub(super) struct Suggested {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) source: String,
    // The oldest kagan the plugin's manifest accepts. Checked against this build.
    pub(super) kagan: String,
}

#[derive(Deserialize)]
struct Shipped {
    plugins: Vec<Suggested>,
}

pub(super) fn list() -> &'static [Suggested] {
    static LIST: OnceLock<Vec<Suggested>> = OnceLock::new();
    LIST.get_or_init(|| {
        serde_json::from_str::<Shipped>(include_str!("suggested_plugins.json"))
            .map(|shipped| shipped.plugins)
            .unwrap_or_default()
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Install {
    Ready,
    Needs(String),
}

// A plugin needs a newer kagan than this build when the build is below its minimum.
pub(super) fn install_state(minimum: &str, running: &str) -> Install {
    match kg_plugins::kagan_at_least(minimum, running) {
        Some(true) => Install::Ready,
        _ => Install::Needs(minimum.to_owned()),
    }
}

// Shown only while no plugin index is added, and only for plugins not installed yet.
pub(super) fn group(cx: &mut Context<Kagan>) -> Option<Group> {
    let host = PluginHost::try_global(cx)?;
    let host = host.read(cx);
    if !host.indexes().ok()?.is_empty() {
        return None;
    }
    let installed: Vec<String> = host
        .status()
        .iter()
        .map(|status| status.id.to_string())
        .collect();
    let rows: Vec<Row> = list()
        .iter()
        .filter(|suggested| !installed.contains(&suggested.id))
        .map(|suggested| {
            let state = install_state(&suggested.kagan, kg_plugins::KAGAN_VERSION);
            row(
                &suggested.id,
                &suggested.name,
                &suggested.description,
                install_control(suggested, &state, cx),
            )
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    Some(row_group("suggested", "Suggested plugins", rows))
}

fn install_control(suggested: &Suggested, state: &Install, cx: &mut Context<Kagan>) -> AnyElement {
    let needs = matches!(state, Install::Needs(_));
    let source = suggested.source.clone();
    let button = Button::new(format!("suggested-install-button-{}", suggested.id))
        .small()
        .outline()
        .label("Install")
        .disabled(needs)
        .on_click(cx.listener(move |this, _, _, cx| this.install_source(source.clone(), cx)));
    let mut line = h_flex().gap_2().items_center();
    if let Install::Needs(minimum) = state {
        line = line.child(
            div()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(format!("Needs kagan {minimum} or later")),
        );
    }
    line.child(control(
        format!("suggested-install-{}", suggested.id),
        Facts {
            disabled: needs,
            ..Facts::button("Install")
        },
        button,
    ))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Install, install_state, list};

    #[test]
    fn a_build_that_meets_the_minimum_can_install() {
        assert_eq!(install_state("0.1.3", "0.9.0"), Install::Ready);
        assert_eq!(install_state("0.1.3", "0.1.3"), Install::Ready);
    }

    #[test]
    fn a_build_below_the_minimum_shows_what_it_needs() {
        assert_eq!(
            install_state("0.1.3", "0.1.0"),
            Install::Needs("0.1.3".into())
        );
    }

    #[test]
    fn the_shipped_list_has_five_plugins_with_readable_minimums() {
        let plugins = list();
        assert_eq!(plugins.len(), 5);
        for plugin in plugins {
            assert!(plugin.id.starts_with("dev.kagan."), "{}", plugin.id);
            assert!(
                kg_plugins::kagan_at_least(&plugin.kagan, "0.0.0").is_some(),
                "{}: {}",
                plugin.id,
                plugin.kagan
            );
        }
    }

    #[test]
    fn every_pin_is_a_full_commit_or_a_release_tag() {
        for plugin in list() {
            let (address, pin) = plugin
                .source
                .rsplit_once('@')
                .unwrap_or_else(|| panic!("{}: no pin", plugin.id));
            assert!(
                address.starts_with("git:https://github.com/kagan-sh/"),
                "{}",
                plugin.source
            );
            assert!(
                is_commit(pin) || is_release_tag(pin),
                "{}: {pin}",
                plugin.id
            );
        }
    }

    fn is_commit(pin: &str) -> bool {
        pin.len() == 40 && pin.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn is_release_tag(pin: &str) -> bool {
        let Some(version) = pin.strip_prefix('v') else {
            return false;
        };
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    }
}
