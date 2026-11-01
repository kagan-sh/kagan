# The window

kagan has one window on macOS, Windows, and Linux. From left to right, the window has these parts:

- **Rail**: a column of icons at the left edge.
- **Left sidebar**: your projects and their threads.
- **Center**: the terminal of the thread that you selected.
- **Right sidebar**: Questions, Changes, and Details.

On a narrow window, the right sidebar hides first, then the left sidebar. The rail always stays. To show a hidden sidebar over the center, press its key (Cmd+B for the left, Cmd+Backslash for the right). Press Escape to close it.

## Left sidebar

The left sidebar shows each project and its threads.

- To start a thread in a project, click **+** next to the project.
- To add a project, click **Add project** at the bottom.
- To remove a project, right-click it and select **Remove project…**. Stop its programs first. kagan forgets the project and its threads. Your folder stays. You cannot remove Personal.

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

To find archived threads, click **Archived** at the bottom of the list. **Restore** brings a thread back. Its program stays stopped.

## Rail

From top to bottom, the rail has:

- The kagan mark. It opens **About kagan** and **Settings…**.
- **Threads**, one icon for each plugin tool and each plugin page, **Manage plugins**, and **New plugin…**.
- The command list and Settings, at the bottom.

A plugin icon opens its tool below the threads, or its page in the center. Click the icon again, or **Close**, to go back. **New plugin…** starts a Personal thread with your first installed agent and a prompt to make a plugin.

## Thread

The center shows the terminal of the selected thread. The terminal has scrollback, mouse support, and copy and paste.

The menu at the right of the thread header has **Pin**, **Archive**, **Rename…**, and **Save diagnostics**. When the program ends, the line under the terminal tells you why, and has **Start again**.

To open a shell in the thread's folder, click **+** in the thread header, or press Cmd+T. Tabs appear over the terminal: the program first, then **Terminal 1**, **Terminal 2**, and so on. Click a tab, or press Cmd+Alt+] and Cmd+Alt+[, to switch. Click **×** on a tab, press Cmd+Shift+W, or type `exit` to close it. `kg thread terminal new`, `list`, and `close` do the same from a terminal, and `--terminal <n>` sends `kg thread input` and the other terminal commands to one of them.

To open a link that a terminal printed, hold Cmd (Ctrl on Linux and Windows) and click it. When a program prints a local address, such as `http://localhost:3000`, **Open localhost:3000** appears over the terminal, at the right. Click it, or press Cmd+Alt+O, to open it in your browser. `kg thread links` lists the links that the thread's terminals printed.

Extra terminals are not part of the thread: they never mark it as busy or needing you. Archive and Quit end them. They do not come back after kagan restarts.

## Review changes

**Changes** in the right sidebar lists the files that the thread changed.

1. Click a file. The review opens in the center.
2. Select a line, or drag across line numbers, and type a comment.
3. Click **Send review to terminal**, or press Cmd+Enter. kagan pastes your comments into the terminal but does not press Enter, so you can edit them first.

**Discard comments** deletes the comments that you did not send. In the review, use these keys:

| Key | Action |
| --- | --- |
| J, K | Next file, previous file |
| N, P | Next change, previous change |
| V | Mark the file as viewed |
| Escape | Go back to the terminal |

## Details

**Details** in the right sidebar shows where the selected thread works: its folder (click the copy icon to copy the path), the kind of folder, and the program. In a git folder it also shows the branch, how many commits the branch is ahead of and behind its base branch, and the last five commits. kagan reads this from your folder every few seconds. It never fetches and never talks to GitHub. `kg thread details` prints the same facts.

## Questions

**Questions** in the right sidebar shows each question that waits for you, with the thread title. Approvals come first, such as a plugin install or a thread delete. An install card selects **Cancel** first, so that an Enter by accident does not install.

If the right sidebar is hidden, its toggle shows the number of questions that wait. A question that only a person can answer opens the sidebar over the center.

## Command list

The command list runs any command by name. Press Cmd+Shift+A, click the search icon on the rail, or run `kg window command-list`.

- Type to filter the list. Each row shows its key.
- Press Enter to run the selected command.
- A command that cannot run now is gray, and tells you why.

**Search threads…** (Cmd+Alt+F) finds threads by title and by their terminal text. Press Enter to open the thread at the line that matched. After an archived thread's 10 seconds end, search finds it only by title. `kg thread search` gives the same results.

## Settings

Press Cmd+, or click the gear at the bottom of the rail. Settings has these sections: Appearance, Agents, Keyboard, and Plugins. To close Settings, press Escape or click **Close**.

## Quit

Quit ends all programs that run. If a program runs, kagan asks first. To keep the programs running, close the window instead. On macOS, Quit is Cmd+Q and Close is Cmd+W. Click the Dock icon to open the window again.

## Keys

On Linux and Windows, Cmd in this book means Ctrl+Shift. For example, Cmd+N is Ctrl+Shift+N. Quit (Cmd+Q) and Close (Cmd+W) are only on macOS. In the terminal on Linux and Windows, Ctrl+Shift+C copies and Ctrl+Shift+V pastes. Ctrl alone always goes to the terminal program. On macOS, Alt is the Option key.

| Key | Action |
| --- | --- |
| Cmd+N | New thread |
| Cmd+. | Stop thread |
| Cmd+Shift+S | Start again |
| Cmd+Shift+O | Add project |
| Cmd+Shift+Backspace | Archive thread |
| Cmd+Shift+P | Pin or unpin thread |
| Cmd+1 to Cmd+9 | Go to thread 1 to 9 |
| Cmd+Shift+] and Cmd+Shift+[ | Next thread, previous thread |
| Cmd+T | New terminal in the thread's folder |
| Cmd+Alt+] and Cmd+Alt+[ | Next terminal tab, previous terminal tab |
| Cmd+Shift+W | Close the shown extra terminal |
| Cmd+Alt+O | Open the newest local link, such as localhost:3000 |
| Cmd+click | Open the link under the pointer |
| Cmd+Alt+J | Next thread that needs you |
| Cmd+Shift+L | Move the keyboard focus to the left sidebar |
| Cmd+B and Cmd+Backslash | Show or hide the left or right sidebar |
| Cmd+Shift+A | Command list |
| Cmd+Alt+F | Search threads |
| Cmd+Enter | Send review |
| Cmd+, | Settings |
| Cmd+= and Cmd+- | Bigger or smaller terminal text |

In the left sidebar, the Up and Down keys move between rows. Enter or Space opens a thread. In the terminal, the Up and Down keys go to the program.

To change a key, open Settings → Keyboard and click **Change** or **Bind**, then press the new key. A key must use Cmd, Alt, Ctrl+Shift, or an F key, because the terminal needs plain keys. A key that a different command uses is not saved. `kg keymap` does the same from a terminal.
