# Your data

kagan stores its data on your computer in `~/.kagan`, or `%USERPROFILE%\.kagan` on Windows. Set `KG_DATA_DIR` to use another folder. The SQLite database is `kagan.sqlite3` in that folder. It holds projects, threads and their events, and saved terminal screens. A settled screen and its scrollback are kept up to 4 MiB in total; older scrollback is left out when it does not fit. See [Threads](threads.md) for when a screen is saved and other limits.

kagan does not upgrade a database made by a pre-release build, and does not open a SQLite file another program made. It shows a window that names the file, changes nothing, and quits. Move that file, and any `-wal` or `-shm` file beside it, out of the folder to start fresh.

Git worktrees created for threads are in the data folder's `worktrees` directory. Personal thread folders, plugin data and installed skills also live in the data folder. kagan has no account and no telemetry.

Installing or updating a git plugin uses the network to fetch its repository. Agents and other programs you run may use the network themselves, and plugins can request network access. To find installed known agents and their versions, kagan runs each one it finds with `--version` before you start a thread.

A program in a thread receives `KG_THREAD_ID`, `KG_PROJECT_ID`, `KG_CLI`, and, when available, `KG_ENVIRONMENT_ID` and `KG_SOCKET`. It inherits the app's environment, which can include `KG_DATA_DIR`. The socket lets it use `kg` to control kagan: for example, it can start, stop or archive threads and operate the window. It cannot answer approvals reserved for a person. A program also has the ordinary access of the user running it; choose programs you trust.

If `LC_ALL`, `LC_CTYPE`, and `LANG` do not name a UTF-8 locale, kagan sets `LANG` to an available UTF-8 locale. A non-UTF-8 `LC_ALL` or `LC_CTYPE` still takes precedence over `LANG`.

With agent integration on, kagan starts Claude with `--mcp-config`, `--plugin-dir`, and `--append-system-prompt-file`. It starts Codex with `-c` overrides for `mcp_servers.kagan.command`, `mcp_servers.kagan.args`, and, when available, `mcp_servers.kagan.env`. Claude subcommands do not get these extra arguments. You can turn agent integration off in Settings; the `KG_*` variables and `kg` on `PATH` remain.

A plugin allowed to run git can use your git credentials, including SSH keys, your credential helper, and a GitHub token. Review the programs a plugin asks to run before approving it.
