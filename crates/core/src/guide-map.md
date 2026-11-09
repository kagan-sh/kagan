Command map (add --help to any command for its arguments):

mcp: `kg mcp`.

status: `kg status`.

guide: `kg guide`.

skill: `kg skill list`, `kg skill show`, `kg skill files`, `kg skill new`, `kg skill add`, `kg skill remove`.

open: `kg open`.

thread: `kg thread get`, `kg thread list`, `kg thread run`, `kg thread stop`, `kg thread input`, `kg thread resize`, `kg thread screen`, `kg thread scroll`, `kg thread select`, `kg thread copy`, `kg thread report`, `kg thread paste`, `kg thread send`, `kg thread terminal new`, `kg thread terminal list`, `kg thread terminal close`, `kg thread details`, `kg thread links`, `kg thread archive`, `kg thread unarchive`, `kg thread delete`, `kg thread set-pinned`, `kg thread rename`, `kg thread mark-read`, `kg thread metadata`, `kg thread set-metadata`, `kg thread events`, `kg thread wait-event`, `kg thread wait`, `kg thread ask`, `kg thread start-again`, `kg thread tools`, `kg thread call-tool`, `kg thread interactions`, `kg thread search`.

project: `kg project list`, `kg project get`, `kg project add`, `kg project remove`, `kg project reorder`.

program: `kg program list`.

environment: `kg environment get`, `kg environment restore`, `kg environment branches`, `kg environment setup`, `kg environment set-base`, `kg environment remove`.

plugin: `kg plugin cancel-install`, `kg plugin start`, `kg plugin command`, `kg plugin new`, `kg plugin check`, `kg plugin share`, `kg plugin reload`, `kg plugin dev`, `kg plugin logs`, `kg plugin types`, `kg plugin list`, `kg plugin remove`, `kg plugin install`, `kg plugin update`, `kg plugin index add`, `kg plugin index list`, `kg plugin index remove`, `kg plugin search`, `kg plugin enable`, `kg plugin disable`, `kg plugin config get`, `kg plugin config set`, `kg plugin config clear`, `kg plugin use`.

sidebar: `kg sidebar`.

section: `kg section create`, `kg section rename`, `kg section delete`, `kg section file`.

theme: `kg theme get`, `kg theme set`, `kg theme new`, `kg theme folder`.

settings: `kg settings open`, `kg settings plugins`.

window: `kg window about`, `kg window hide`, `kg window hide-others`, `kg window show-all`, `kg window quit`, `kg window minimize`, `kg window zoom`, `kg window close`, `kg window next-thread`, `kg window previous-thread`, `kg window thread`, `kg window next-thread-needing-you`, `kg window toggle-review-wrap`, `kg window command-list`, `kg window search-threads`, `kg window next-terminal`, `kg window previous-terminal`, `kg window open-local-link`.

surface: `kg surface list`, `kg surface threads`, `kg surface open`, `kg surface close`.

keymap: `kg keymap get`, `kg keymap set`, `kg keymap remove`.

preference: `kg preference get`, `kg preference set`.

ui: `kg ui snapshot`, `kg ui click`, `kg ui type`, `kg ui key`, `kg ui scroll`, `kg ui drag`, `kg ui drop`, `kg ui screenshot`.

browser: `kg browser open`, `kg browser status`, `kg browser snapshot`, `kg browser goto`, `kg browser back`, `kg browser forward`, `kg browser reload`, `kg browser click`, `kg browser fill`, `kg browser type`, `kg browser press`, `kg browser select`, `kg browser scroll`, `kg browser close`.

diff: `kg diff list`, `kg diff show`, `kg diff commits`.

preview: `kg preview show` prints one file with the same rules as the Preview tab.

diagnostics: `kg diagnostics export`.

update: `kg update check` reads the GitHub release list now; `kg update status` prints the version, the install kind and what the last check found. Only a person installs, from the window.

Preferences: `confirm-archive`, `agent-integration`, `notify`, `plugin-status`, `update-check` (off by default; on checks at launch and daily), `terminal-font-size`, `review-viewed`, `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width`, `<window>.review-wrap`, `slot.threads`, and `slot.review` (a plugin id, or `kagan` for the built-in; set from the window, or with `kg plugin use`). For example, `kg preference set notify false`.

Examples: `kg project list`, `kg thread get`, `kg thread run --program codex`, and `kg plugin list`.
