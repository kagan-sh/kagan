use super::{Fixture, op, program};
use gpui_kit::TestAppContext;
use kg_domain::{
    Op, OpError, OpResult, PreviewFile, PreviewImageFormat, PreviewKind, PreviewOp, ThreadId,
};

async fn started(fixture: &Fixture, cx: &mut TestAppContext) -> ThreadId {
    let OpResult::Thread(thread) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    thread.id
}

async fn preview(
    thread: Option<&ThreadId>,
    path: &str,
    cx: &mut TestAppContext,
) -> Result<PreviewFile, OpError> {
    match op(
        Op::Preview(PreviewOp::Show {
            thread_id: thread.cloned(),
            path: path.into(),
        }),
        cx,
    )
    .await?
    {
        OpResult::Preview(file) => Ok(file),
        _ => panic!("preview"),
    }
}

#[gpui_kit::test]
async fn a_socket_preview_asks_the_window_for_its_tab_and_a_plugin_preview_does_not(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("notes.txt"), "one\n").unwrap();
    let thread = started(&fixture, cx).await;
    let jobs = std::rc::Rc::new(std::cell::RefCell::new(Vec::<crate::UiJob>::new()));
    let sink = jobs.clone();
    cx.update(|cx| {
        fixture.workspace.update(cx, |workspace, _| {
            workspace.set_ui_driver(std::rc::Rc::new(
                move |job: crate::UiJob, _: &mut gpui_kit::App| {
                    sink.borrow_mut().push(job);
                    gpui_kit::Task::ready(Ok::<_, OpError>(OpResult::Done))
                },
            ));
        });
    });
    let (_gone, gone) = async_channel::bounded::<()>(1);
    let socket = Op::Preview(PreviewOp::Show {
        thread_id: Some(thread.clone()),
        path: "notes.txt".into(),
    });
    cx.update(|cx| crate::Workspace::dispatch_for(socket, Some(crate::Requester::new(gone)), cx))
        .await
        .unwrap();
    let shown = jobs.borrow().clone();
    assert!(
        matches!(
            shown.as_slice(),
            [crate::UiJob::Preview { thread: Some(id), path }] if *id == thread && path == "notes.txt"
        ),
        "{shown:?}"
    );
    preview(Some(&thread), "notes.txt", cx).await.unwrap();
    assert_eq!(jobs.borrow().len(), 1, "a plugin read shows no tab");
}

#[gpui_kit::test]
async fn a_relative_path_inside_the_environment_reads_as_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("notes.txt"), "one\ntwo\n").unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "notes.txt", cx).await.unwrap();
    assert_eq!(file.path, "notes.txt");
    assert_eq!(file.bytes, 8);
    let PreviewKind::Text { text, cut } = file.kind else {
        panic!("text")
    };
    assert_eq!(text, "one\ntwo");
    assert!(cut.is_none());
}

#[gpui_kit::test]
async fn a_path_outside_the_thread_folder_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "no\n").unwrap();
    let thread = started(&fixture, cx).await;
    let absolute = outside.path().join("secret.txt");
    let error = preview(Some(&thread), &absolute.display().to_string(), cx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("outside this thread's folder"));
    let sibling = outside.path().file_name().unwrap().to_string_lossy();
    let error = preview(Some(&thread), &format!("../{sibling}/secret.txt"), cx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("outside this thread's folder"));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_symlink_out_of_the_folder_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "no\n").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        fixture.dir.path().join("link"),
    )
    .unwrap();
    let thread = started(&fixture, cx).await;
    let error = preview(Some(&thread), "link", cx).await.unwrap_err();
    assert!(error.to_string().contains("outside this thread's folder"));
}

#[gpui_kit::test]
async fn a_relative_path_without_a_thread_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("notes.txt"), "one\n").unwrap();
    let error = preview(None, "notes.txt", cx).await.unwrap_err();
    assert!(error.to_string().contains("absolute path"));
    let file = preview(
        None,
        &fixture.dir.path().join("notes.txt").display().to_string(),
        cx,
    )
    .await
    .unwrap();
    assert!(matches!(file.kind, PreviewKind::Text { .. }));
}

#[gpui_kit::test]
async fn a_folder_a_missing_file_and_an_empty_file_say_why(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::create_dir(fixture.dir.path().join("folder")).unwrap();
    std::fs::write(fixture.dir.path().join("empty.txt"), "").unwrap();
    let thread = started(&fixture, cx).await;
    assert!(
        preview(Some(&thread), "folder", cx)
            .await
            .unwrap_err()
            .to_string()
            .contains("is a folder")
    );
    assert!(
        preview(Some(&thread), "missing.txt", cx)
            .await
            .unwrap_err()
            .to_string()
            .contains("no file")
    );
    let file = preview(Some(&thread), "empty.txt", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Unavailable {
            reason: "Empty file.".into()
        }
    );
}

#[gpui_kit::test]
async fn a_file_over_one_megabyte_shows_no_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(
        fixture.dir.path().join("big.txt"),
        vec![b'a'; 1024 * 1024 + 1],
    )
    .unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "big.txt", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Unavailable {
            reason: "1.0 MB. Preview shows text up to 1 MB.".into()
        }
    );
}

#[gpui_kit::test]
async fn long_text_keeps_two_thousand_lines_and_cuts_a_long_line(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let text: String = (0..2_001)
        .map(|n| format!("line {n}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(fixture.dir.path().join("many.txt"), text).unwrap();
    let long = format!("{}tail\n", "x".repeat(4_001));
    std::fs::write(fixture.dir.path().join("long.txt"), long).unwrap();
    let thread = started(&fixture, cx).await;
    let PreviewKind::Text { text, cut } =
        preview(Some(&thread), "many.txt", cx).await.unwrap().kind
    else {
        panic!("text")
    };
    assert_eq!(text.lines().count(), 2_000);
    assert_eq!(cut.unwrap().total, 2_001);
    let PreviewKind::Text { text, .. } = preview(Some(&thread), "long.txt", cx).await.unwrap().kind
    else {
        panic!("text")
    };
    assert_eq!(text.len(), 4_000);
}

#[gpui_kit::test]
async fn a_nul_byte_and_invalid_utf8_are_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("binary.txt"), b"a\0b").unwrap();
    std::fs::write(fixture.dir.path().join("bad.txt"), [0xff, 0xfe, 0x41]).unwrap();
    let thread = started(&fixture, cx).await;
    let nul = preview(Some(&thread), "binary.txt", cx).await.unwrap();
    assert!(matches!(nul.kind, PreviewKind::Unavailable { .. }));
    let bad = preview(Some(&thread), "bad.txt", cx).await.unwrap();
    assert_eq!(
        bad.kind,
        PreviewKind::Unavailable {
            reason: "Not UTF-8. kagan does not guess an encoding.".into()
        }
    );
}

#[gpui_kit::test]
async fn a_bom_is_dropped_and_crlf_becomes_lf(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("bom.txt"), "\u{feff}one\r\ntwo\r\n").unwrap();
    let thread = started(&fixture, cx).await;
    let PreviewKind::Text { text, .. } = preview(Some(&thread), "bom.txt", cx).await.unwrap().kind
    else {
        panic!("text")
    };
    assert_eq!(text, "one\ntwo");
}

#[gpui_kit::test]
async fn markdown_is_chosen_by_extension(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("readme.md"), "# Title\n").unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "readme.md", cx).await.unwrap();
    assert!(matches!(file.kind, PreviewKind::Markdown { .. }));
}

#[gpui_kit::test]
async fn csv_quotes_embedded_commas_newlines_and_ragged_rows(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let csv = "name,note\n\"a,b\",\"say \"\"hi\"\"\"\n\"line\nbreak\",x,extra\n";
    std::fs::write(fixture.dir.path().join("data.csv"), csv).unwrap();
    std::fs::write(fixture.dir.path().join("data.tsv"), "a\tb\n1\t2\n").unwrap();
    let thread = started(&fixture, cx).await;
    let PreviewKind::Csv {
        rows,
        rows_total,
        columns_total,
        truncated,
    } = preview(Some(&thread), "data.csv", cx).await.unwrap().kind
    else {
        panic!("csv")
    };
    assert_eq!(rows_total, 3);
    assert_eq!(rows[0], vec!["name", "note"]);
    assert_eq!(rows[1], vec!["a,b", "say \"hi\""]);
    assert_eq!(rows[2], vec!["line break", "x", "extra"]);
    assert_eq!(columns_total, 3);
    assert!(!truncated);
    let PreviewKind::Csv { rows, .. } = preview(Some(&thread), "data.tsv", cx).await.unwrap().kind
    else {
        panic!("csv")
    };
    assert_eq!(rows[1], vec!["1", "2"]);
}

#[gpui_kit::test]
async fn csv_stops_at_one_hundred_rows_and_twenty_columns(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let mut csv = String::from("header\n");
    for row in 0..150 {
        csv.push_str(&row.to_string());
        csv.push('\n');
    }
    std::fs::write(fixture.dir.path().join("rows.csv"), csv).unwrap();
    let wide: String = (0..25).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    std::fs::write(fixture.dir.path().join("wide.csv"), format!("{wide}\n")).unwrap();
    let thread = started(&fixture, cx).await;
    let PreviewKind::Csv {
        rows,
        rows_total,
        truncated,
        ..
    } = preview(Some(&thread), "rows.csv", cx).await.unwrap().kind
    else {
        panic!("csv")
    };
    assert_eq!(rows.len(), 100);
    assert_eq!(rows_total, 151);
    assert!(truncated);
    let PreviewKind::Csv {
        rows,
        columns_total,
        truncated,
        ..
    } = preview(Some(&thread), "wide.csv", cx).await.unwrap().kind
    else {
        panic!("csv")
    };
    assert_eq!(rows[0].len(), 20);
    assert_eq!(columns_total, 25);
    assert!(truncated);
}

#[gpui_kit::test]
async fn text_exactly_at_each_cap_is_accepted(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(
        fixture.dir.path().join("exact.txt"),
        vec![b'a'; 1024 * 1024],
    )
    .unwrap();
    let lines: String = (0..2_000)
        .map(|n| format!("line {n}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(fixture.dir.path().join("lines.txt"), lines).unwrap();
    std::fs::write(
        fixture.dir.path().join("line.txt"),
        format!("{}\ntail", "x".repeat(4_000)),
    )
    .unwrap();
    let thread = started(&fixture, cx).await;

    let PreviewKind::Text { text, cut } =
        preview(Some(&thread), "exact.txt", cx).await.unwrap().kind
    else {
        panic!("text at the cap")
    };
    assert_eq!(text.len(), 4_000);
    assert!(cut.is_none());

    let PreviewKind::Text { text, cut } =
        preview(Some(&thread), "lines.txt", cx).await.unwrap().kind
    else {
        panic!("2,000 lines at the cap")
    };
    assert_eq!(text.lines().count(), 2_000);
    assert!(cut.is_none());

    let PreviewKind::Text { text, cut } =
        preview(Some(&thread), "line.txt", cx).await.unwrap().kind
    else {
        panic!("4,000 bytes at the cap")
    };
    assert_eq!(text.len(), 4_005);
    assert_eq!(text.lines().next().map(str::len), Some(4_000));
    assert_eq!(text.lines().nth(1), Some("tail"));
    assert!(cut.is_none());
}

#[gpui_kit::test]
async fn csv_and_image_exactly_at_the_caps_are_accepted(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let mut csv = String::new();
    for row in 0..100 {
        let cells: Vec<String> = (0..20)
            .map(|column| {
                if row == 0 && column == 0 {
                    "c".repeat(120)
                } else {
                    format!("{row}-{column}")
                }
            })
            .collect();
        csv.push_str(&cells.join(","));
        csv.push('\n');
    }
    std::fs::write(fixture.dir.path().join("exact.csv"), csv).unwrap();
    let mut heavy = png(2, 2);
    heavy.resize(8 * 1024 * 1024, 0);
    std::fs::write(fixture.dir.path().join("exact.png"), heavy).unwrap();
    std::fs::write(fixture.dir.path().join("many.png"), png(8_000, 5_000)).unwrap();
    let thread = started(&fixture, cx).await;

    let PreviewKind::Csv {
        rows,
        rows_total,
        columns_total,
        truncated,
    } = preview(Some(&thread), "exact.csv", cx).await.unwrap().kind
    else {
        panic!("csv at the caps")
    };
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0].len(), 20);
    assert_eq!(rows[0][0].chars().count(), 120);
    assert_eq!((rows_total, columns_total), (100, 20));
    assert!(!truncated);

    let file = preview(Some(&thread), "exact.png", cx).await.unwrap();
    assert!(matches!(file.kind, PreviewKind::Image { .. }));
    let file = preview(Some(&thread), "many.png", cx).await.unwrap();
    assert!(matches!(file.kind, PreviewKind::Image { .. }));
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend(chunk(*b"IHDR", &ihdr));
    bytes.extend(chunk(*b"IDAT", &[]));
    bytes.extend(chunk(*b"IEND", &[]));
    bytes
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut input = Vec::from(kind);
    input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&input).to_be_bytes());
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([0, 0, 0]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    bytes
}

#[gpui_kit::test]
async fn png_and_jpeg_are_found_by_their_bytes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("pixel.png"), png(1440, 900)).unwrap();
    std::fs::write(fixture.dir.path().join("renamed.txt"), png(32, 16)).unwrap();
    std::fs::write(fixture.dir.path().join("photo.jpg"), jpeg(800, 600)).unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "pixel.png", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Image {
            format: PreviewImageFormat::Png,
            width: 1440,
            height: 900
        }
    );
    assert!(file.absolute.ends_with("pixel.png"));
    let file = preview(Some(&thread), "renamed.txt", cx).await.unwrap();
    assert!(matches!(file.kind, PreviewKind::Image { .. }));
    let file = preview(Some(&thread), "photo.jpg", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Image {
            format: PreviewImageFormat::Jpeg,
            width: 800,
            height: 600
        }
    );
}

#[gpui_kit::test]
async fn an_image_past_the_caps_says_why(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("huge.png"), png(10_000, 9_000)).unwrap();
    let mut big = png(10, 10);
    big.resize(8 * 1024 * 1024 + 1, 0);
    std::fs::write(fixture.dir.path().join("heavy.png"), big).unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "huge.png", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Unavailable {
            reason: "10000×9000 is too many pixels to preview.".into()
        }
    );
    let file = preview(Some(&thread), "heavy.png", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Unavailable {
            reason: "8.0 MB. Preview shows images up to 8 MB.".into()
        }
    );
}

#[gpui_kit::test]
async fn a_pdf_says_it_waits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(fixture.dir.path().join("paper.pdf"), "%PDF-1.7\n").unwrap();
    let thread = started(&fixture, cx).await;
    let file = preview(Some(&thread), "paper.pdf", cx).await.unwrap();
    assert_eq!(
        file.kind,
        PreviewKind::Unavailable {
            reason: "PDF is not previewed yet.".into()
        }
    );
}

#[test]
fn check_refuses_an_empty_and_a_control_character_path() {
    for path in ["", "a\nb"] {
        let op = Op::Preview(PreviewOp::Show {
            thread_id: None,
            path: path.into(),
        });
        assert!(kg_domain::check(&op).is_err());
    }
    let op = Op::Preview(PreviewOp::Show {
        thread_id: None,
        path: "notes.txt".into(),
    });
    assert!(kg_domain::check(&op).is_ok());
}

#[test]
fn the_cut_note_names_only_the_dimension_that_was_cut() {
    let csv = |rows: Vec<Vec<&str>>, rows_total: usize, columns_total: usize| PreviewKind::Csv {
        rows: rows
            .into_iter()
            .map(|row| row.into_iter().map(str::to_owned).collect())
            .collect(),
        rows_total,
        columns_total,
        truncated: true,
    };
    assert_eq!(
        crate::preview_note(&csv(vec![vec!["a"]; 100], 151, 1)).as_deref(),
        Some("Showing the first 100 of 151 rows.")
    );
    assert_eq!(
        crate::preview_note(&csv(vec![vec!["a"; 20]; 100], 100, 30)).as_deref(),
        Some("Showing the first 20 of 30 columns.")
    );
    assert_eq!(
        crate::preview_note(&csv(vec![vec!["a"], vec!["a", "b", "c"]], 2, 3)),
        None
    );
    let cut = PreviewKind::Text {
        text: "one".into(),
        cut: Some(kg_domain::PreviewCut {
            shown: 2_000,
            total: 2_001,
        }),
    };
    assert_eq!(
        crate::preview_note(&cut).as_deref(),
        Some("Showing the first 2,000 of 2,001 lines.")
    );
    assert_eq!(
        crate::preview_note(&csv(vec![vec!["a"; 20]; 100], 151, 30)).as_deref(),
        Some("Showing the first 100 of 151 rows. Showing the first 20 of 30 columns.")
    );
}
