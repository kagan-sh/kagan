# Replacing GPUI with Rune

Read on 2026-11-06. [Decision 159](decisions.md) records the outcome: the window stays on GPUI.

Rune is [anomalyco/rune](https://github.com/anomalyco/rune), `dev` at `d03349e25189ce1edc8c26ada6bf6d4aac72c826` (committed 2026-11-06). This note uses that commit's README, `crates/rune/README.md`, `crates/rune-kit/README.md`, `WEB.md`, and `rust-toolchain.toml`. Nothing here was compiled, and no kagan view was ported. Frame times quoted from Rune are theirs, measured on their settings example.

## What would have to move

GPUI is the window, the process runtime, and the way a plugin draws. Four crates import it. A count of Rust files under `crates/` that mention `gpui::`, `gpui_kit::`, `gpui_shell::`, or `gpui_webview::`:

| Crate | Files |
|---|---|
| `ui` | 198 |
| `core` | 67 |
| `plugins` | 45 |
| `kg` | 1 |

`domain`, `store`, `terminal`, and `process` do not. `kg-terminal` already keeps the pty and returns a `Grid` with no GPUI. The window paints that grid.

The pieces that would have to be replaced, in the order they block each other:

1. **The runtime.** `Workspace`, `Thread`, `Project`, `Environment`, and the updater are GPUI entities. `Workspace::open` takes `&mut App`. Decision 90 allows async only through `cx.spawn` and `cx.background_spawn`; SQLite runs on the background executor. 217 lines call one of those two. Rune's model is one state value, `fn view(s: &S, cx) -> El`, handlers between frames, `AppHandle::update` from another thread, and `rune::Latest` (at most four workers, newest answer kept). That covers a search box. It does not cover the pty reader, the grace timers, the updater, plugin jobs, or the database worker. A GPUI-free `Workspace` is the prerequisite, and it is most of `kg-core`. The window rewrite waits on it. The tests that park GPUI's executor move with it.

2. **The toolchain.** Rune pins Rust 1.99.1. Kagan pins 1.98.0 (`rust-toolchain.toml`). A Rune crate cannot join this workspace until that pin moves, and this spike did not rebuild the pinned `gpui-kit` on 1.99.1. While `core` still needs GPUI, both have to build on one compiler.

3. **The window.** `crates/ui` would be rewritten onto Rune's view. `rune-kit` supplies buttons, fields, menus, lists, toasts, and the other controls named in its README, in Rust, themed from JSON tokens. It does not supply the terminal grid, the review, the activity strip, or a plugin slot. Theme files today are the Workbench palette plus Zed syntax colors handed to GPUI Kit (decision 139). Those syntax colors would need a new paint path. Rune can decorate text per frame; this spike did not try that on a diff.

4. **The terminal, as drawing only.** `TerminalView` in `crates/ui/src/terminal.rs` paints cell backgrounds with quads and glyphs with shaped text runs. Rune's rects, text, and app-drawn input method (`App::on_ime`, `.ime_cursor()`) match that shape, including IME for an editor the app draws itself. Cell alignment, wide characters, selection, links, and a grid that updates about 30 times a second are unmeasured. A throwaway example fed a saved grid would prove the drawing. It would not construct `Workspace`, because that needs a GPUI `App`.

5. **Pages.** The Browser, a plugin `WebView`, and a web panel open through `gpui-webview`: WKWebView on macOS, WebView2 under GPUI's composition visual on Windows (decisions 117, 121, 131, 133, 137, 142, 143). At this rev, Rune's README and `rune-kit` README describe no WKWebView and no WebView2. `NativeSurface` takes CoreVideo frames for video on macOS. Until a page can sit under menus and popovers on both systems, the Browser and those plugin panels have nowhere to go. Linux already shows a sentence and no page (142).

6. **Plugin views.** A plugin is one QuickJS program. `ShellRuntime` in `crates/plugins` mounts an exported view with `mount_exported_view` or `mount_paint_only_view`, under a policy, on the shared UI thread, with gpui-shell's time budgets (decision 25). The catalog is gpui-kit's, plus the components kagan registers. Rune has no script runtime and no JavaScript component catalog. The shell that would have to grow a Rune element tree lives in the `kagan-sh/gpui-kit` fork, not in this repository. Keeping those views on GPUI while the host window moves to Rune means two window loops in one process. Dropping them and keeping web panels only deletes the view contract in [plugin-api.md](plugin-api.md): panels, overlays, `KgButton`, and `kg ui` inside a plugin view.

7. **`kg ui`.** Snapshot, click, type, key, scroll, and screenshot dispatch through the GPUI window. Synthetic input must leave a person-only question unanswered (decision 10). Rune's headless runner is the part worth wanting: the same renderer, a fixed clock, a PNG, an element tree, and a script, and `--snap` needs no display. It is a different protocol from the socket `kg ui` already is. Decision 156's smoke opens a real window; a headless PNG does not replace that run.

8. **The phone.** Decision 37 waits until GPUI runs on the phone, then moves the iOS client. This Rune rev says iOS is not wired (winit owns the scene delegate), and its hot-reload path does not support a physical iOS device. Android is outside that README. A swap does not move the phone.

`domain`, `store`, the pty, `process`, and the `Op` list stay where they are. The window is already a client of `Workspace::grid` and `dispatch`. That split is what makes a later port thinkable. It is not the port.

## Why a trial cannot sit inside `kg`

GPUI runs its own application loop on the main thread. Rune runs a winit loop on the main thread (`rune::run` / `App::run`). Rune describes no way to draw inside another toolkit's window. `NativeSurface` is a texture slot for video frames. So this process cannot grow a Rune window beside `Workspace`.

The smallest drawing trial is a separate example, on Rust 1.99.1, painting a recorded grid. It answers cell drawing. It leaves the runtime, the page host, and plugin views untouched.

## What Rune already is

Useful where it matches, so a later reader does not re-survey the same commit:

- macOS, Windows, Linux, and an experimental web canvas (WebGPU, one canvas). `WEB.md` at this rev: no extra windows, no native menus, no drag-out, no web audio or video on that target.
- Headless checks with a virtual clock. Their settings example, headless at 2× on Apple silicon: mean frame CPU 0.13 ms, p95 0.29 ms, over 180 frames. That is not a terminal grid.
- A process that outlives its window (`keep_running`, `windowless`), Dock reopen on macOS, file dialogs, drag in and out, pop-up menus, an accessibility tree, and virtual rows.
- Video playback and recording on macOS only, in that README. Other platforms fail the clip.
- Pre-release. The install line is a git rev of `dev`, which they say is never rewritten. They do not take pull requests: an issue, then their own agents. Kagan's GPUI pin is a fork we rebase (`kagan-sh/gpui-kit`). A missing page host or a text bug in Rune waits on an issue, and `dev` moved the same day as this note.

## If the question is reopened

Do it in this order. Stop when a step has nowhere to land.

1. A separate example that paints a saved grid and takes keys. Drawing only.
2. `Workspace` constructed without a GPUI `App`, with the same parked tests decision 90 relies on. Start the window rewrite only after this exists. It is worth doing on its own even if the window stays.
3. A page host on macOS and Windows that menus and popovers can cover, or an explicit cut of the Browser, `WebView`, and web panels.
4. A JavaScript view host on the new element tree, or an explicit cut of native plugin views.
5. Then `crates/ui`, the `kg ui` driver, and the window tests.

Reopen decision 159 only when the page host, the plugin view host, and a deterministic executor are all real. Until then the window stays on GPUI (decision 47).
