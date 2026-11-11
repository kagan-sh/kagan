use kg_domain::{Program, ProgramSource};

use super::marked_program;

fn program(label: &str, command: &str, source: ProgramSource) -> Program {
    Program {
        command: command.into(),
        label: label.into(),
        source,
        path: None,
        version: None,
        pinned: false,
    }
}

#[test]
fn a_line_that_several_rows_run_marks_only_the_first_of_them() {
    let programs = [
        program("Claude Code", "claude", ProgramSource::Known),
        program("Careful", "claude", ProgramSource::Saved),
        program("Quick", "codex", ProgramSource::Saved),
    ];
    assert_eq!(
        marked_program(&programs, Some(&"claude".to_owned())),
        Some(0)
    );
    assert_eq!(
        marked_program(&programs, Some(&"codex".to_owned())),
        Some(2)
    );
    assert_eq!(marked_program(&programs, Some(&"gemini".to_owned())), None);
    assert_eq!(marked_program(&programs, None), None);
}
