//! The lines `kg plugin preview` prints. Each one is ready to paste into a shell.

use std::path::Path;

use kg_domain::{OpError, PluginPreview};

use crate::out;

pub(crate) fn started(preview: &PluginPreview) -> Result<(), OpError> {
    out::line(format!(
        "preview {id}: {name} ({plugin}), watching {path}\nexport KG_DATA_DIR={data} KG_SOCKET={socket}\nstop it with: kg plugin preview stop {id}",
        id = preview.id,
        name = preview.name,
        plugin = preview.plugin,
        path = preview.path.display(),
        data = quoted(&preview.data_dir),
        socket = quoted(&preview.socket),
    ))?;
    Ok(())
}

pub(crate) fn listed(previews: &[PluginPreview]) -> Result<(), OpError> {
    if previews.is_empty() {
        out::line("no previews are running")?;
    }
    for preview in previews {
        let thread = preview
            .thread_id
            .as_ref()
            .map_or("none".to_owned(), ToString::to_string);
        out::line(format!(
            "{}  {} ({})  thread {thread}  {}",
            preview.id,
            preview.name,
            preview.plugin,
            preview.path.display()
        ))?;
    }
    Ok(())
}

// Plain words stay as they are. Anything else is single-quoted, so the line pastes as one value.
fn quoted(path: &Path) -> String {
    let text = path.display().to_string();
    let plain = !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte));
    if plain {
        text
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}
