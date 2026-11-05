use std::path::PathBuf;

use kg_domain::{ProjectId, ProjectKind};

use super::marks::row_tip;
use super::{Kind, Row};

fn project(kind: ProjectKind) -> Row {
    Row {
        kind: Kind::Project {
            id: ProjectId("personal".into()),
            name: "Personal".into(),
            path: PathBuf::new(),
            open: true,
            kind,
        },
        label: "Personal".into(),
        detail: None,
        selected: false,
        open: true,
    }
}

#[test]
fn the_personal_header_says_what_it_holds() {
    assert_eq!(
        row_tip(&project(ProjectKind::Personal)).as_deref(),
        Some("Threads that are not in a project. Each one gets its own folder.")
    );
    assert_eq!(row_tip(&project(ProjectKind::Standard)), None);
}
