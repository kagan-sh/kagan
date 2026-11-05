use clap::CommandFactory;
use clap::builder::styling::Styles;

use crate::Cli;

#[test]
fn guide_names_every_command() {
    fn check(command: &clap::Command, path: &str, guide: &str) {
        let children: Vec<_> = command
            .get_subcommands()
            .filter(|child| child.get_name() != "help")
            .collect();
        if children.is_empty() && path != "kg" {
            assert!(guide.contains(&format!("`{path}`")), "missing {path}");
        }
        for child in children {
            check(child, &format!("{path} {}", child.get_name()), guide);
        }
    }
    check(
        &Cli::command(),
        "kg",
        include_str!("../../../core/src/dispatch/guide-map.md"),
    );
}

#[test]
fn cli_reference_matches_help() {
    let mut root = Cli::command().styles(Styles::plain()).term_width(100);
    root.set_bin_name("kg");
    let mut discard = Vec::new();
    root.write_long_help(&mut discard).expect("help");

    let mut out = String::from(
        "# `kg`\n\nThis page is generated from `kg --help`.\n\nEvery command accepts `--json`. A failure prints the error and exits non-zero. `kg mcp` is how the agent connects. You rarely type it.\n\n",
    );
    append_help(&root, "kg", &mut out);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../book/src/cli.md");
    if std::env::var_os("KG_UPDATE_CLI").is_some() {
        std::fs::write(&path, &out).expect("write cli.md");
    }
    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        on_disk, out,
        "book/src/cli.md does not match clap help; rerun with KG_UPDATE_CLI=1"
    );
}

fn append_help(cmd: &clap::Command, path: &str, out: &mut String) {
    let mut built = cmd.clone().styles(Styles::plain()).term_width(100);
    built.set_bin_name(path);
    let mut buf = Vec::new();
    built.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf-8").replace("\r\n", "\n");
    out.push_str("## `");
    out.push_str(path);
    out.push_str("`\n\n```text\n");
    out.push_str(help.trim_end());
    out.push_str("\n```\n\n");
    if cmd.get_name() == "help" {
        return;
    }
    if path == "kg"
        && let Some(help) = cmd.find_subcommand("help")
    {
        append_help(help, "kg help", out);
    }
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        append_help(sub, &format!("{path} {}", sub.get_name()), out);
    }
}
