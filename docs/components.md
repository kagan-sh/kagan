# Native components

`kg_ui::register_components(&mut ComponentRegistry)` registers KgButton,
KgInput, KgTextarea, WebView, ThreadTitle, ThreadStatus and Markdown.
The application builds one registry, registers the gpui-component catalog and
these components into it, and freezes it. `kg_plugins::init` accepts that
frozen catalog and retains it for each plugin runtime. The registry generates
their script declarations.

## Thread components

ThreadTitle and ThreadStatus take a core thread id and observe that thread
entity. Missing ids fail explicitly. Status uses the shared Workbench colors,
a 3 px bar and a visible status word with an accessible name.

Markdown takes a stable id and text, and uses GPUI Kit's native Markdown view.
All components receive the active Workbench theme. Runtime creation is tested
to preserve its projected background, foreground and primary colors.

## WebView

WebView takes one id. `url(url)` loads one http or https page and the element
fills its parent. The host must be `127.0.0.1`, `localhost`, or a host in that
plugin's `powers.fetch`. The URL rule is in `docs/plugin-api.md`.

## Web panel

A panel may name `"web"` instead of `"view"`. The body is an HTML file in the
plugin folder, drawn by the same macOS web view. The page, the bridge, and the
theme variables are in `docs/plugin-api.md` (decision 132).
