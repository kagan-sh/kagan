use kg_domain::{GuideChapter, OpResult};

pub(crate) fn read(chapter: Option<GuideChapter>) -> OpResult {
    let text = match chapter {
        None => {
            return OpResult::Text(format!(
                "{}\n\nChapters: threads, projects, environments, plugins, skills, ui, json\nRead one with kg guide <chapter>.\n\n{}",
                include_str!("../../../docs/CONTEXT.md"),
                include_str!("guide-map.md")
            ));
        }
        Some(GuideChapter::Threads) => {
            "A thread is one program running in an environment. KG_THREAD_ID defaults thread arguments. Use kg thread run --program <command>, get [id], events [id], stop [id], archive [id]. Plugin powers are approved only by a person in the app. Old threads still open; their saved events stay in the log. A success does not notify the parent. `waiting` is the title going plain after a spinner. `idle` is the row, once the program has ended or been stopped. `kg thread wait` returns `waiting`, `exited` (the row is idle, including after Stop), `failed`, `not-busy`, or `timeout`. `kg thread paste --file` reads that file, or stdin when the path is `-`, up to 1 MiB, and sends the text. `kg thread ask` raises a question, and the answer arrives as `answered`. The question shows in Questions, with approvals first. `kg thread start-again` runs that program again in the same folder. `kg thread screen --scrollback` prints the kept lines; `--lines` needs `--scrollback` and is 1 to 10000. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays. `kg diagnostics export` writes a local file with no terminal contents and no environment variables. When a thread goes from busy to waiting while it is not on screen, the app posts a system notification with the thread title, unless `kg preference set notify false`."
        }
        Some(GuideChapter::Projects) => {
            "A project is a named body of work with one folder. The personal project holds work tied to no repository. Use kg project list, kg project get [id], and kg project add <folder>. KG_PROJECT_ID supplies the default id. Use kg thread run --project-id <id> to choose a project. kg project list --json returns {\"projects\":[{\"id\",\"kind\",\"name\",\"path\",\"gitRemoteUrl\"}]}."
        }
        Some(GuideChapter::Environments) => {
            "An environment is the folder a thread works in. Use kg environment get [id]; KG_ENVIRONMENT_ID supplies the default. Run --environment takes project-default, personal, worktree[:<base>[:<branch>]] or reuse:<id>. The branch defaults to a name from the thread title. A new worktree branch is created with --no-track and does not track an upstream. Kagan does not fetch and does not set an upstream later. The only scripts are .kagan/setup and .kagan/teardown, from the pinned base commit. Files a setup script creates should be git-ignored, or removal stops and kg environment get names them. kg environment remove tries that removal again. A project folder and a personal folder run neither. Managed environments retire after their last unarchived thread; unmanaged folders are kept. Use kg environment restore <thread-id> to restore a destroyed managed environment. kg environment branches and set-base read git. They never post or merge."
        }
        Some(GuideChapter::Plugins) => {
            return OpResult::Text(format!(
                "{}\n\n{}",
                include_str!("../../../docs/plugin-api.md"),
                include_str!("../../../docs/self-extend.md")
            ));
        }
        Some(GuideChapter::Skills) => {
            "A skill is a folder with SKILL.md: YAML frontmatter name (lowercase, hyphenated, matching the folder), description, and an optional one-line argument-hint, followed by instructions. Supporting files live beside it. Sources override in order: bundled, enabled plugin manifest skills, <dataDir>/skills, <project>/.kagan/skills. Duplicate names within one source are dropped and reported. Use kg skill list, show <name>, files <name>, new <name> [--project], add <path> [--project], remove <name>. Project context defaults to KG_PROJECT_ID. Only user and project skills can be removed. The tool list names each skill and its description. The agent loads instructions with the skill_load tool."
        }
        Some(GuideChapter::Ui) => {
            return OpResult::Text(include_str!("../../../docs/ui-driver.md").into());
        }
        Some(GuideChapter::Json) => {
            "Every command accepts --json. Results are the serialized OpResult for the running version. Failures return {\"error\":{\"kind\":\"not-found|invalid|failed\",\"message\":\"...\"}} and a nonzero exit code. `hint` may be present. `kg thread paste --file` reads the file in the client and sends the text; a payload that still carries a path is invalid. kg status --json returns {\"status\":{\"threadId\",\"projectId\",\"environmentId\",\"cli\",\"dataDir\",\"warnings\"}}. cli is the absolute path of the kg that is serving this command. Leave KG_DATA_DIR unset. In a thread KG_SOCKET is the open app's socket already; outside one, leave it unset so you reach the open app. If the socket does not answer, the command starts a second headless app, and you should stop. On macOS that socket is ~/Library/Application Support/kagan/kg.sock, and the binary in /Applications is /Applications/kagan.app/Contents/MacOS/kg. That macOS build is unsigned and not notarized unless the Apple signing secrets are set. It does not add itself to PATH. A kg mcp process is not the app. --help is generated from the same typed operations that the app dispatches."
        }
    };
    OpResult::Text(text.into())
}

pub(crate) async fn status(
    this: &gpui_kit::WeakEntity<crate::Workspace>,
    thread_id: Option<kg_domain::ThreadId>,
    project_id: Option<kg_domain::ProjectId>,
    environment_id: Option<kg_domain::EnvironmentId>,
    cx: &mut gpui_kit::AsyncApp,
) -> crate::database::Result<OpResult> {
    let (cli, data_dir) = this
        .update(cx, |workspace, _| {
            (workspace.cli.clone(), workspace.data_dir.clone())
        })
        .map_err(crate::workspace::closed)?;
    let OpResult::Warnings(warnings) =
        crate::thread::forward(this, crate::PluginRequest::Warnings, cx).await?
    else {
        return Err(kg_domain::OpError::Failed(
            "plugin host returned no warnings".into(),
        ));
    };
    Ok(OpResult::Status {
        thread_id,
        project_id,
        environment_id,
        cli,
        data_dir,
        warnings,
    })
}
