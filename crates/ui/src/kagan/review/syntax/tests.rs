use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::{Hsla, hsla};
use kg_domain::{DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffSide};

use super::{captures, language_of, recorded_name, runs};

fn line_of(kind: DiffLineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        old,
        new,
        text: text.to_owned(),
    }
}

fn view(path: &str, file: Option<&str>, hunks: Vec<DiffHunk>) -> DiffFileView {
    DiffFileView {
        path: path.to_owned(),
        revision: "rev".to_owned(),
        binary: false,
        truncated: false,
        too_large: false,
        hunks,
        file: file.map(str::to_owned),
        added: 1,
        removed: 1,
    }
}

fn assert_token(
    syntax: &super::Syntax,
    side: DiffSide,
    line_number: u32,
    source: &str,
    token: &str,
    name: &str,
) {
    let start = source.find(token).expect("token");
    let at = start + token.len() / 2;
    let found = syntax
        .spans(side, line_number)
        .iter()
        .find(|(range, _)| range.contains(&at));
    assert!(
        found.is_some_and(|(_, found)| found == name || found.starts_with(&format!("{name}."))),
        "at {at} in {source:?}: {:?}",
        syntax.spans(side, line_number)
    );
}

#[test]
fn a_rust_file_colors_keywords_on_both_sides() {
    let file = "fn main() {\n    let value = 1;\n}\n";
    let syntax = captures(
        &view(
            "src/main.rs",
            Some(file),
            vec![DiffHunk {
                old_start: 2,
                new_start: 2,
                lines: vec![
                    line_of(DiffLineKind::Remove, Some(2), None, "    let value = 0;"),
                    line_of(DiffLineKind::Add, None, Some(2), "    let value = 1;"),
                ],
            }],
        ),
        "src/main.rs",
        None,
    );
    assert_token(&syntax, DiffSide::New, 1, "fn main() {", "fn", "keyword");
    assert_token(&syntax, DiffSide::New, 1, "fn main() {", "main", "function");
    assert_token(
        &syntax,
        DiffSide::New,
        2,
        "    let value = 1;",
        "1",
        "constant.builtin",
    );
    assert_token(
        &syntax,
        DiffSide::Old,
        2,
        "    let value = 0;",
        "0",
        "constant.builtin",
    );
}

#[test]
fn a_string_opened_outside_the_hunk_still_colors_the_line() {
    let file = "let value = \"one\nTWO\nthree\";\n";
    let syntax = captures(
        &view(
            "src/main.rs",
            Some(file),
            vec![DiffHunk {
                old_start: 2,
                new_start: 2,
                lines: vec![
                    line_of(DiffLineKind::Remove, Some(2), None, "two"),
                    line_of(DiffLineKind::Add, None, Some(2), "TWO"),
                ],
            }],
        ),
        "src/main.rs",
        None,
    );
    assert_token(&syntax, DiffSide::New, 2, "TWO", "TWO", "string");
    assert_token(&syntax, DiffSide::Old, 2, "two", "two", "string");
}

#[test]
fn a_deleted_file_is_colored_from_its_hunk() {
    let syntax = captures(
        &view(
            "src/lib.rs",
            None,
            vec![DiffHunk {
                old_start: 1,
                new_start: 0,
                lines: vec![line_of(DiffLineKind::Remove, Some(1), None, "fn old() {}")],
            }],
        ),
        "src/lib.rs",
        None,
    );
    assert_token(&syntax, DiffSide::Old, 1, "fn old() {}", "fn", "keyword");
    assert!(syntax.spans(DiffSide::New, 1).is_empty());
}

#[test]
fn a_rename_uses_the_old_language_for_removed_lines() {
    let syntax = captures(
        &view(
            "new.rs",
            Some("fn go() {}\n"),
            vec![DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: vec![
                    line_of(DiffLineKind::Remove, Some(1), None, "function go() {}"),
                    line_of(DiffLineKind::Add, None, Some(1), "fn go() {}"),
                ],
            }],
        ),
        "new.rs",
        Some("old.js"),
    );
    assert_token(
        &syntax,
        DiffSide::Old,
        1,
        "function go() {}",
        "function",
        "keyword",
    );
    assert_token(&syntax, DiffSide::New, 1, "fn go() {}", "fn", "keyword");
}

#[test]
fn an_unknown_file_and_a_long_line_stay_plain() {
    let notes = captures(
        &view(
            "notes.txt",
            Some("fn main() {}\n"),
            vec![DiffHunk {
                old_start: 0,
                new_start: 1,
                lines: vec![line_of(DiffLineKind::Add, None, Some(1), "fn main() {}")],
            }],
        ),
        "directory.rs/README",
        None,
    );
    assert!(notes.spans(DiffSide::New, 1).is_empty());

    let long = format!("fn {}\n", "a".repeat(1000));
    let syntax = captures(
        &view(
            "src/main.rs",
            Some(&long),
            vec![DiffHunk {
                old_start: 0,
                new_start: 1,
                lines: vec![line_of(DiffLineKind::Add, None, Some(1), long.trim_end())],
            }],
        ),
        "src/main.rs",
        None,
    );
    assert!(syntax.spans(DiffSide::New, 1).is_empty());
}

#[test]
fn a_changed_word_keeps_its_tint_over_the_syntax_color() {
    let source = "fn main() {}";
    let syntax = captures(
        &view(
            "src/main.rs",
            Some("fn main() {}\n"),
            vec![DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: vec![
                    line_of(DiffLineKind::Remove, Some(1), None, "fn kept() {}"),
                    line_of(DiffLineKind::Add, None, Some(1), source),
                ],
            }],
        ),
        "src/main.rs",
        None,
    );
    let theme = HighlightTheme::default_dark();
    let keyword = theme.style("keyword").expect("keyword");
    let tint = hsla(0.4, 0.5, 0.5, 1.);
    let painted = runs(
        source,
        syntax.spans(DiffSide::New, 1),
        Some(&(3..7)),
        Some(tint),
        theme.as_ref(),
    );
    let keyword_run = painted
        .iter()
        .find(|(range, _)| range.contains(&0))
        .expect("fn");
    assert_eq!(keyword_run.0, 0..2);
    assert_eq!(keyword_run.1.color, keyword.color);
    assert!(keyword_run.1.background_color.is_none());
    let changed = painted
        .iter()
        .find(|(range, _)| *range == (3..7))
        .expect("main");
    assert_eq!(changed.1.background_color, Some(tint.opacity(0.30)));
    assert_eq!(
        changed.1.color,
        theme.style("function").expect("function").color
    );

    assert!(runs(source, &[], Some(&(0..0)), Some(tint), theme.as_ref()).is_empty());
    assert!(runs("    ", &[], Some(&(0..4)), Some(tint), theme.as_ref()).is_empty());
}

#[test]
fn each_added_line_keeps_its_own_number_and_its_own_text() {
    let syntax = captures(
        &view(
            "src/lib.rs",
            None,
            vec![DiffHunk {
                old_start: 0,
                new_start: 1,
                lines: vec![
                    line_of(DiffLineKind::Add, None, Some(1), "// first"),
                    line_of(DiffLineKind::Add, None, Some(2), "fn second() {}"),
                ],
            }],
        ),
        "src/lib.rs",
        None,
    );
    assert_token(&syntax, DiffSide::New, 2, "fn second() {}", "fn", "keyword");
}

#[test]
fn the_old_side_skips_added_lines_and_counts_context_lines() {
    let syntax = captures(
        &view(
            "src/lib.rs",
            Some("fn added() {}\nfn kept() {}\n"),
            vec![DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: vec![
                    line_of(DiffLineKind::Add, None, Some(1), "fn added() {}"),
                    line_of(DiffLineKind::Context, Some(1), Some(2), "fn kept() {}"),
                ],
            }],
        ),
        "src/lib.rs",
        None,
    );
    assert_token(&syntax, DiffSide::Old, 1, "fn kept() {}", "fn", "keyword");
    assert!(syntax.spans(DiffSide::Old, 2).is_empty());
}

#[test]
fn a_binary_truncated_or_large_file_is_never_colored() {
    for (binary, truncated, too_large) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut file = view("src/lib.rs", Some("fn main() {}\n"), vec![]);
        file.binary = binary;
        file.truncated = truncated;
        file.too_large = too_large;
        let syntax = captures(&file, "src/lib.rs", None);
        assert!(syntax.spans(DiffSide::New, 1).is_empty());
    }
}

#[test]
fn clearing_drops_every_span() {
    let mut syntax = captures(
        &view("src/lib.rs", Some("fn main() {}\n"), vec![]),
        "src/lib.rs",
        None,
    );
    assert_token(&syntax, DiffSide::New, 1, "fn main() {}", "fn", "keyword");
    syntax.clear();
    assert!(syntax.spans(DiffSide::New, 1).is_empty());
}

#[test]
fn only_the_marker_colors_name_a_capture() {
    let names = vec!["keyword".to_owned()];
    let marker = Hsla {
        h: 0.,
        s: 0.,
        l: 0.,
        a: 0.,
    };
    assert_eq!(recorded_name(&names, marker), Some("keyword".to_owned()));
    for color in [
        Hsla {
            h: 0.,
            s: 0.5,
            l: 0.,
            a: 0.,
        },
        Hsla {
            h: 0.,
            s: 0.5,
            l: 0.5,
            a: 0.,
        },
        Hsla {
            h: 0.,
            s: 0.,
            l: 0.,
            a: 0.5,
        },
    ] {
        assert_eq!(recorded_name(&names, color), None);
    }
}

#[test]
fn a_plain_text_file_is_never_colored() {
    let syntax = captures(
        &view("notes.txt", Some("fn main() {}\n"), vec![]),
        "notes.txt",
        None,
    );
    assert!(syntax.spans(DiffSide::New, 1).is_empty());
}

#[test]
fn makefiles_and_cmake_lists_get_their_own_language() {
    assert_eq!(language_of("Makefile"), "make");
    assert_eq!(language_of("src/GNUmakefile"), "make");
    assert_eq!(language_of("CMakeLists.txt"), "cmake");
}

#[test]
fn hunks_longer_than_the_file_do_not_panic() {
    let hunks = vec![
        DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                line_of(DiffLineKind::Add, None, Some(1), "a"),
                line_of(DiffLineKind::Add, None, Some(2), "b"),
                line_of(DiffLineKind::Add, None, Some(3), "c"),
            ],
        },
        DiffHunk {
            old_start: 1,
            new_start: 10,
            lines: vec![line_of(DiffLineKind::Context, Some(1), Some(10), "z")],
        },
    ];
    captures(&view("src/lib.rs", Some("a\n"), hunks), "src/lib.rs", None);
}
