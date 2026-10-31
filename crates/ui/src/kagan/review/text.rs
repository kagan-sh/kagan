use kg_domain::{DiffFile, DiffFileView, DiffLineKind, DiffStatus};

pub(super) fn layout(view: &DiffFileView) -> (String, Vec<u32>) {
    let mut rows: Vec<String> = Vec::new();
    let mut starts: Vec<u32> = Vec::new();
    for hunk in &view.hunks {
        starts.push(u32::try_from(rows.len()).unwrap_or(u32::MAX));
        let old = hunk
            .lines
            .iter()
            .filter(|line| line.kind != DiffLineKind::Add)
            .count();
        let new = hunk
            .lines
            .iter()
            .filter(|line| line.kind != DiffLineKind::Remove)
            .count();
        rows.push(format!(
            "@@ -{},{old} +{},{new} @@",
            hunk.old_start, hunk.new_start
        ));
        for line in &hunk.lines {
            rows.push(format!(
                "{} {} {} {}",
                sign(line.kind),
                number(line.old),
                number(line.new),
                line.text
            ));
        }
    }
    (rows.join("\n"), starts)
}

pub(super) fn sign(kind: DiffLineKind) -> char {
    match kind {
        DiffLineKind::Add => '+',
        DiffLineKind::Remove => '-',
        DiffLineKind::Context => ' ',
    }
}

pub(super) fn number(line: Option<u32>) -> String {
    line.map_or_else(|| " ".repeat(5), |line| format!("{line:>5}"))
}

pub(super) fn renamed_from(file: &DiffFile) -> Option<String> {
    (file.status == DiffStatus::Renamed)
        .then(|| file.old_path.clone())
        .flatten()
}

pub(super) fn notice(view: &DiffFileView) -> Option<String> {
    if view.binary {
        Some("This file is binary. Its text is not shown.".to_owned())
    } else if view.too_large {
        Some("This file is too large to show.".to_owned())
    } else if view.truncated {
        Some("This diff is too large to show.".to_owned())
    } else if view.hunks.is_empty() {
        Some("No line changes.".to_owned())
    } else {
        None
    }
}
