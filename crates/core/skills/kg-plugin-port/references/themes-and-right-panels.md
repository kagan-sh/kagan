# Themes and right panels

## Themes

A theme file is JSON. CSS does not load.

The file lists both modes and the tokens in `docs/look.md`. Colors are opaque `#RRGGBB`. `kg theme new <id>` writes one to copy.

Copy the palette into `themes/<id>.json` and declare `"themes": [{ "id": "<id>", "file": "themes/<id>.json" }]`. The id the person sets is `<plugin id>/<id>`. A stylesheet that only recolors the app becomes that file. One that also changes layout or syntax colors is rewritten by hand, or reported as a gap.

Styles inside a plugin screen are View calls (`div`, `v_flex`, gaps) that use the same tokens. Rewrite those rules into the script.

## Right panels

A thread side panel maps to `"placement": "right"`: a closable tab in the right sidebar, not a rail icon. Open it with a command, `openPanel`, `kg surface open`, or the right tab bar `+`. Questions, Changes, and Details stay the built-in tabs.

That placement needs right panel support. Run `kg plugin check` with `"right"`. If it refuses with `panel "<id>": placement "right" is not supported. Use "left" or "center".`, use `left` or `center` and mark the row not supported yet. An already installed plugin that still says `right` still starts, and that panel is skipped. Do not claim the right tab works until check accepts `"right"`.

`"bottom"` stays refused either way.
