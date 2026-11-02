//! Settings → Updates: the running version, Check now, and the automatic check.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex};
use gpui_kit::{App, IntoElement, ParentElement as _};
use kg_core::Updater;
use kg_domain::{Op, PreferenceOp, UpdateState};
use serde_json::json;

use super::pages::{Group, group, row, save, switch};
use crate::kagan::shown::control;
use crate::ui_registry::Facts;

pub(super) fn group_for(automatic: bool, cx: &App) -> Group {
    let updater = Updater::global(cx).read(cx);
    let status = match updater.state() {
        UpdateState::Unchecked => "Not checked yet.".to_owned(),
        UpdateState::Checking => "Checking…".to_owned(),
        UpdateState::Current => "This is the newest version.".to_owned(),
        UpdateState::Available { version, .. } => format!("kagan {version} is available."),
        UpdateState::Downloading { version } => format!("Downloading kagan {version}…"),
        UpdateState::Ready {
            version,
            at_quit: true,
            ..
        } => format!("kagan {version} installs when kagan quits."),
        UpdateState::Ready { version, .. } => format!("kagan {version} is ready to install."),
        UpdateState::Failed { reason } => format!("Could not check: {reason}"),
    };
    let busy = matches!(
        updater.state(),
        UpdateState::Checking | UpdateState::Downloading { .. }
    );
    let check = control(
        "update-check-now",
        Facts {
            disabled: busy,
            ..Facts::button("Check now")
        },
        Button::new("update-check-now-button")
            .small()
            .outline()
            .label("Check now")
            .disabled(busy)
            .on_click(|_, _, cx| super::super::update::check(cx)),
    );
    let version = row(
        "update-version",
        &format!("kagan {}", updater.version()),
        &status,
        h_flex().child(check).into_any_element(),
    );
    group(
        "updates",
        "Updates",
        vec![
            version,
            row(
                "update-check",
                "Check automatically",
                "Off by default. On, kagan asks GitHub for the newest release at launch and once a day. Check now always works.",
                switch(
                    "update-check",
                    "Check automatically",
                    automatic,
                    |on, _, cx| {
                        save(
                            Op::Preference(PreferenceOp::Set {
                                key: "update-check".into(),
                                value: json!(on),
                            }),
                            cx,
                        );
                    },
                ),
            ),
        ],
    )
}
