//! The lines `kg plugin preview` prints. Each command is ready to paste into a shell.

use std::path::Path;

use kg_domain::{Op, OpError, PluginOp, PluginPreview, PluginPreviewOp};

use crate::out;

// A start blocks until the person answers its question, so the wait is said on stderr first.
pub(crate) fn say_waiting(op: &Op) {
    if matches!(
        op,
        Op::Plugin(PluginOp::Preview {
            op: PluginPreviewOp::Start { .. }
        })
    ) {
        eprintln!("waiting for approval in your kagan window…");
    }
}

pub(crate) fn started(preview: &PluginPreview) -> Result<(), OpError> {
    out::line(started_text(preview))?;
    Ok(())
}

// The variables go in front of one command. Nothing is exported, so the stop line and install
// run in the person's own shell, and a paste cannot send them to the preview.
pub(crate) fn started_text(preview: &PluginPreview) -> String {
    format!(
        "preview {id}: {name} ({plugin}), watching {path}\ndrive it:  KG_DATA_DIR={data} KG_SOCKET={socket} kg ui snapshot\nstop it:   kg plugin preview stop {id}",
        id = preview.id,
        name = preview.name,
        plugin = preview.plugin,
        path = preview.path.display(),
        data = quoted(&preview.data_dir),
        socket = quoted(&preview.socket),
    )
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
pub(crate) fn quoted(path: &Path) -> String {
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

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
