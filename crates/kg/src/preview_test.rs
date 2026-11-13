use std::path::Path;

use kg_domain::{PluginId, PluginPreview, PreviewId};

use super::{quoted, started_text};

fn preview(data: &str) -> PluginPreview {
    PluginPreview {
        id: PreviewId("p-3f2a9c".into()),
        plugin: PluginId("dev.kagan.panel".into()),
        name: "Panel".into(),
        thread_id: None,
        path: "/work/panel".into(),
        data_dir: data.into(),
        socket: format!("{data}/kg.sock").into(),
    }
}

#[test]
fn a_plain_path_is_not_quoted() {
    assert_eq!(quoted(Path::new("/tmp/kagan/data")), "/tmp/kagan/data");
}

#[test]
fn a_path_with_a_space_or_a_quote_is_quoted_as_one_word() {
    assert_eq!(
        quoted(Path::new("/tmp/my kagan/it's")),
        r"'/tmp/my kagan/it'\''s'"
    );
}

#[test]
fn the_started_lines_put_the_variables_in_front_of_one_command() {
    let text = started_text(&preview("/tmp/my kagan/data"));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert_eq!(
        lines[0],
        "preview p-3f2a9c: Panel (dev.kagan.panel), watching /work/panel"
    );
    assert!(
        lines[1].starts_with("drive it:  KG_DATA_DIR='/tmp/my kagan/data' KG_SOCKET='/tmp/my kagan/data/kg.sock' kg ui snapshot"),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2], "stop it:   kg plugin preview stop p-3f2a9c");
    assert!(
        !text.contains("export "),
        "an export would leak into the next command: {text}"
    );
}
