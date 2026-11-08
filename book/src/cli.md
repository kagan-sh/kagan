# `kg`

This page is generated from `kg --help`.

Every command accepts `--json`. A failure prints the error and exits non-zero. `kg mcp` is how the agent connects. You rarely type it.

## `kg`

```text
kagan

Usage: kg [OPTIONS] [COMMAND]

Commands:
  mcp          Serve a thread's agent tools to its agent over stdio MCP
  status       Current thread context and plugin warnings
  guide        Concepts and command chapters
  skill        Find, read and create skills
  open         Open a window
  thread       Threads
  project      Projects
  program      Programs a new thread can run, as the new-thread picker lists them
  environment  Environments
  plugin       Plugins
  sidebar      Projects, sections and threads as the sidebar shows them
  section      Named groups threads are filed under
  theme        Read or change appearance
  settings     The settings page in the window
  window       The application menu and the window
  surface      Plugin surfaces in the window
  keymap       Keyboard shortcuts: built-in ones and plugin commands
  preference   Read or save a preference: confirm-archive, agent-integration, notify, plugin-status, update-check, terminal-font-size, review-viewed, slot.threads and slot.review (set from the window or with kg plugin use), or one window's `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width` and `<window>.review-wrap`, where `<window>` is a window id such as main
  ui           Native window for QA and agents
  browser      The Browser tab's page, driven by this thread
  diff         A thread environment's diff. Reads git and never writes the worktree
  preview      One file in the Preview tab, read-only
  diagnostics  Write a local diagnostics file
  update       Check for a newer kagan
  help         Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help

  -V, --version
          Print version
```

## `kg help`

```text
Print this message or the help of the given subcommand(s)

Usage: kg help [COMMAND]...

Arguments:
  [COMMAND]...
          Print help for the subcommand(s)
```

## `kg mcp`

```text
Serve a thread's agent tools to its agent over stdio MCP

Usage: kg mcp [OPTIONS] --thread <THREAD>

Options:
      --json
          Print compact JSON; failures print the error envelope

      --thread <THREAD>
          

  -h, --help
          Print help
```

## `kg status`

```text
Current thread context and plugin warnings

Usage: kg status [OPTIONS]

Options:
      --json
          Print compact JSON; failures print the error envelope

      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

      --project-id <PROJECT_ID>
          [env: KG_PROJECT_ID=]

      --environment-id <ENVIRONMENT_ID>
          [env: KG_ENVIRONMENT_ID=]

  -h, --help
          Print help
```

## `kg guide`

```text
Concepts and command chapters

Usage: kg guide [OPTIONS] [CHAPTER]

Arguments:
  [CHAPTER]
          [possible values: threads, projects, environments, plugins, skills, ui, json]

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg skill`

```text
Find, read and create skills

Usage: kg skill [OPTIONS] <COMMAND>

Commands:
  list    List skills
  show    Show one skill
  files   List a skill's files
  new     Create a skill
  add     Add a skill from a folder
  remove  Remove a skill
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg skill list`

```text
List skills

Usage: kg skill list [OPTIONS]

Options:
      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg skill show`

```text
Show one skill

Usage: kg skill show [OPTIONS] <NAME>

Arguments:
  <NAME>
          Skill name

Options:
      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg skill files`

```text
List a skill's files

Usage: kg skill files [OPTIONS] <NAME>

Arguments:
  <NAME>
          Skill name

Options:
      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg skill new`

```text
Create a skill

Usage: kg skill new [OPTIONS] <NAME>

Arguments:
  <NAME>
          Skill name, lowercase and hyphenated

Options:
      --project
          Create it in the project folder instead of the user folder

      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg skill add`

```text
Add a skill from a folder

Usage: kg skill add [OPTIONS] <PATH>

Arguments:
  <PATH>
          Folder to read the skill from

Options:
      --project
          Add it to the project folder instead of the user folder

      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg skill remove`

```text
Remove a skill

Usage: kg skill remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Skill name

Options:
      --project-id <PROJECT_ID>
          Project whose skills to include
          
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg open`

```text
Open a window

Usage: kg open [OPTIONS]

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg thread`

```text
Threads

Usage: kg thread [OPTIONS] <COMMAND>

Commands:
  get           Show a thread
  list          List threads
  run           Start a thread that runs a program in its folder
  stop          Stop the program
  input         Type into the program's terminal
  resize        Resize the program's terminal
  screen        The program's screen as ANSI escape sequences; use `--scrollback` for plain text
  scroll        Scroll the program's view back into its history, or forward with a negative count
  select        Select text on the program's screen from a cell, counted from the top left
  copy          The text selected on the program's screen
  report        Send mouse-report bytes to the program, as decimal byte values such as `27 91 77 32 128 33`. Unlike `input`, it keeps a scrolled view where it is
  paste         Paste text into the program, bracketed when the program asks for that
  send          Type text into the program and press Enter
  terminal      Extra terminals: login shells in the thread's folder
  details       The thread's folder, program, branch, base branch and last commits, read from git without fetching
  links         http and https links printed in the thread's terminals: the program's, then each extra one. A link printed again moves to the end
  archive       Archive a thread
  unarchive     Bring back an archived thread
  delete        Delete a thread and its history for good
  set-pinned    Pin or unpin a thread
  rename        Rename a thread
  mark-read     Mark a thread read
  metadata      A plugin's metadata on a thread
  set-metadata  Set a plugin's metadata on a thread
  events        A thread's events
  wait-event    Wait for an event
  wait          Wait until a reply ends, the program ends, or the time is up
  ask           Raise a question for the person. The answer arrives as `answered`
  start-again   Run a stopped or recovered thread's program again, in its folder. Opening kagan does this for a thread that was still running
  tools         Agent tools offered to the thread, with plugin instructions
  call-tool     Call an agent tool as the thread
  interactions  Pending interactions waiting for an answer
  search        Threads whose title or kept terminal lines hold the text, ignoring case. Hidden threads are left out
  help          Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg thread get`

```text
Show a thread

Usage: kg thread get <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread list`

```text
List threads

Usage: kg thread list [OPTIONS]

Options:
      --project-id <PROJECT_ID>
          [env: KG_PROJECT_ID=]

      --archived <ARCHIVED>
          [possible values: true, false]

      --limit <LIMIT>
          

      --before <BEFORE>
          

  -h, --help
          Print help
```

## `kg thread run`

```text
Start a thread that runs a program in its folder

Usage: kg thread run [OPTIONS] --project-id <PROJECT_ID> --program <PROGRAM>

Options:
      --project-id <PROJECT_ID>
          [env: KG_PROJECT_ID=]

      --program <PROGRAM>
          The program to run, such as `claude`, `codex`, or a command the person types. Words split on spaces; single or double quotes keep spaces together

      --title <TITLE>
          

      --environment <ENVIRONMENT>
          project-default, `project-on:checkout:<branch>`, `project-on:new:<base>`, personal, `worktree[:<base>[:<branch>]]`, or `reuse:<environment-id>`
          
          [default: worktree]

      --visibility <VISIBILITY>
          [possible values: visible, hidden]

      --parent-thread-id <PARENT_THREAD_ID>
          

      --lifecycle-owner-thread-id <LIFECYCLE_OWNER_THREAD_ID>
          

      --plugin-id <PLUGIN_ID>
          The plugin whose metadata this is

      --metadata <JSON>
          The metadata as JSON

  -h, --help
          Print help
```

## `kg thread stop`

```text
Stop the program

Usage: kg thread stop <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread input`

```text
Type into the program's terminal

Usage: kg thread input [OPTIONS] <ID> <TEXT>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <TEXT>
          

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

  -h, --help
          Print help
```

## `kg thread resize`

```text
Resize the program's terminal

Usage: kg thread resize [OPTIONS] <ID> <COLS> <ROWS>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <COLS>
          

  <ROWS>
          

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

  -h, --help
          Print help
```

## `kg thread screen`

```text
The program's screen as ANSI escape sequences; use `--scrollback` for plain text

Usage: kg thread screen [OPTIONS] <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

      --scrollback
          Every kept line, as plain text

      --lines <LINES>
          Last rows of that scrollback, from 1 to 10000. Refused without `--scrollback`

  -h, --help
          Print help
```

## `kg thread scroll`

```text
Scroll the program's view back into its history, or forward with a negative count

Usage: kg thread scroll [OPTIONS] <ID> <LINES>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <LINES>
          Lines up into the history; negative moves back down

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

  -h, --help
          Print help
```

## `kg thread select`

```text
Select text on the program's screen from a cell, counted from the top left

Usage: kg thread select [OPTIONS] <ID> <MODE> <COLUMN> <ROW>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <MODE>
          [possible values: simple, word, line, block, extend]

  <COLUMN>
          From the left, starting at 0

  <ROW>
          From the top, starting at 0

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

      --right
          The right half of the cell

  -h, --help
          Print help
```

## `kg thread copy`

```text
The text selected on the program's screen

Usage: kg thread copy [OPTIONS] <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

  -h, --help
          Print help
```

## `kg thread report`

```text
Send mouse-report bytes to the program, as decimal byte values such as `27 91 77 32 128 33`. Unlike `input`, it keeps a scrolled view where it is

Usage: kg thread report [OPTIONS] <ID> [BYTES]...

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  [BYTES]...
          Decimal byte values, 0 to 255, in order

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

  -h, --help
          Print help
```

## `kg thread paste`

```text
Paste text into the program, bracketed when the program asks for that

Usage: kg thread paste [OPTIONS] <ID> [TEXT]

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  [TEXT]
          The text. Omit it when `--file` is set

Options:
      --terminal <TERMINAL>
          An extra terminal, by its number; the program's terminal when left out

      --file <PATH>
          Read the paste from this path, or `-` for stdin

  -h, --help
          Print help
```

## `kg thread send`

```text
Type text into the program and press Enter

Usage: kg thread send <ID> <TEXT>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <TEXT>
          

Options:
  -h, --help
          Print help
```

## `kg thread terminal`

```text
Extra terminals: login shells in the thread's folder

Usage: kg thread terminal <COMMAND>

Commands:
  new    Open another terminal in the thread's folder, running the login shell
  list   The thread's extra terminals
  close  Close an extra terminal and end its shell
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

## `kg thread terminal new`

```text
Open another terminal in the thread's folder, running the login shell

Usage: kg thread terminal new <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread terminal list`

```text
The thread's extra terminals

Usage: kg thread terminal list <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread terminal close`

```text
Close an extra terminal and end its shell

Usage: kg thread terminal close <ID> <NUMBER>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <NUMBER>
          

Options:
  -h, --help
          Print help
```

## `kg thread details`

```text
The thread's folder, program, branch, base branch and last commits, read from git without fetching

Usage: kg thread details <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread links`

```text
http and https links printed in the thread's terminals: the program's, then each extra one. A link printed again moves to the end

Usage: kg thread links <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread archive`

```text
Archive a thread

Usage: kg thread archive <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread unarchive`

```text
Bring back an archived thread

Usage: kg thread unarchive <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread delete`

```text
Delete a thread and its history for good

Usage: kg thread delete [OPTIONS] <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --approval-thread-id <APPROVAL_THREAD_ID>
          The thread where a person approves this delete
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg thread set-pinned`

```text
Pin or unpin a thread

Usage: kg thread set-pinned <ID> <PINNED>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <PINNED>
          [possible values: true, false]

Options:
  -h, --help
          Print help
```

## `kg thread rename`

```text
Rename a thread

Usage: kg thread rename <ID> <TITLE>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <TITLE>
          

Options:
  -h, --help
          Print help
```

## `kg thread mark-read`

```text
Mark a thread read

Usage: kg thread mark-read <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread metadata`

```text
A plugin's metadata on a thread

Usage: kg thread metadata --plugin-id <PLUGIN_ID> <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --plugin-id <PLUGIN_ID>
          

  -h, --help
          Print help
```

## `kg thread set-metadata`

```text
Set a plugin's metadata on a thread

Usage: kg thread set-metadata --plugin-id <PLUGIN_ID> --value <JSON> <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --plugin-id <PLUGIN_ID>
          The plugin whose metadata this is

      --value <JSON>
          The metadata as JSON

  -h, --help
          Print help
```

## `kg thread events`

```text
A thread's events

Usage: kg thread events [OPTIONS] <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --after-seq <AFTER_SEQ>
          

      --limit <LIMIT>
          

      --order <ORDER>
          [possible values: asc, desc]

  -h, --help
          Print help
```

## `kg thread wait-event`

```text
Wait for an event

Usage: kg thread wait-event [OPTIONS] --kind <KIND> --timeout-ms <TIMEOUT_MS> <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --kind <KIND>
          

      --after-seq <AFTER_SEQ>
          

      --timeout-ms <TIMEOUT_MS>
          

  -h, --help
          Print help
```

## `kg thread wait`

```text
Wait until a reply ends, the program ends, or the time is up

Usage: kg thread wait --timeout-ms <TIMEOUT_MS> <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
      --timeout-ms <TIMEOUT_MS>
          How long to wait, in milliseconds

  -h, --help
          Print help
```

## `kg thread ask`

```text
Raise a question for the person. The answer arrives as `answered`

Usage: kg thread ask [OPTIONS] <ID> <TITLE>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <TITLE>
          

Options:
      --option <ID=LABEL>
          An option, as id=label. Repeat for at least two

  -h, --help
          Print help
```

## `kg thread start-again`

```text
Run a stopped or recovered thread's program again, in its folder. Opening kagan does this for a thread that was still running.

A claude, cursor-agent, or agent thread, and a plain codex thread, resume their conversation; when kagan cannot find it, the agent starts a new one and the thread says so.

Usage: kg thread start-again <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help (see a summary with '-h')
```

## `kg thread tools`

```text
Agent tools offered to the thread, with plugin instructions

Usage: kg thread tools <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread call-tool`

```text
Call an agent tool as the thread

Usage: kg thread call-tool [OPTIONS] <ID> <NAME>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

  <NAME>
          

Options:
      --input <INPUT>
          [default: {}]

  -h, --help
          Print help
```

## `kg thread interactions`

```text
Pending interactions waiting for an answer

Usage: kg thread interactions <ID>

Arguments:
  <ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg thread search`

```text
Threads whose title or kept terminal lines hold the text, ignoring case. Hidden threads are left out

Usage: kg thread search [OPTIONS] <TEXT>

Arguments:
  <TEXT>
          

Options:
      --limit <LIMIT>
          Most threads to return, from 1 to 200; 50 when left out

      --archived
          Also search archived threads. Each result says whether it is archived

  -h, --help
          Print help
```

## `kg project`

```text
Projects

Usage: kg project [OPTIONS] <COMMAND>

Commands:
  list     List projects
  get      Show a project
  add      Add a project for a folder
  remove   Remove a project and its threads
  reorder  Move a project in the sidebar
  help     Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg project list`

```text
List projects

Usage: kg project list

Options:
  -h, --help
          Print help
```

## `kg project get`

```text
Show a project

Usage: kg project get <ID>

Arguments:
  <ID>
          [env: KG_PROJECT_ID=]

Options:
  -h, --help
          Print help
```

## `kg project add`

```text
Add a project for a folder

Usage: kg project add <PATH>

Arguments:
  <PATH>
          

Options:
  -h, --help
          Print help
```

## `kg project remove`

```text
Remove a project and its threads

Usage: kg project remove <ID>

Arguments:
  <ID>
          Project to remove

Options:
  -h, --help
          Print help
```

## `kg project reorder`

```text
Move a project in the sidebar

Usage: kg project reorder [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --before <BEFORE>
          Place before this project; omit to move to the end, still above Personal

  -h, --help
          Print help
```

## `kg program`

```text
Programs a new thread can run, as the new-thread picker lists them

Usage: kg program [OPTIONS] <COMMAND>

Commands:
  list  Programs found on the PATH, the shell, and the recent threads; the first installed one is the default
  help  Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg program list`

```text
Programs found on the PATH, the shell, and the recent threads; the first installed one is the default

Usage: kg program list

Options:
  -h, --help
          Print help
```

## `kg environment`

```text
Environments

Usage: kg environment [OPTIONS] <COMMAND>

Commands:
  get       Show an environment
  restore   Recreate a thread's environment
  branches  Local branches of a project
  set-base  Which branch change counts compare against
  remove    Remove a worktree again after git refused because of files it would keep
  help      Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg environment get`

```text
Show an environment

Usage: kg environment get <ID>

Arguments:
  <ID>
          [env: KG_ENVIRONMENT_ID=]

Options:
  -h, --help
          Print help
```

## `kg environment restore`

```text
Recreate a thread's environment

Usage: kg environment restore <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg environment branches`

```text
Local branches of a project

Usage: kg environment branches <PROJECT_ID>

Arguments:
  <PROJECT_ID>
          [env: KG_PROJECT_ID=]

Options:
  -h, --help
          Print help
```

## `kg environment set-base`

```text
Which branch change counts compare against

Usage: kg environment set-base <ID> <BASE>

Arguments:
  <ID>
          [env: KG_ENVIRONMENT_ID=]

  <BASE>
          

Options:
  -h, --help
          Print help
```

## `kg environment remove`

```text
Remove a worktree again after git refused because of files it would keep

Usage: kg environment remove <ID>

Arguments:
  <ID>
          [env: KG_ENVIRONMENT_ID=]

Options:
  -h, --help
          Print help
```

## `kg plugin`

```text
Plugins

Usage: kg plugin [OPTIONS] <COMMAND>

Commands:
  cancel-install  Stop the plugin install that is running
  start           Start a thread to build a plugin
  command         Run one of a plugin's declared commands
  new             Scaffold a plugin
  check           Check a plugin without running it
  share           Check a committed, pushed plugin and print its index entry and install line
  reload          Reload installed code, asking a person about new powers
  dev             Watch an installed local plugin and reload on changes
  logs            Read plugin logs
  types           Write declarations for this version
  list            Installed plugins
  remove          Remove an installed plugin
  install         Install from `git:<url>@<ref>`, a local folder, or a plugin name an index lists
  update          Update to the latest commit
  index           The plugin indexes searched by name, in order
  search          Plugins the indexes list whose name or description holds the text
  enable          Enable a plugin
  disable         Disable a plugin
  config          Read or change a plugin setting
  use             Fill a slot with a plugin that offers it. `kagan` is the built-in
  help            Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg plugin cancel-install`

```text
Stop the plugin install that is running

Usage: kg plugin cancel-install

Options:
  -h, --help
          Print help
```

## `kg plugin start`

```text
Start a thread to build a plugin

Usage: kg plugin start

Options:
  -h, --help
          Print help
```

## `kg plugin command`

```text
Run one of a plugin's declared commands

Usage: kg plugin command [OPTIONS] <ID> <COMMAND>

Arguments:
  <ID>
          

  <COMMAND>
          

Options:
      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

      --project-id <PROJECT_ID>
          [env: KG_PROJECT_ID=]

  -h, --help
          Print help
```

## `kg plugin new`

```text
Scaffold a plugin

Usage: kg plugin new [OPTIONS] <NAME>

Arguments:
  <NAME>
          Plugin name; for example: kg plugin new hello

Options:
      --dir <DIR>
          

  -h, --help
          Print help
```

## `kg plugin check`

```text
Check a plugin without running it

Usage: kg plugin check [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
  -h, --help
          Print help
```

## `kg plugin share`

```text
Check a committed, pushed plugin and print its index entry and install line

Usage: kg plugin share [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
  -h, --help
          Print help
```

## `kg plugin reload`

```text
Reload installed code, asking a person about new powers

Usage: kg plugin reload [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin dev`

```text
Watch an installed local plugin and reload on changes

Usage: kg plugin dev [OPTIONS] [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin logs`

```text
Read plugin logs

Usage: kg plugin logs [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --follow
          

  -h, --help
          Print help
```

## `kg plugin types`

```text
Write declarations for this version

Usage: kg plugin types [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
  -h, --help
          Print help
```

## `kg plugin list`

```text
Installed plugins

Usage: kg plugin list

Options:
  -h, --help
          Print help
```

## `kg plugin remove`

```text
Remove an installed plugin

Usage: kg plugin remove [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --thread-id <THREAD_ID>
          The thread where a person approves this removal
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin install`

```text
Install from `git:<url>@<ref>`, a local folder, or a plugin name an index lists

Usage: kg plugin install [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>
          

Options:
      --plugin <PLUGIN>
          

      --thread-id <THREAD_ID>
          The thread where a person approves the plugin's powers
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin update`

```text
Update to the latest commit

Usage: kg plugin update [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --thread-id <THREAD_ID>
          The thread where a person approves new powers
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin index`

```text
The plugin indexes searched by name, in order

Usage: kg plugin index <COMMAND>

Commands:
  add     Add an index: `git:<url>@<ref>`, or a file or folder on this computer
  list    The indexes, first searched first
  remove  Remove an index
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

## `kg plugin index add`

```text
Add an index: `git:<url>@<ref>`, or a file or folder on this computer

Usage: kg plugin index add <LOCATION>

Arguments:
  <LOCATION>
          

Options:
  -h, --help
          Print help
```

## `kg plugin index list`

```text
The indexes, first searched first

Usage: kg plugin index list

Options:
  -h, --help
          Print help
```

## `kg plugin index remove`

```text
Remove an index

Usage: kg plugin index remove <LOCATION>

Arguments:
  <LOCATION>
          

Options:
  -h, --help
          Print help
```

## `kg plugin search`

```text
Plugins the indexes list whose name or description holds the text

Usage: kg plugin search [TEXT]

Arguments:
  [TEXT]
          [default: ""]

Options:
  -h, --help
          Print help
```

## `kg plugin enable`

```text
Enable a plugin

Usage: kg plugin enable <ID>

Arguments:
  <ID>
          

Options:
  -h, --help
          Print help
```

## `kg plugin disable`

```text
Disable a plugin

Usage: kg plugin disable <ID>

Arguments:
  <ID>
          

Options:
  -h, --help
          Print help
```

## `kg plugin config`

```text
Read or change a plugin setting

Usage: kg plugin config [OPTIONS] <ID> <COMMAND>

Commands:
  get    Read a setting
  set    Change a setting
  clear  Forget a secret, or clear a project setting
  help   Print this message or the help of the given subcommand(s)

Arguments:
  <ID>
          

Options:
      --thread-id <THREAD_ID>
          The thread where a person approves a folder setting
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg plugin config get`

```text
Read a setting

Usage: kg plugin config get <KEY>

Arguments:
  <KEY>
          

Options:
  -h, --help
          Print help
```

## `kg plugin config set`

```text
Change a setting

Usage: kg plugin config set <KEY> [VALUE]

Arguments:
  <KEY>
          

  [VALUE]
          Omit it to type it at a prompt that does not show it, or to pipe it on stdin. Do that for a secret, so it stays out of your shell history

Options:
  -h, --help
          Print help
```

## `kg plugin config clear`

```text
Forget a secret, or clear a project setting

Usage: kg plugin config clear <KEY>

Arguments:
  <KEY>
          

Options:
  -h, --help
          Print help
```

## `kg plugin use`

```text
Fill a slot with a plugin that offers it. `kagan` is the built-in

Usage: kg plugin use [OPTIONS] <SLOT> <ID>

Arguments:
  <SLOT>
          [possible values: threads, review]

  <ID>
          A plugin id, or `kagan`

Options:
      --thread-id <THREAD_ID>
          The thread where a person approves the choice
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg sidebar`

```text
Projects, sections and threads as the sidebar shows them

Usage: kg sidebar [OPTIONS]

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg section`

```text
Named groups threads are filed under

Usage: kg section [OPTIONS] <COMMAND>

Commands:
  create  Create a section, and file a thread under it
  rename  Rename a section
  delete  Remove a section. Threads in it stay, unfiled
  file    File a thread under a section. Omit the section to clear it
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg section create`

```text
Create a section, and file a thread under it

Usage: kg section create [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg section rename`

```text
Rename a section

Usage: kg section rename <ID> <NAME>

Arguments:
  <ID>
          

  <NAME>
          

Options:
  -h, --help
          Print help
```

## `kg section delete`

```text
Remove a section. Threads in it stay, unfiled

Usage: kg section delete <ID>

Arguments:
  <ID>
          

Options:
  -h, --help
          Print help
```

## `kg section file`

```text
File a thread under a section. Omit the section to clear it

Usage: kg section file [OPTIONS] --thread-id <THREAD_ID>

Options:
      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

      --section-id <SECTION_ID>
          

  -h, --help
          Print help
```

## `kg theme`

```text
Read or change appearance

Usage: kg theme [OPTIONS] <COMMAND>

Commands:
  get     Selected theme and mode
  set     Select a theme and mode
  new     Copy a theme to `<dataDir>/themes/<id>.json` and print its path
  folder  Print the folder of your own theme files, making it when missing
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg theme get`

```text
Selected theme and mode

Usage: kg theme get

Options:
  -h, --help
          Print help
```

## `kg theme set`

```text
Select a theme and mode

Usage: kg theme set <THEME> <MODE>

Arguments:
  <THEME>
          

  <MODE>
          Possible values:
          - light
          - dark
          - system: Follow the operating system's light or dark mode

Options:
  -h, --help
          Print help (see a summary with '-h')
```

## `kg theme new`

```text
Copy a theme to `<dataDir>/themes/<id>.json` and print its path

Usage: kg theme new [OPTIONS] <ID>

Arguments:
  <ID>
          The new file's id: lowercase letters, digits and dashes

Options:
      --from <FROM>
          The theme to copy; the one in use when left out

  -h, --help
          Print help
```

## `kg theme folder`

```text
Print the folder of your own theme files, making it when missing

Usage: kg theme folder

Options:
  -h, --help
          Print help
```

## `kg settings`

```text
The settings page in the window

Usage: kg settings [OPTIONS] <COMMAND>

Commands:
  open     Show the settings page, opening the window if it is closed
  plugins  Show the plugin management section
  help     Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg settings open`

```text
Show the settings page, opening the window if it is closed

Usage: kg settings open

Options:
  -h, --help
          Print help
```

## `kg settings plugins`

```text
Show the plugin management section

Usage: kg settings plugins

Options:
  -h, --help
          Print help
```

## `kg window`

```text
The application menu and the window

Usage: kg window [OPTIONS] <COMMAND>

Commands:
  about                    Show the application name and version
  hide                     Hide kagan
  hide-others              Hide the other applications
  show-all                 Show the applications hidden by Hide Others
  quit                     Quit. Programs still running end
  minimize                 Minimize the window
  zoom                     Zoom the window
  close                    Close the window. Programs keep running
  next-thread              Open the next visible thread and focus its terminal
  previous-thread          Open the previous visible thread and focus its terminal
  thread                   Open the Nth visible thread and focus its terminal
  next-thread-needing-you  Open the oldest unread thread or question and focus its terminal
  toggle-review-wrap       Toggle wrapping in the open diff review
  command-list             Open the command list
  search-threads           Open the command list on thread search
  next-terminal            Move the keyboard to the shown thread's next terminal
  previous-terminal        Move the keyboard to the shown thread's previous terminal
  open-local-link          Open the newest local link the shown thread printed, such as `localhost:3000`, in the system browser
  help                     Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg window about`

```text
Show the application name and version

Usage: kg window about

Options:
  -h, --help
          Print help
```

## `kg window hide`

```text
Hide kagan

Usage: kg window hide

Options:
  -h, --help
          Print help
```

## `kg window hide-others`

```text
Hide the other applications

Usage: kg window hide-others

Options:
  -h, --help
          Print help
```

## `kg window show-all`

```text
Show the applications hidden by Hide Others

Usage: kg window show-all

Options:
  -h, --help
          Print help
```

## `kg window quit`

```text
Quit. Programs still running end

Usage: kg window quit

Options:
  -h, --help
          Print help
```

## `kg window minimize`

```text
Minimize the window

Usage: kg window minimize

Options:
  -h, --help
          Print help
```

## `kg window zoom`

```text
Zoom the window

Usage: kg window zoom

Options:
  -h, --help
          Print help
```

## `kg window close`

```text
Close the window. Programs keep running

Usage: kg window close

Options:
  -h, --help
          Print help
```

## `kg window next-thread`

```text
Open the next visible thread and focus its terminal

Usage: kg window next-thread

Options:
  -h, --help
          Print help
```

## `kg window previous-thread`

```text
Open the previous visible thread and focus its terminal

Usage: kg window previous-thread

Options:
  -h, --help
          Print help
```

## `kg window thread`

```text
Open the Nth visible thread and focus its terminal

Usage: kg window thread <INDEX>

Arguments:
  <INDEX>
          

Options:
  -h, --help
          Print help
```

## `kg window next-thread-needing-you`

```text
Open the oldest unread thread or question and focus its terminal

Usage: kg window next-thread-needing-you

Options:
  -h, --help
          Print help
```

## `kg window toggle-review-wrap`

```text
Toggle wrapping in the open diff review

Usage: kg window toggle-review-wrap

Options:
  -h, --help
          Print help
```

## `kg window command-list`

```text
Open the command list

Usage: kg window command-list

Options:
  -h, --help
          Print help
```

## `kg window search-threads`

```text
Open the command list on thread search

Usage: kg window search-threads

Options:
  -h, --help
          Print help
```

## `kg window next-terminal`

```text
Move the keyboard to the shown thread's next terminal

Usage: kg window next-terminal

Options:
  -h, --help
          Print help
```

## `kg window previous-terminal`

```text
Move the keyboard to the shown thread's previous terminal

Usage: kg window previous-terminal

Options:
  -h, --help
          Print help
```

## `kg window open-local-link`

```text
Open the newest local link the shown thread printed, such as `localhost:3000`, in the system browser

Usage: kg window open-local-link

Options:
  -h, --help
          Print help
```

## `kg surface`

```text
Plugin surfaces in the window

Usage: kg surface [OPTIONS] <COMMAND>

Commands:
  list     Declared plugin surfaces, and which are on screen
  threads  Show the threads list in the left sidebar
  open     Show a declared surface. Succeeds only when it is on screen
  close    Close a left tool, a center page, or a right tab
  help     Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg surface list`

```text
Declared plugin surfaces, and which are on screen

Usage: kg surface list

Options:
  -h, --help
          Print help
```

## `kg surface threads`

```text
Show the threads list in the left sidebar

Usage: kg surface threads

Options:
  -h, --help
          Print help
```

## `kg surface open`

```text
Show a declared surface. Succeeds only when it is on screen

Usage: kg surface open <PLUGIN> <ID>

Arguments:
  <PLUGIN>
          

  <ID>
          

Options:
  -h, --help
          Print help
```

## `kg surface close`

```text
Close a left tool, a center page, or a right tab

Usage: kg surface close <PLUGIN> <ID>

Arguments:
  <PLUGIN>
          

  <ID>
          

Options:
  -h, --help
          Print help
```

## `kg keymap`

```text
Keyboard shortcuts: built-in ones and plugin commands

Usage: kg keymap [OPTIONS] <COMMAND>

Commands:
  get     Saved shortcut overrides, and the key of each enabled plugin's commands
  set     Rebind a shortcut, or reset it
  remove  Leave a plugin's command with no key
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg keymap get`

```text
Saved shortcut overrides, and the key of each enabled plugin's commands

Usage: kg keymap get

Options:
  -h, --help
          Print help
```

## `kg keymap set`

```text
Rebind a shortcut, or reset it

Usage: kg keymap set [OPTIONS] <ACTION> <KEY>

Arguments:
  <ACTION>
          Action name, such as `ToggleSidebar`, or `<plugin-id>:<command>` for a plugin's command

  <KEY>
          Chord to record. Empty with `--reset`

Options:
      --reset
          Drop the saved chord and use the default key

  -h, --help
          Print help
```

## `kg keymap remove`

```text
Leave a plugin's command with no key

Usage: kg keymap remove <ACTION>

Arguments:
  <ACTION>
          `<plugin-id>:<command>`

Options:
  -h, --help
          Print help
```

## `kg preference`

```text
Read or save a preference: confirm-archive, agent-integration, notify, plugin-status, update-check, terminal-font-size, review-viewed, slot.threads and slot.review (set from the window or with kg plugin use), or one window's `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width` and `<window>.review-wrap`, where `<window>` is a window id such as main

Usage: kg preference [OPTIONS] <COMMAND>

Commands:
  get   Read a preference
  set   Save a preference
  help  Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg preference get`

```text
Read a preference

Usage: kg preference get <KEY>

Arguments:
  <KEY>
          

Options:
  -h, --help
          Print help
```

## `kg preference set`

```text
Save a preference

Usage: kg preference set <KEY> <VALUE>

Arguments:
  <KEY>
          

  <VALUE>
          

Options:
  -h, --help
          Print help
```

## `kg ui`

```text
Native window for QA and agents

Usage: kg ui [OPTIONS] <COMMAND>

Commands:
  snapshot    Observed elements with stable ids, roles, bounds, and focus state
  click       Click the center of an element. `--within` picks the one inside that element, when several match
  type        Send text to the focused control
  key         Send a keystroke such as escape or cmd-,
  scroll      Scroll an element by dy pixels
  drag        Press an element, drag it onto another, and release. `--at` picks the target's top quarter, middle or bottom quarter. `--hold` keeps the button down: finish with `kg ui drop`, or cancel with `kg ui key escape`
  drop        Release a drag that `kg ui drag --hold` left held, where the pointer is
  screenshot  Save a PNG without focusing the window
  help        Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg ui snapshot`

```text
Observed elements with stable ids, roles, bounds, and focus state

Usage: kg ui snapshot [OPTIONS]

Options:
      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui click`

```text
Click the center of an element. `--within` picks the one inside that element, when several match

Usage: kg ui click [OPTIONS] <ELEMENT_ID>

Arguments:
  <ELEMENT_ID>
          

Options:
      --within <WITHIN>
          

      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui type`

```text
Send text to the focused control

Usage: kg ui type [OPTIONS] <TEXT>

Arguments:
  <TEXT>
          

Options:
      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui key`

```text
Send a keystroke such as escape or cmd-,

Usage: kg ui key [OPTIONS] <KEYSTROKE>

Arguments:
  <KEYSTROKE>
          

Options:
      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui scroll`

```text
Scroll an element by dy pixels

Usage: kg ui scroll [OPTIONS] <ELEMENT_ID> <DY>

Arguments:
  <ELEMENT_ID>
          

  <DY>
          

Options:
      --within <WITHIN>
          

      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui drag`

```text
Press an element, drag it onto another, and release. `--at` picks the target's top quarter, middle or bottom quarter. `--hold` keeps the button down: finish with `kg ui drop`, or cancel with `kg ui key escape`

Usage: kg ui drag [OPTIONS] <FROM> <TO>

Arguments:
  <FROM>
          

  <TO>
          

Options:
      --at <AT>
          [default: middle]
          [possible values: top, middle, bottom]

      --within <WITHIN>
          Pick both elements inside the element with this id

      --hold
          

      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui drop`

```text
Release a drag that `kg ui drag --hold` left held, where the pointer is

Usage: kg ui drop [OPTIONS]

Options:
      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg ui screenshot`

```text
Save a PNG without focusing the window

Usage: kg ui screenshot [OPTIONS] <PATH>

Arguments:
  <PATH>
          

Options:
      --window <WINDOW>
          [default: 0]

  -h, --help
          Print help
```

## `kg browser`

```text
The Browser tab's page, driven by this thread

Usage: kg browser [OPTIONS] <COMMAND>

Commands:
  open      Give this thread the Browser tab's page and load a URL
  status    The page this thread drives: thread, url, title, and whether the tab is drawn
  snapshot  Controls and text on the page, each control with a ref for the next command
  goto      Load another URL in the page
  back      Go back in the page's history
  forward   Go forward in the page's history
  reload    Load the page again
  click     Click the control with this ref
  fill      Set a control's text, or read it from --file
  type      Insert text into the focused control
  press     Press enter, escape, tab, a letter, or an arrow
  select    Choose a <select> option by value or label
  scroll    Scroll the page by dy pixels
  close     Stop driving; the page stays as the person's
  help      Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg browser open`

```text
Give this thread the Browser tab's page and load a URL

Usage: kg browser open <URL> <THREAD_ID>

Arguments:
  <URL>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser status`

```text
The page this thread drives: thread, url, title, and whether the tab is drawn

Usage: kg browser status <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser snapshot`

```text
Controls and text on the page, each control with a ref for the next command

Usage: kg browser snapshot <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser goto`

```text
Load another URL in the page

Usage: kg browser goto <URL> <THREAD_ID>

Arguments:
  <URL>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser back`

```text
Go back in the page's history

Usage: kg browser back <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser forward`

```text
Go forward in the page's history

Usage: kg browser forward <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser reload`

```text
Load the page again

Usage: kg browser reload <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser click`

```text
Click the control with this ref

Usage: kg browser click <ref> <THREAD_ID>

Arguments:
  <ref>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser fill`

```text
Set a control's text, or read it from --file

Usage: kg browser fill [OPTIONS] --thread-id <THREAD_ID> <ref> [TEXT]

Arguments:
  <ref>
          

  [TEXT]
          The text. Omit it when `--file` is set

Options:
      --file <PATH>
          Read the text from this path, or `-` for stdin

      --thread-id <THREAD_ID>
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg browser type`

```text
Insert text into the focused control

Usage: kg browser type <TEXT> <THREAD_ID>

Arguments:
  <TEXT>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser press`

```text
Press enter, escape, tab, a letter, or an arrow

Usage: kg browser press <KEY> <THREAD_ID>

Arguments:
  <KEY>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser select`

```text
Choose a <select> option by value or label

Usage: kg browser select <ref> <OPTION> <THREAD_ID>

Arguments:
  <ref>
          

  <OPTION>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser scroll`

```text
Scroll the page by dy pixels

Usage: kg browser scroll <DY> <THREAD_ID>

Arguments:
  <DY>
          

  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg browser close`

```text
Stop driving; the page stays as the person's

Usage: kg browser close <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg diff`

```text
A thread environment's diff. Reads git and never writes the worktree

Usage: kg diff [OPTIONS] <COMMAND>

Commands:
  list     Changed files in a scope
  show     One file's diff and its new side
  commits  Commits that can be reviewed, newest first
  help     Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg diff list`

```text
Changed files in a scope

Usage: kg diff list [OPTIONS] --scope <SCOPE> <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
      --scope <SCOPE>
          [possible values: uncommitted, branch, commit]

      --base <BASE>
          

      --rev <REV>
          

  -h, --help
          Print help
```

## `kg diff show`

```text
One file's diff and its new side

Usage: kg diff show [OPTIONS] --scope <SCOPE> <THREAD_ID> <PATH>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

  <PATH>
          

Options:
      --scope <SCOPE>
          [possible values: uncommitted, branch, commit]

      --base <BASE>
          

      --rev <REV>
          

  -h, --help
          Print help
```

## `kg diff commits`

```text
Commits that can be reviewed, newest first

Usage: kg diff commits <THREAD_ID>

Arguments:
  <THREAD_ID>
          [env: KG_THREAD_ID=]

Options:
  -h, --help
          Print help
```

## `kg preview`

```text
One file in the Preview tab, read-only

Usage: kg preview [OPTIONS] <COMMAND>

Commands:
  show  Show one file in the Preview tab
  help  Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg preview show`

```text
Show one file in the Preview tab

Usage: kg preview show [OPTIONS] <PATH>

Arguments:
  <PATH>
          File to show. A relative name joins the thread's folder; without a thread it must be absolute

Options:
      --thread-id <THREAD_ID>
          Thread whose folder a relative path joins (defaults to `KG_THREAD_ID`)
          
          [env: KG_THREAD_ID=]

  -h, --help
          Print help
```

## `kg diagnostics`

```text
Write a local diagnostics file

Usage: kg diagnostics [OPTIONS] <COMMAND>

Commands:
  export  Write a local diagnostics file. Refuses to replace an existing file
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg diagnostics export`

```text
Write a local diagnostics file. Refuses to replace an existing file

Usage: kg diagnostics export [OPTIONS]

Options:
      --path <PATH>
          File to write. The parent folder must exist. Omit to write under the data directory

  -h, --help
          Print help
```

## `kg update`

```text
Check for a newer kagan

Usage: kg update [OPTIONS] <COMMAND>

Commands:
  check   Look for a newer kagan on the GitHub releases page now
  status  The running version, how this copy was installed, and what the last check found. Only a person installs an update, from the window
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          Print compact JSON; failures print the error envelope

  -h, --help
          Print help
```

## `kg update check`

```text
Look for a newer kagan on the GitHub releases page now

Usage: kg update check

Options:
  -h, --help
          Print help
```

## `kg update status`

```text
The running version, how this copy was installed, and what the last check found. Only a person installs an update, from the window

Usage: kg update status

Options:
  -h, --help
          Print help
```

