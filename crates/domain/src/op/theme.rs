use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    /// Follow the operating system's light or dark mode
    #[default]
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettings {
    pub theme: String,
    pub mode: Appearance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ThemeOp {
    /// Selected theme and mode
    Get,
    /// Select a theme and mode
    Set {
        #[arg(value_parser = theme_choice)]
        theme: String,
        #[arg(value_enum)]
        mode: Appearance,
    },
    /// Copy a theme to `<dataDir>/themes/<id>.json` and print its path
    New {
        /// The new file's id: lowercase letters, digits and dashes
        #[arg(value_parser = theme_id)]
        id: String,
        /// The theme to copy; the one in use when left out
        #[arg(long, value_parser = theme_choice)]
        from: Option<String>,
    },
    /// Print the folder of your own theme files, making it when missing
    Folder,
}

#[must_use]
pub fn valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// A theme id, or a plugin's theme as `<plugin id>/<theme id>`.
#[must_use]
pub fn valid_theme_choice(id: &str) -> bool {
    match id.split_once('/') {
        Some((plugin, theme)) => valid_theme_id(theme) && crate::PluginId(plugin.into()).valid(),
        None => valid_theme_id(id),
    }
}

// shortcut: a clap value parser reports the message as a String for the CLI to print.
fn theme_id(id: &str) -> Result<String, String> {
    if valid_theme_id(id) {
        Ok(id.to_owned())
    } else {
        Err("theme id must contain only lowercase letters, digits and dashes".into())
    }
}

fn theme_choice(id: &str) -> Result<String, String> {
    if valid_theme_choice(id) {
        Ok(id.to_owned())
    } else {
        Err(
            "theme must be an id of lowercase letters, digits and dashes, or <plugin id>/<id>"
                .into(),
        )
    }
}
