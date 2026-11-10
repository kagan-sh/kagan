use std::path::Path;

pub(crate) const INSTRUCTIONS_CAP: usize = 4096;
pub(crate) const INSTRUCTIONS_PREFIX: &str = "Instructions from your kagan instructions file:";

pub(crate) fn read_instructions(data_dir: &Path) -> String {
    let Some(bytes) = read_prefix(data_dir) else {
        return String::new();
    };
    let end = if bytes.len() < INSTRUCTIONS_CAP {
        bytes.len()
    } else {
        complete_prefix(&bytes)
    };
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn read_prefix(data_dir: &Path) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(data_dir.join("instructions.md")).ok()?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1024];
    while bytes.len() < INSTRUCTIONS_CAP {
        let want = (INSTRUCTIONS_CAP - bytes.len()).min(buf.len());
        let n = std::io::Read::read(&mut file, &mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    Some(bytes)
}

pub(crate) fn note_with(id: &str, instructions: &str, extra: &str) -> String {
    let mut text = format!(
        "{}\n\nYou are running inside kagan thread {id}.",
        introduction()
    );
    append_instructions(&mut text, instructions);
    append_extra(&mut text, extra);
    text
}

pub(crate) fn cursor_rule(instructions: &str, extra: &str) -> String {
    let mut body = introduction().to_owned();
    append_instructions(&mut body, instructions);
    append_extra(&mut body, extra);
    format!("---\nalwaysApply: true\n---\n{body}\n")
}

fn append_instructions(text: &mut String, instructions: &str) {
    let instructions = instructions.trim();
    if instructions.is_empty() {
        return;
    }
    text.push_str("\n\n");
    text.push_str(INSTRUCTIONS_PREFIX);
    text.push('\n');
    text.push_str(instructions);
}

fn append_extra(text: &mut String, extra: &str) {
    if extra.is_empty() {
        return;
    }
    text.push_str("\n\n");
    text.push_str(extra);
}

// A continuation byte is 10xxxxxx. A capped read can end on one, so the leading byte is dropped.
fn complete_prefix(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let mut index = bytes.len() - 1;
    let mut continuations = 0;
    while continuations < 3 && (bytes[index] & 0b1100_0000) == 0b1000_0000 {
        if index == 0 {
            return 0;
        }
        index -= 1;
        continuations += 1;
    }
    let width = match bytes[index] {
        byte if byte & 0b1000_0000 == 0 => 1,
        byte if byte & 0b1110_0000 == 0b1100_0000 => 2,
        byte if byte & 0b1111_0000 == 0b1110_0000 => 3,
        byte if byte & 0b1111_1000 == 0b1111_0000 => 4,
        _ => 1,
    };
    if bytes.len() - index < width {
        index
    } else {
        bytes.len()
    }
}

fn introduction() -> &'static str {
    "You are working inside kagan, a workspace for programs in threads. This thread's id is the environment variable KG_THREAD_ID. `kg` is on PATH. Run `kg status --json` for this thread, its project, and its environment. Run `kg guide` for the map.\n\
     \n\
     A helper, reviewer, or parallel worker is another kagan thread: `kg thread run` with `--parent-thread-id` set to KG_THREAD_ID, then `kg thread paste` from a file. Task, Agent, and this program's subagent tool are the wrong tool. One task stays in this thread. `kg thread wait` and `kg thread wait-event` block until the child is done. `kg thread ask` asks the person. Before the first child, run `kg skill show kg-threads`. `kg-cli` is the command map. `kg-ui` is the window.\n\
     \n\
     Plugin powers and a delete are approved by the person in the app. `kg ui` cannot approve them. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays."
}
