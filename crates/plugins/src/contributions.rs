use std::collections::BTreeSet;

use kg_domain::Slot;
use serde::Deserialize;
use serde::de::IgnoredAny;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Contributions {
    pub(crate) panels: Vec<PanelContribution>,
    pub(crate) settings: Option<String>,
    pub(crate) overlay: Option<String>,
    // Spots kagan does not draw yet. They are read only to refuse them: see `check_spots`.
    pub(crate) toolbar: Vec<IgnoredAny>,
    pub(crate) status_bar: Vec<IgnoredAny>,
    pub(crate) sidebar_footer: Vec<IgnoredAny>,
    pub(crate) thread_list: Option<IgnoredAny>,
    pub(crate) file_opener: Option<IgnoredAny>,
}

/// One thing a plugin can add. The ids are the `ui` keys on [`Contributions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    pub detail: &'static str,
}

impl Contributions {
    #[must_use]
    pub fn kinds() -> &'static [Capability] {
        &[
            Capability {
                id: "panels",
                name: "Panel",
                detail: "adds a panel",
            },
            Capability {
                id: "settings",
                name: "Settings page",
                detail: "adds a settings page",
            },
            Capability {
                id: "overlay",
                name: "Overlay",
                detail: "draws over the terminal",
            },
        ]
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelContribution {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) view: String,
    pub(crate) placement: Placement,
    pub(crate) follows: Option<Follows>,
    pub(crate) replaces: Option<Slot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Left,
    Right,
    Bottom,
    Center,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Follows {
    Thread,
}

impl Contributions {
    pub(crate) fn check(&self) -> Result<(), String> {
        self.check_spots()?;
        self.check_panels()?;
        self.check_views()
    }

    fn check_panels(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for panel in &self.panels {
            check_id(&panel.id, &mut ids)?;
            if matches!(panel.placement, Placement::Bottom) {
                return Err(format!(
                    "panel {} must be placed left or center: bottom panels are not in this version of kagan",
                    panel.id
                ));
            }
            check_view(&panel.view)?;
            if panel.title.trim().is_empty() || panel.title.chars().any(char::is_control) {
                return Err(format!("panel {} needs a plain title", panel.id));
            }
            match (panel.replaces, panel.placement) {
                (Some(Slot::Threads), Placement::Left)
                | (Some(Slot::Review), Placement::Center)
                | (None, _) => {}
                (Some(Slot::Threads), _) => {
                    return Err(format!(
                        "panel {} replaces threads, so it must be placed left",
                        panel.id
                    ));
                }
                (Some(Slot::Review), _) => {
                    return Err(format!(
                        "panel {} replaces review, so it must be placed center",
                        panel.id
                    ));
                }
            }
        }
        for slot in Slot::ALL {
            if self
                .panels
                .iter()
                .filter(|panel| panel.replaces == Some(slot))
                .count()
                > 1
            {
                return Err(format!("only one panel may replace {}", slot.id()));
            }
        }
        Ok(())
    }

    /// The panel that offers to fill `slot`.
    #[must_use]
    pub fn replacement(&self, slot: Slot) -> Option<&PanelContribution> {
        self.panels
            .iter()
            .find(|panel| panel.replaces == Some(slot))
    }

    /// `kg plugin check` and a new install refuse this. An already installed plugin still loads.
    pub(crate) fn right_panel_error(&self) -> Option<String> {
        self.panels
            .iter()
            .find(|panel| panel.placement == Placement::Right)
            .map(|panel| {
                format!(
                    "panel \"{}\": placement \"right\" is not supported. Use \"left\" or \"center\".",
                    panel.id
                )
            })
    }

    pub(crate) fn skipped_right(&self) -> Option<String> {
        let titles: Vec<_> = self
            .panels
            .iter()
            .filter(|panel| panel.placement == Placement::Right)
            .map(|panel| panel.title.as_str())
            .collect();
        let verb = if titles.len() == 1 { "isn't" } else { "aren't" };
        match titles.as_slice() {
            [] => None,
            _ => Some(format!(
                "{} {verb} shown: kagan no longer draws right panels. Update the plugin, or ask its author to move it to left or center.",
                titles.join(", ")
            )),
        }
    }

    /// The spots kagan does not draw yet. A manifest that declares one is refused, so a plugin never
    /// loads as if its spot worked.
    fn check_spots(&self) -> Result<(), String> {
        let spots = [
            ("toolbar", !self.toolbar.is_empty()),
            ("status-bar", !self.status_bar.is_empty()),
            ("sidebar-footer", !self.sidebar_footer.is_empty()),
            ("thread-list", self.thread_list.is_some()),
            ("file-opener", self.file_opener.is_some()),
        ];
        match spots.iter().find(|(_, declared)| *declared) {
            Some(("thread-list", _)) => Err(
                "ui.thread-list is not in this version of kagan; declare a left panel with \"replaces\": \"threads\""
                    .into(),
            ),
            Some((spot, _)) => Err(format!(
                "ui.{spot} is not in this version of kagan; remove it"
            )),
            None => Ok(()),
        }
    }

    fn check_views(&self) -> Result<(), String> {
        for view in [&self.settings, &self.overlay].into_iter().flatten() {
            check_view(view)?;
        }
        Ok(())
    }
}

fn check_id<'a>(id: &'a str, ids: &mut BTreeSet<&'a str>) -> Result<(), String> {
    if id.trim().is_empty() || id.chars().any(char::is_control) || !ids.insert(id) {
        return Err(format!(
            "UI contribution id {id:?} is blank, repeated or contains control characters"
        ));
    }
    Ok(())
}

fn check_view(view: &str) -> Result<(), String> {
    if view.is_empty()
        || !view.bytes().enumerate().all(|(ix, byte)| {
            byte.is_ascii_alphabetic()
                || byte == b'_'
                || byte == b'$'
                || (ix > 0 && byte.is_ascii_digit())
        })
    {
        return Err(format!("UI view {view:?} must name an exported class"));
    }
    Ok(())
}

impl Contributions {
    #[must_use]
    pub fn panels(&self) -> &[PanelContribution] {
        &self.panels
    }
    #[must_use]
    pub fn settings(&self) -> Option<&str> {
        self.settings.as_deref()
    }
    #[must_use]
    pub fn overlay(&self) -> Option<&str> {
        self.overlay.as_deref()
    }
}
impl PanelContribution {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn view(&self) -> &str {
        &self.view
    }
    #[must_use]
    pub fn placement(&self) -> Placement {
        self.placement
    }
    #[must_use]
    pub fn follows(&self) -> Option<Follows> {
        self.follows
    }
    /// The slot this panel offers to fill, when it is a replacement.
    #[must_use]
    pub fn replaces(&self) -> Option<Slot> {
        self.replaces
    }
}
