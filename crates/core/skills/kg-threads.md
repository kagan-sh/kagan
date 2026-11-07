---
name: kg-threads
description: "Create or operate a kagan thread: when work stays here, how to start one, and how to paste, wait, stop, ask, and read it."
---

# Threads

`kg` is how you use kagan. `kg status --json` is this thread. `KG_THREAD_ID`, `KG_PROJECT_ID`, and `KG_ENVIRONMENT_ID` fill the matching arguments when you leave them out. `cli` in that status is the binary that talks to the open app.

Read `kg guide threads` for the flag list and for what `waiting`, `idle`, and `busy` mean. Read `kg thread <command> --help` for the live flags. This skill says when a job is another thread, and the commands that do it. Pass `--json` when the output decides the next command. A nonzero exit is a failure.

Stay in this thread unless the person asked for another one.

## When the job stays here

Keep the work here when it is one task, the person is talking to you, or you are running a command in this folder. Do not start a thread to hold a note, to retry the same edit, or to ask something you can ask here.

`kg thread ask` raises a question in this thread. The answer arrives as `answered`.

## When it becomes another thread

Start another thread when the person asked for one. A thread is one program in one folder. The child does not inherit this conversation. Give it a prompt that names the outcome, the constraints, what to check, and what to report back.

A success does not notify the parent. A failed or interrupted child appends a notice on its parent. A stop the person asked for stays quiet.

Report the new thread id. Do not build a URL for it.

## Start one

```sh
kg thread run --project-id <id> --program <command> --environment <spec> --title <title>
```

- `--project-id` is required. Pass it. Inside a thread, `KG_PROJECT_ID` supplies this project when you mean this project.
- `--program` is the command, such as `claude`, `codex`, `cursor-agent`, `opencode`, `pi`, or `kimi`. `kg program list --json` lists what is installed and the default. Words split on spaces. Single or double quotes keep spaces together. A backslash is an ordinary character.
- kagan does not take a model. A model flag belongs to the program, inside `--program`: `--program 'claude --model <name>'`.
- `--environment` chooses the folder. `project-default` is the project folder on its current branch. `project-on:checkout:<branch>` checks out that local branch in the project folder. `project-on:new:<base>` creates a branch there from that local branch. The name is the thread title: `--title`, or the program line when you omit it. Creating or switching is refused while the checkout has uncommitted changes, while HEAD is detached or the folder has no commit yet, while a merge, rebase, cherry-pick or revert is in progress, or while another unarchived thread is using the folder. A base that is not a local branch is refused. A blocked checkout is reported before that thread. `personal` is the personal folder. `worktree` is a new worktree and is the default when you omit the flag. `worktree:<base>` and `worktree:<base>:<branch>` name the base and the branch. `reuse:<environment-id>` uses a folder that already exists. A folder that is not a git repository refuses `worktree` and names `--environment project-default`.
- `--title` is the thread's name. Omit it and the title starts as the program. The program may rename the thread once, with `kg thread rename`. A later rename says the title was kept. A name set in the window stays.

The rest of the flags are `kg thread run --help` and `kg guide threads`.

## A long prompt

`kg thread run` does not take the prompt. The program starts in its terminal. Send the prompt from a file so the shell does not eat it:

```sh
kg thread paste <id> --file <path>
```

`--file -` reads stdin. The cap is 1 MiB. Inside double quotes the shell runs backticks and `$(...)` before `kg` sees the text, which corrupts the prompt and can run commands. Paste sends the file's text and does not add a key after it. A newline in the file reaches the program. Tab and newline stay. Other control characters are dropped.

`kg thread send <id> <text>` types that text and presses Enter. The cap is 1 MiB. The same control characters are dropped. It writes a `message` event with role `user`. It does not queue, and it fails when the program is not running.

`kg thread input <id> <text>` types into the terminal, including a short submit. Use paste for anything long.

## Parent and lifetime

`--parent-thread-id <id>` nests the new thread under that thread in the sidebar. It changes nothing about how long the child lives. From this thread, pass this thread's id when the person wants the child nested here. The parent has to be unarchived and in the same project.

`--lifecycle-owner-thread-id <id>` is the thread the new one lives and dies with. Archiving or deleting the owner does the same to the child. Set it only when you create the thread. Stop does not follow the owner. Archive keeps history. Delete removes the thread and its history, and waits for the person. The owner has the same limits as a parent: unarchived, and in this project. The parent and the owner can be different threads.

`--visibility hidden` keeps the thread out of the sidebar. It stays reachable by id. The default is `visible`.

## Paste, wait, stop, ask, and events

Do not poll with sleeps or repeated status reads.

- `kg thread paste <id> --file <path>` sends text, as above.
- `kg thread send <id> <text>` types that text and presses Enter, and writes a user message.
- `kg thread wait <id> --timeout-ms <ms>` waits until a reply ends, the program ends, or the time is up. `--timeout-ms` is required. It returns `waiting`, `exited` (the row is idle, including after Stop), `failed`, `not-busy`, or `timeout`.
- `kg thread wait-event <id> --kind <kind> --timeout-ms <ms>` waits for one event. `--after-seq` skips what you have already read.
- `kg thread stop <id>` stops the program now. `kg thread archive <id>` is not a stop. Archive retires the thread after a short undo grace. `kg thread unarchive` brings it back.
- `kg thread ask <id> <title> --option <id=label> --option <id=label>` raises a question. Pass at least two options. The question shows in Questions, with approvals first. The answer arrives as an `answered` event.
- `kg thread get <id>` is the row: status, parent, environment, title. `kg thread events <id> --json` is the log. `--after-seq` and `--limit` page it. `kg thread screen <id> --scrollback` is the kept terminal text.

`kg thread start-again <id>` runs a stopped program again in the same folder. Only the person asking for that runs it. A `claude` thread, and a plain `codex` thread, resume their conversation when kagan can find it. When it cannot, the thread says the program started a new one.

## What the person approves

Do not approve on their behalf, and do not answer these with `kg ui`.

- Plugin install, and a reload whose powers grew, wait for the person. The command fails if they decline.
- `kg thread delete` waits for the person. The question names a personal folder when delete would remove one.
- A question from `kg thread ask` is theirs to answer.

Stop, archive, and unarchive do not ask.
