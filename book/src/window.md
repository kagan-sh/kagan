# The window

kagan opens one window on macOS, Windows, and Linux. It is drawn by GPUI, with no web page inside it. Starting `kg` opens the window.

The window has a title bar, a rail of icons at the left edge, a left sidebar, the thread in the center, and a right sidebar. The center keeps at least 480 points beside the 48-point rail, so the window is at least 528 points wide. When the sidebars would leave less room, the right sidebar hides first, then the left, and comes back when the window is wide enough. When the left sidebar has no room, Show sidebar, ⌘B, and ⇧⌘L draw it over the center. Escape, a click on the dimmed center, or picking a thread closes it. The rail stays on screen at every width. Drag either sidebar to resize it. A button or a shortcut hides it.

## Left sidebar

The sidebar lists projects, and under each project its threads. A disclosure hides or shows a project's threads, and the project name selects that project and shows its New thread form. Enter on a focused project starts the picked program there. + starts a thread in it. Add project is the last row. Right-click a project and choose Remove project… to forget it and all its threads after confirmation. Stop its programs first. The project folder stays; git may keep a worktree if it has files that would be lost. Personal cannot be removed.

Each thread has a mark. It spins while the thread is busy, rings in amber when it needs you, and fills red when it failed. A thread that finishes while it is not shown gets a filled square until you show it. A question or failure keeps its own mark. A live idle thread shows a small muted dot. A program that has ended shows a small hollow muted square, including when it exited with code 0, and only while that thread is idle and does not need you. A pinned thread also shows a pin. Archived is an icon in the list footer. It appears only while a visible thread is archived, and it opens a searchable list. Restore brings a thread back, and the program stays stopped.

The rail holds the kagan mark, Threads, one icon per plugin with a left tool, one icon per center page below them, Manage plugins, and New plugin…. The command list and Settings sit at the bottom of the rail. A page's icon carries the page's title as its tooltip. Choosing it opens the page in the center. Choosing that icon again, or Close, returns to the terminal. Choosing a tool's icon shows its body in the list column, below the threads. Choosing it again closes the body. Manage plugins opens Settings → Installed. New plugin… starts a Personal thread titled New plugin with the first installed agent. If no agent is installed, the window says so and starts nothing. Its prompt is pasted without Enter after the screen settles. If it does not settle within 15 seconds, Copy offers the prompt instead. A page replaces the center: a title, Close, then the plugin. Close returns to the terminal. The selected tool is drawn with a fill.

## Thread

+ opens the program line, titled New thread. Type the program you want, such as `claude`, then press Enter. Choosing a thread shows its terminal. You type into that program. The terminal keeps scrollback, the mouse, and copy and paste. Stop ends the program.

Rename, Pin, and Archive are in the menu at the right of the thread header. Rename edits the title in place: Enter keeps it, Escape leaves it, and clicking the title does not start a rename. The program may set the title once. A later attempt says the title was kept. A name you set in the window stays.

When the program has ended, the line under the terminal says why: Stopped, Exited with code N, Ended by signal N, Ended when kagan quit, or Stopped while starting. Any other failure shows its error. Start again is on that line. It runs the same program again in the same folder. Nothing starts it on its own. Save diagnostics is in the thread actions menu, and on that line when the program failed, was interrupted, or the screen was not kept in full. The file has no terminal contents and no environment variables.

## Command list

The command list runs any command by name. Open it with ⇧⌘A (Ctrl+Shift+A on Linux and Windows), the search icon above the gear, or `kg window command-list`. It lists every command, grouped Threads, New thread, Review, Window, Settings and Plugins, then each enabled plugin's commands under the plugin's name. Each row shows the key that runs it, including a key you changed and a plugin's key. Type to filter. Enter runs the highlighted command where you were, as if you had pressed its key there. A command that cannot run there is greyed, and says why, such as "Open a thread first." Escape, even with text typed, or a click outside closes the list and puts the keyboard back where it was.

Search threads… is a row in the list, and has its own key, ⌥⌘F (Ctrl+Shift+Alt+F on Linux and Windows). It finds threads by title and by the text their terminal kept: the scrollback of a running program, or the screen kept after it ended. Case does not matter, and colors and other terminal codes are ignored. Threads show first and archived threads under Archived; hidden threads are not shown. Each row shows the title, the project, and the line that matched. Enter opens the thread and scrolls its terminal to that line. An archived thread keeps its terminal text only for its 10 second grace, so after that it is found by title only. `kg thread search` gives the same results to a script or an agent.

## Right sidebar

The right sidebar stacks Questions, then Changes. A plugin is a tool or a page on the left, not a panel on the right.

Questions lists every question that is waiting, with the thread's title, and approvals first. A question shows its options there. An approval, such as installing a plugin or deleting a thread, is there too. The install card focuses Cancel, so a stray Enter does not install. While a git address downloads, Settings shows Cancel, which stops the download. The amber ring on a thread still means that thread needs you.

Changes lists the files the shown thread changed. Open a file to review it in the center, choose a scope, and use the file tree or its filter to move between files. Expand unchanged lines and mark files Viewed as you read. Select a line or drag across line numbers to add a comment. Send review to terminal pastes the notes into the thread’s terminal without submitting them, so you can edit first. Comment line breaks are flattened to avoid submitting lines to a shell. Discard comments clears the saved review comments. Cmd+Enter sends the review (Ctrl+Shift+Enter off macOS). Use j and k for files, n and p for changes, v for Viewed, and Escape to return to the terminal.

When the window hides the right sidebar to keep the center at 480, the right toggle says hidden. While something is waiting it also shows how many. A person-only question opens those panels over the center by itself, with Questions shown, and focus stays where it is. With the panels hidden and the shown thread changed, the toggle also says how many files changed, and its tooltip says so. Pressing it draws those same panels over the center's right edge, 320 points wide. That drawing does not change the saved choice to show the sidebar. Opening either side overlay closes the other; menus can still open over it. Escape, or a click outside the panels, closes it and returns focus to the previous control when available. Opening a review also closes it. With no thread open and nothing waiting, the toggle is disabled, and its tooltip says why.

When a thread needs you and that thread is not the one on screen, kagan posts a system notification with the thread's title. `kg preference set notify false` turns that off. It stays on when the preference is unset. The Notifications switch in Settings → Agents turns it off too.

## Settings

The kagan mark at the top of the rail opens a small menu with About kagan and Settings…. Tab reaches it, and Enter or Space opens it. Escape closes the menu and puts the keyboard back on the mark. With no thread open, a large faded mark sits above the welcome; it is only a picture.

Settings takes over the window. Open it with the gear at the bottom of the rail, the kagan mark's menu, the shortcut, or `kg settings open`. Both sidebars hide. One page scrolls through Appearance, Agents, Keyboard, and Plugins, in that order. There is no search and no section menu. Close, Escape, the gear, or picking a thread leaves, and both sidebars come back.

## Menu bar

On macOS the menu bar is kagan, Edit, View, and Window.

kagan has About, Settings, Hide, Hide Others, Show All, and Quit. About shows the name and the version. Settings is the same page as the gear. Quit ends programs that are still running. When one is running, Quit asks first: “Quit kagan?” and how many programs will end. Quit is the default. Cancel leaves them running. With nothing running, Quit closes kagan at once. There is no setting to skip the question. `kg window quit` does not ask. The next time you open a thread that was running, it says it ended when kagan quit, and that no output was saved for that run.

Edit has Undo, Redo, Cut, Copy, Paste, and Select All. Copy and Paste in the terminal are the same as the terminal's own copy and paste.

View shows or hides the left sidebar and the right sidebar.

Window has Minimize, Zoom, and Close. Close closes the window and leaves programs running. Clicking the Dock icon opens the window again.

Quit is Cmd+Q, Settings is Cmd+,, Hide is Cmd+H, Hide Others is Cmd+Option+H, Minimize is Cmd+M, and Close is Cmd+W.

## Keyboard

Shortcuts use Command on macOS. On Linux and Windows they use Control and Shift together, such as Ctrl+Shift+N for New thread, so Ctrl alone always reaches the program. `FocusSidebar` moves the keyboard to the selected row, or the first row. Up and down move between sidebar rows and do not open them. Enter or Space on a thread opens it. Enter on a project starts the picked program there. Space on a project shows its New thread form. Up and down in the terminal still reach the program. Stop, Start again, Pin, Archive, Restore, New thread, Add project, the command list, and Search threads… have shortcuts too. Windows and Linux keep the same chords as each other.

Settings → Keyboard lists every shortcut, grouped by where it applies, then each enabled plugin's commands under the plugin's name. A row shows its key, or Not set. Change, or Bind on a row with no key, waits for the next key you press and saves it; Escape stops without saving. A plain key or a bare Ctrl key is not saved, because typing in the terminal needs it: use Command, Option or Alt, Ctrl and Shift, or an F key. Remove leaves a plugin command with no key. A key another shortcut already uses is not saved, and the row says which one uses it. When two commands want the same key, both rows say so. `kg keymap` does the same from a terminal: `kg keymap get` lists them, `kg keymap set <action> <key>` binds one, with `<plugin-id>:<command>` for a plugin's command, and `kg keymap remove <plugin-id>:<command>` clears one.
