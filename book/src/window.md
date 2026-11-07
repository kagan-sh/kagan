# The window

From left to right: the **rail** (icons), the **left sidebar** (projects and threads), the **center** (the selected thread's terminal), and the **right sidebar**. The right sidebar has Questions, Changes, Details, plugin panels, the Browser, extra terminals, and Preview while a file is open. Open the Browser, a terminal, or a plugin panel with `+`.

On a narrow window, the right sidebar hides first, then the left. The rail stays. Cmd+B shows the left sidebar. Cmd+Backslash shows the right. Escape closes the one that is open.

## Use the window

1. Start or select a thread in the left sidebar. The center is that thread's terminal. Type to the program there.
2. Open **Changes** in the right sidebar when you want the diff. [Review changes](review.md) sends comments back to the program.
3. Use the rail for plugin tools, plugin pages, **New plugin…**, and Settings.

The rest of this page is each part of that window.

## Left sidebar

- **+** next to a project starts a thread.
- **Add project** at the bottom adds a folder.
- Right-click a project and choose **Remove project…**. Stop its programs first.

kagan forgets a removed project and its threads. Your folder stays. You cannot remove Personal.

| Mark | Meaning |
| --- | --- |
| Spinner | The program is busy |
| Amber ring | The thread needs you |
| Red fill | The thread failed |
| Filled square | The program finished while you looked at a different thread |
| Small dot | The program runs and waits |
| Hollow square | The program ended |
| Pin | You pinned the thread |

**Archived** at the bottom lists archived threads. **Restore** brings one back. Its program stays stopped.

## Rail

From top to bottom: the kagan mark (About kagan and Settings…), **Threads**, plugin tools, plugin pages, **Manage plugins**, **New plugin…**, the command list, and Settings.

A plugin icon opens its tool below the threads, or its page in the center. Click the icon again, or **Close**, to return. **New plugin…** starts a Personal thread with your first installed agent and a prompt to make a plugin.

## Activity strip

One line at the bottom of the window, under the sidebars and the center. It is closed by default.

- **Quiet** means no program is busy with a reply. **1 agent working** means one thread is. **2 agents working** means two.
- **2 needs you** means other threads want you. Click it, or press Option+Cmd+J, to open the next one. The thread you are viewing is not counted.
- **Update available** means a check found a newer kagan. **Not now** on the update notice hides it here too.
- **Installing /home/me/plugins/kagan-tools** or **Installing git:github.com/acme/tools@main** means a plugin install is running. The words are the source.

Click the line, or press Enter or Space while it has focus, to see the last seven things: turns started and ended, other threads that need you, updates, and plugin installs. A new row is added at the bottom. Click again, or press Escape, to close it.

A plugin may add one short piece of plain text at the right, such as a usage figure. It shows the plugin's own label first, then the text, in a color for quiet, busy, needs you, or failed. You cannot click these, and they never take the keyboard. At most four show, and only while the window is at least 720 px wide. Turn **Plugin status** off in Settings → Appearance to hide them all, or turn one plugin off in Installed.

The list stays in memory and is gone when kagan quits. A question or a finished turn never takes your keyboard. Plugins cannot draw panels in the strip.

## Thread

The center is the selected thread's terminal: scrollback, mouse, copy, and paste.

The header menu has **Pin**, **Archive**, **Rename…**, and **Save diagnostics**. A plugin can put a command button on that title bar with `"header": true`. When the program ends, the line under the terminal says why and shows **Start again**. Start again picks up a Claude Code or Codex conversation where you left it, when kagan can find it.

A program can rename its thread once. A title you set stays.

### Extra terminals

Click **+** in the right sidebar and choose **Terminal**, or press Cmd+T, to open a shell in the thread's folder. Each shell is a tab: **Terminal 1**, **Terminal 2**, and so on. The center always shows the program.

Click a tab, or press Cmd+Alt+] and Cmd+Alt+[, to move between the program and the shells. Close a shell with **×**, Cmd+Shift+W, or `exit`.

`kg thread terminal new`, `list`, and `close` do this from a terminal. Add `--terminal <n>` to `kg thread input` and the other terminal commands to select an extra terminal.

Extra terminals do not change the thread's marks. Archive and Quit end them. They do not return after kagan restarts.

### Links

Hold Cmd and click a link in the terminal. On Linux and Windows, hold Ctrl and click it.

When a program prints a local address, **Open localhost:3000** appears over the terminal. Click it, or press Cmd+Alt+O. `kg thread links` lists the links from the thread's terminals.

### Browser

Click **+** and choose **Browser**. Type an address and press Enter. Only http and https pages open. A local address such as `localhost:3000` opens over http. Your logins end when the page closes. It cannot use your camera or microphone. Closing the tab closes the page. On Linux the page is the system WebKit view. kagan uses an X11 window when `DISPLAY` is set, including XWayland. A Wayland desktop with no X11 display shows one sentence instead of the page.

The address row has **Back**, **Forward**, **Reload**, and a button that opens the address in your usual browser. Back, Forward, and Reload stay off until a page is open. A lock marks an https address.

A thread's agent can drive the same page with `kg browser`. The tab then shows **Driven by** the thread and **Stop agent** instead of the address field, and the four buttons stay. Stop agent, or `kg browser close`, leaves the page open for you with its address in the field. Showing another thread hides the page without closing it. The Browser tab comes back with that thread. A plugin can drive this same page when you approve the Browser area on its install card. It does not add a second Browser tab. Only one thread drives it. Another thread is refused, and the message names the owner. After 30 minutes with no `kg browser` command, the next command ends that control and leaves the page as yours.

## Details

**Details** shows the folder, the folder kind, and the program. The copy icon copies the folder path.

In a git folder it also shows the branch, how many commits it is ahead of and behind the base branch, and the last five commits. kagan reads your folder every few seconds. It never fetches and never contacts GitHub. `kg thread details` prints the same facts.

## Questions

**Questions** lists each question that waits, with the thread title. Approvals come before other questions. You can approve a plugin install. Only you can approve a request to delete a thread. An install card selects **Cancel** first, so an accidental Enter does not install.

A question that includes fields opens over that thread's terminal and takes the keyboard. Tab stays inside it. Escape closes the form without answering, and the question stays in this list. The form is not drawn over a review, a plugin page, Archived, or Settings, or when the right sidebar is full screen.

If the right sidebar is hidden, its toggle shows how many questions are waiting. A question that only you can answer opens the sidebar over the center.

## Command list

Press Cmd+Shift+A, click the search icon on the rail, or run `kg window command-list`.

Type to filter. Each row shows its key. Enter runs the selected command. A command that cannot run now is gray and tells you why.

**Search threads…** (Cmd+Alt+F) finds threads by title and terminal text. Enter opens the thread at the matching line. After an archived thread's 10 seconds end, search finds it only by title. `kg thread search` gives the same results.

## Settings

Press Cmd+, or click the gear at the bottom of the rail. Settings has Appearance, Agents, Updates, Keyboard, and Plugins. Updates shows the version, **Check now**, what the last check found, and **Check automatically**, which is off until you turn it on. Escape or **Close** leaves Settings.

## Quit

Quit ends every program. If a program is running, kagan asks first. Close the window to keep the programs running. `kg window quit` ends them without asking.

On macOS, Quit is Cmd+Q and Close is Cmd+W. Click the Dock icon to open the window again.
