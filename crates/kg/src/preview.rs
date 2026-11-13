//! The lines `kg plugin preview` prints. Each command is ready to paste into a shell.

use std::path::Path;

use kg_domain::{Op, OpError, PluginOp, PluginPreview, PluginPreviewOp};

use crate::out;
use crate::socket::Client;

pub(crate) fn started(preview: &PluginPreview) -> Result<(), OpError> {
    out::line(started_text(preview))?;
    Ok(())
}

// A start asks the person, so the wait is said just before that question: once the folder checks
// as a plugin. A start that fails earlier says nothing about waiting.
pub(crate) fn check_start(client: &mut Client, op: &Op) -> Result<(), OpError> {
    let Op::Plugin(PluginOp::Preview {
        op: PluginPreviewOp::Start { path, .. },
    }) = op
    else {
        return Ok(());
    };
    client.call(&Op::Plugin(PluginOp::Check { path: path.clone() }))?;
    eprintln!("waiting for approval in your kagan window…");
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
