use std::path::Path;

const INSTRUCTIONS_CAP: usize = 4096;
const INSTRUCTIONS_PREFIX: &str = "Instructions from your kagan instructions file:";

pub(crate) fn read_instructions(data_dir: &Path) -> String {
    let Ok(bytes) = std::fs::read(data_dir.join("instructions.md")) else {
        return String::new();
    };
    let end = char_boundary(&bytes, bytes.len().min(INSTRUCTIONS_CAP));
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
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

// A continuation byte is 10xxxxxx, so a cut there is mid-character and the leading byte is earlier.
fn char_boundary(bytes: &[u8], mut end: usize) -> usize {
    if end >= bytes.len() {
        return bytes.len();
    }
    while end > 0 && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    end
}

fn introduction() -> &'static str {
    "You are working inside kagan, a workspace for programs in threads. This thread's id is the environment variable KG_THREAD_ID. `kg` is on PATH. Run `kg status --json` for this thread, its project, and its environment. Run `kg guide` for the map.\n\
     \n\
     A helper, reviewer, or parallel worker is another kagan thread: `kg thread run` with `--parent-thread-id` set to KG_THREAD_ID, then `kg thread paste` from a file. Task, Agent, and this program's subagent tool are the wrong tool. One task stays in this thread. `kg thread wait` and `kg thread wait-event` block until the child is done. `kg thread ask` asks the person. Before the first child, run `kg skill show kg-threads`. `kg-cli` is the command map. `kg-ui` is the window.\n\
     \n\
     Plugin powers and a delete are approved by the person in the app. `kg ui` cannot approve them. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays."
}
