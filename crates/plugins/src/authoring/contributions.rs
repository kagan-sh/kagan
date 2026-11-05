use std::collections::BTreeSet;
use std::path::{Component, Path};

use kg_domain::Slot;
use serde::Deserialize;
use serde::de::IgnoredAny;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Contributions {
    /// The panels this plugin declares, in manifest order.
    pub panels: Vec<PanelContribution>,
    /// The exported view class that draws this plugin's settings page.
    pub settings: Option<String>,
    /// The exported view class that draws over the terminal.
    pub overlay: Option<String>,
    // Spots kagan does not draw yet. They are read only to refuse them: see `check_spots`.
    pub(crate) toolbar: Vec<IgnoredAny>,
    pub(crate) status_bar: Vec<IgnoredAny>,
    pub(crate) sidebar_footer: Vec<IgnoredAny>,
    pub(crate) thread_list: Option<IgnoredAny>,
    pub(crate) file_opener: Option<IgnoredAny>,
}

/// Why a manifest's UI contributions cannot be used.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(
        "panel {0} must be placed left, right or center: bottom panels are not in this version of kagan"
    )]
    BottomPanel(String),
    #[error("panel {0} needs a plain title")]
    PlainTitle(String),
    #[error("panel {0} replaces threads, so it must be placed left")]
    ReplacesThreads(String),
    #[error("panel {0} replaces review, so it must be placed center")]
    ReplacesReview(String),
    #[error("only one panel may replace {0}")]
    OneReplacement(&'static str),
    #[error(
        "ui.thread-list is not in this version of kagan; declare a left panel with \"replaces\": \"threads\""
    )]
    ThreadList,
    #[error("ui.{0} is not in this version of kagan; remove it")]
    Spot(&'static str),
    #[error("UI contribution id {0:?} is blank, repeated or contains control characters")]
    BlankId(String),
    #[error("panel {0} uses a view, so store belongs on WebView.store(true)")]
    StoreOnView(String),
    #[error("panel {0} is a web panel, so it cannot replace a host slot")]
    WebReplaces(String),
    #[error("panel {0} needs a view or a web page, and not both")]
    Body(String),
    #[error("web {0:?} must be an .html file inside the plugin")]
    WebShape(String),
    #[error("UI view {0:?} must name an exported class")]
    ViewShape(String),
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
    /// The `ui` key this panel is declared under.
    pub id: String,
    /// The title the window shows for the panel.
    pub title: String,
    /// The exported view class the panel draws, when it is a native view.
    #[serde(default)]
    pub view: Option<String>,
    /// The HTML file this panel shows, when its body is a web page.
    #[serde(default)]
    pub web: Option<String>,
    /// Whether the page keeps cookies and storage under this plugin's data folder.
    #[serde(default)]
    pub store: bool,
    /// Where the panel is placed.
    pub placement: Placement,
    /// Whether the panel follows the shown thread.
    pub follows: Option<Follows>,
    /// The slot this panel offers to fill, when it is a replacement.
    pub replaces: Option<Slot>,
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
    pub(crate) fn check(&self) -> Result<(), Error> {
        self.check_spots()?;
        self.check_panels()?;
        self.check_views()
    }

    fn check_panels(&self) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        for panel in &self.panels {
            check_id(&panel.id, &mut ids)?;
            if matches!(panel.placement, Placement::Bottom) {
                return Err(Error::BottomPanel(panel.id.clone()));
            }
            check_body(panel)?;
            if panel.title.trim().is_empty() || panel.title.chars().any(char::is_control) {
                return Err(Error::PlainTitle(panel.id.clone()));
            }
            match (panel.replaces, panel.placement) {
                (Some(Slot::Threads), Placement::Left)
                | (Some(Slot::Review), Placement::Center)
                | (None, _) => {}
                (Some(Slot::Threads), _) => {
                    return Err(Error::ReplacesThreads(panel.id.clone()));
                }
                (Some(Slot::Review), _) => {
                    return Err(Error::ReplacesReview(panel.id.clone()));
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
                return Err(Error::OneReplacement(slot.id()));
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

    /// The spots kagan does not draw yet. A manifest that declares one is refused, so a plugin never
    /// loads as if its spot worked.
    fn check_spots(&self) -> Result<(), Error> {
        let spots = [
            ("toolbar", !self.toolbar.is_empty()),
            ("status-bar", !self.status_bar.is_empty()),
            ("sidebar-footer", !self.sidebar_footer.is_empty()),
            ("thread-list", self.thread_list.is_some()),
            ("file-opener", self.file_opener.is_some()),
        ];
        match spots.iter().find(|(_, declared)| *declared) {
            Some(("thread-list", _)) => Err(Error::ThreadList),
            Some((spot, _)) => Err(Error::Spot(spot)),
            None => Ok(()),
        }
    }

    fn check_views(&self) -> Result<(), Error> {
        for view in [&self.settings, &self.overlay].into_iter().flatten() {
            check_view(view)?;
        }
        Ok(())
    }
}

fn check_id<'a>(id: &'a str, ids: &mut BTreeSet<&'a str>) -> Result<(), Error> {
    if id.trim().is_empty() || id.chars().any(char::is_control) || !ids.insert(id) {
        return Err(Error::BlankId(id.to_owned()));
    }
    Ok(())
}

fn check_body(panel: &PanelContribution) -> Result<(), Error> {
    match (panel.view.as_deref(), panel.web.as_deref()) {
        (Some(view), None) => {
            if panel.store {
                return Err(Error::StoreOnView(panel.id.clone()));
            }
            check_view(view)
        }
        (None, Some(web)) => {
            if panel.replaces.is_some() {
                return Err(Error::WebReplaces(panel.id.clone()));
            }
            check_web(web)
        }
        _ => Err(Error::Body(panel.id.clone())),
    }
}

fn check_web(web: &str) -> Result<(), Error> {
    let path = Path::new(web);
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
        && !web.starts_with('/')
        && !web.contains('\\')
        && !web.contains('\0')
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && !web
            .split('/')
            .any(|part| part.is_empty() || part.eq_ignore_ascii_case("__kagan"))
    {
        Ok(())
    } else {
        Err(Error::WebShape(web.to_owned()))
    }
}

fn check_view(view: &str) -> Result<(), Error> {
    if view.is_empty()
        || !view.bytes().enumerate().all(|(ix, byte)| {
            byte.is_ascii_alphabetic()
                || byte == b'_'
                || byte == b'$'
                || (ix > 0 && byte.is_ascii_digit())
        })
    {
        return Err(Error::ViewShape(view.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Contributions, Error};

    #[test]
    fn a_refused_spot_is_a_typed_error() {
        let contributions: Contributions = serde_json::from_str(r#"{"toolbar": [{}]}"#).unwrap();
        let error = contributions.check().unwrap_err();
        assert!(matches!(&error, Error::Spot("toolbar")), "{error}");
        assert_eq!(
            error.to_string(),
            "ui.toolbar is not in this version of kagan; remove it"
        );
    }
}
