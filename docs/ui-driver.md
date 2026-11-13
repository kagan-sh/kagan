# In-app UI driver (`kg ui`)

QA and agents drive the native kagan window without OS mouse or keyboard. Input goes through the window's own event dispatch (`Window::dispatch_event` / `dispatch_keystroke`). It works while the window stays in the background.

## Commands

- `kg ui snapshot [--window N]` — observed element tree: stable ids (from `ui_observe_as` for controls and `ui_container` for containers), role, label, value, bounds, focused, disabled, visible, hint. Plain text on stdout (one element per line, `hint=None` when it has none); `--json` returns `OpResult::ui`.
- `kg ui click <element-id> [--within <id>]` — left-click the element center. When two observed elements have the same id, the click is refused and names both paths; `--within <id>` picks the one inside the element with that id.
- `kg ui type <text>` — text to the current focus.
- `kg ui key <keystroke>` — key down/up (for example `escape`, `secondary-,`, `enter`).
- `kg ui scroll <element-id> <dy>` — wheel delta in pixels at the element. A negative `dy` scrolls down the page and a positive one scrolls up, so `kg ui scroll list -300` reads further down a list.
- `kg ui drag <from-id> <to-id> [--at top|middle|bottom] [--within <id>] [--hold]` — press the center of `from`, move past the drag threshold, move onto `to` in steps, and release. `--at` lets go in the top quarter, the middle, or the bottom quarter of `to` (default middle), where a plugin's `on_drop` reports that side. `--within` picks both elements inside the element with that id. `--hold` keeps the button down, so `snapshot` and `screenshot` show the drag in progress. If `to` is gone once the drag has started, the drag is cancelled, the button is released, and the command fails with `not-found`. While a drag is held, another `kg ui drag` fails.
- `kg ui drop` — release a held drag where the pointer is. With no drag held it fails: `no drag is held; start one with kg ui drag --hold`. `kg ui key escape` cancels a held drag instead.
- `kg ui screenshot <path> [--window N]` — PNG without focusing the window (macOS: `screencapture -l`, and a failure returns its reason as `screencapture failed: <reason>`, and a run that exits 0 without writing a file is a failure too (`screencapture wrote no image to <path>`). The capture goes to a new file beside the destination and replaces it only when that run wrote one, so an older image is never reported as the new capture; Windows: `PrintWindow`). A covered window is not presenting frames, so the capture is the last presented buffer.

`--window` is a zero-based index into kagan's own open windows (the main window and the About window), sorted by `WindowId`. A plugin's window is not counted. Default `0`.

## Offscreen windows

`kg --offscreen` starts the app with the same socket, data folder and commands as `kg --headless`, but its windows are never shown or brought forward, so a run takes no focus and is not tiled. Its windows are created with `show` and `focus` off. `kg ui screenshot` renders the window's own scene to a PNG on macOS, with no present and no frame wait. On other platforms it fails with the typed screenshot-unavailable error. Offscreen is proven on macOS only; Linux and Windows are not run yet. The capture is always `current`. Browser pages, web panels and VSCodium are native views outside the scene, so they do not appear in it [decision 238]. `scripts/smoke.sh` runs offscreen by default; `--visible` runs the window the person sees.

`kg ui` does not drive page content. A Browser page is driven with `kg browser`.

A screenshot's JSON is `{"capture": {"path": ..., "current": true|false}}`. `current` is false when the window did not draw a new frame before the capture (it was hidden, or no frame came within two seconds), so the image may show an earlier state. The plain output prints the path and, when `current` is false, a note on stderr.

## What `kg ui` can find

`kg ui` finds the controls kagan states with `ui_observe_as`. A snapshot lists elements top to bottom, then left to right; elements at the same position are listed by id, so a child that sits exactly on its parent may list before or after it. Its `Facts` are all required: role, name, value, disabled, selected and focus, so a control cannot leave one out. A container uses `ui_container` and states nothing. A control found this way reports its role, name, value, disabled, selected and focus. The thread header buttons, the title-bar sidebar toggles, the start button, the program rows and the thread actions menu state theirs. A plugin's `KgButton` states its own (see decision 55). A plugin's `KgInput` or `KgTextarea` with an `aria_label` states its name, value and focus; `kg ui click` uses that name and `kg ui type` goes to the focused field. A snapshot line also reports `selected`, which a control states itself. A plugin's plain `Button`, and any built-in control that has not stated its name, is not found. A plugin element that carries `on_drag`, `on_drag_move` or `on_drop` is found by its `id(...)` with its bounds, and reports no role, name or state. A web panel is found as a document named "Web panel", with the panel title as its value and the hint `kg ui cannot drive this page`, which the snapshot line shows as `hint=Some("kg ui cannot drive this page")`. Clicks, typing, and keys do not enter that page.

## Person-only approvals

Synthetic `kg ui` input sets a marker for the duration of dispatch. Person-only answer handlers ignore any click or key that carries it, and the Op returns `person-only approvals cannot be answered with kg ui`. Renaming a button does not bypass this. A person must answer in the app.

Only input that reaches an answer button is refused. An unrelated click or key succeeds and runs once while an approval waits, so a caller that gets success does not retry it. The approval stays pending either way.

## Requirements

The `kg` app must be running with a window open. If the UI driver is not installed yet, open a window once (`kg open`). Drive the instance `kg status --json` confirmed. Leave `KG_DATA_DIR` and `KG_SOCKET` unset. A command that starts a second headless app is the wrong window. `KG_NO_START=1` makes a client fail with `no kagan app is listening on <socket>; start it first` instead of starting one. The smoke gate (`scripts/smoke.sh`) is one exception: it sets every variable to a temporary folder, so it drives its own app and never yours. The Browser check (`scripts/browser.sh`) is the other, and CI runs it on Linux, macOS and Windows (decision 167).

`kg plugin install` waits until a person approves powers in the app. This driver cannot answer that. The Op returns `person-only approvals cannot be answered with kg ui`.
