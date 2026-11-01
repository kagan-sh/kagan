# The window

From left to right, the window has these parts:

- **Rail**: a column of icons at the left edge.
- **Left sidebar**: your projects and their threads.
- **Center**: the terminal of the selected thread.
- **Right sidebar**: Questions, Changes, and Details.

On a narrow window, the right sidebar hides first, then the left sidebar. The rail stays. Press Cmd+B to show the left sidebar or Cmd+Backslash to show the right sidebar. Press Escape to close it.

## Left sidebar

- To start a thread, click **+** next to the project.
- To add a project, click **Add project** at the bottom.
- To remove a project, right-click it and select **Remove project…**. Stop its programs first.

kagan forgets a removed project and its threads. Your folder stays. You cannot remove Personal.

Each thread has a mark:

| Mark | Meaning |
| --- | --- |
| Spinner | The program is busy |
| Amber ring | The thread needs you |
| Red fill | The thread failed |
| Filled square | The program finished while you looked at a different thread |
| Small dot | The program runs and waits |
| Hollow square | The program ended |
| Pin | You pinned the thread |

Click **Archived** at the bottom of the list to find archived threads. **Restore** brings a thread back. Its program stays stopped.

## Rail

From top to bottom, the rail has:

- The kagan mark, which opens **About kagan** and **Settings…**.
- **Threads**, plugin tools, plugin pages, **Manage plugins**, and **New plugin…**.
- The command list and Settings.

A plugin icon opens its tool below the threads or its page in the center. Click the icon again, or **Close**, to return. **New plugin…** starts a Personal thread with your first installed agent and a prompt to make a plugin.

## Thread

The center shows the terminal of the selected thread. The terminal has scrollback, mouse support, and copy and paste.

The thread header menu has **Pin**, **Archive**, **Rename…**, and **Save diagnostics**. When the program ends, the line under the terminal tells you why and shows **Start again**.

A program can rename its thread one time. A title that you set stays.

### Extra terminals

To open a shell in the thread's folder, click **+** in the thread header or press Cmd+T. Tabs over the terminal show the program first, then **Terminal 1**, **Terminal 2**, and so on.

Click a tab, or press Cmd+Alt+] and Cmd+Alt+[, to switch. To close an extra terminal, click **×**, press Cmd+Shift+W, or type `exit`.

From a terminal, use `kg thread terminal new`, `list`, and `close`. Add `--terminal <n>` to `kg thread input` and other terminal commands to select an extra terminal.

Extra terminals do not change the thread's marks. Archive and Quit end them. They do not return after kagan restarts.

### Links

To open a link in the terminal, hold Cmd and click it. On Linux and Windows, hold Ctrl and click it.

When a program prints a local address, **Open localhost:3000** appears over the terminal. Click it, or press Cmd+Alt+O, to open the address in your browser. `kg thread links` lists the links from the thread's terminals.

## Details

**Details** shows the selected thread's folder, folder kind, and program. Click the copy icon to copy the folder path.

In a git folder, Details also shows the branch, its commits ahead of and behind the base branch, and the last five commits. kagan reads your folder every few seconds. It never fetches and never contacts GitHub. `kg thread details` prints the same facts.

## Questions

**Questions** shows each question that waits for you, with the thread title. It shows approvals before other questions. You can approve a plugin install. You also approve when you delete a thread. An install card selects **Cancel** first, so an accidental Enter does not install.

If the right sidebar does not show, its toggle shows the number of questions. A question that only you can answer opens the sidebar over the center.

## Command list

Press Cmd+Shift+A, click the search icon on the rail, or run `kg window command-list`.

- Type to filter the list. Each row shows its key.
- Press Enter to run the selected command.
- A command that cannot run now is gray and tells you why.

**Search threads…** (Cmd+Alt+F) finds threads by title and terminal text. Press Enter to open the thread at the line that matches. After an archived thread's 10 seconds end, search finds it only by title. `kg thread search` gives the same results.

## Settings

Press Cmd+, or click the gear at the bottom of the rail. Settings has Appearance, Agents, Keyboard, and Plugins. Press Escape or click **Close** to close Settings.

## Quit

Quit ends all programs. If a program runs, kagan asks first. Close the window instead to keep the programs.

On macOS, Quit is Cmd+Q and Close is Cmd+W. Click the Dock icon to open the window again.
