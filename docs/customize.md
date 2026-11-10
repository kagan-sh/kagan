Each change below is in the person's data folder (the `dataDir` in `kg status --json`), or is a command.

Themes: a theme is a JSON file at `<dataDir>/themes/<id>.json` with a `name`, a complete `light` palette, a complete `dark` palette, and optionally `look`. `kg theme new <id> [--from <theme>]` writes a copy there, `kg theme folder` prints the folder, and `kg theme set <id> <light|dark|system>` picks a theme and a mode. `fjord` is the default; `ember` and `grey` ship too. A plugin theme's id is `<plugin id>/<theme id>`.

Keys: `kg keymap get` lists them. `kg keymap set <action> <key>` rebinds one. To restore the default, run `kg keymap set <action> "" --reset`. The action is a name such as `ToggleSidebar`, or `<plugin id>:<command>` for a plugin command.

Preferences: `kg preference get <key>` reads one, and `kg preference set <key> <value>` saves one, for example `kg preference set notify false`. Keys: `confirm-archive`, `agent-integration`, `notify`, `plugin-status`, `update-check`, `terminal-font-size`, `review-viewed`, `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width`, `<window>.review-wrap`, `slot.threads`, and `slot.review`.

Agent rules: `<dataDir>/instructions.md` is appended to the agent note, after the line `Instructions from your kagan instructions file:`, when agent integration is on. It reaches `claude`, `codex`, `cursor-agent` and `pi`. `kimi` and `opencode` do not get it. Only its first 4096 bytes are used.

Skills: `kg skill new <name>` writes into `<dataDir>/skills`, and `--project` writes into `<project>/.kagan/skills`. `kg skill list` shows what loads and any error.
