---
name: kg-customize
description: Change how kagan looks or behaves, with its theme, keys, preferences, agent rules, or skill folders. Use when the person asks to change the theme or colors, rebind a key, turn a setting on or off, change a sidebar, or set a rule for every agent.
---

Read `kg guide customize` first. It has every command, key, and file.

1. Find which kind of change the person wants: a theme, a key, a preference, an agent rule, or a skill. Change only that.
2. For a theme, write a copy with `kg theme new <id>`, edit that file in the theme folder, and select it with `kg theme set <id> <light|dark|system>`. Do not edit `fjord`, `ember`, or `grey`; they ship with the app.
3. For a key or a preference, read the current value first with `kg keymap get` or `kg preference get <key>`, then set it.
4. For an agent rule, edit `<dataDir>/instructions.md`, and keep it under 4096 bytes. The data folder is `dataDir` in `kg status --json`.
5. For a skill, use `kg skill new <name>` or `kg skill add <path>`, then check it with `kg skill list`.
6. Read the value back, and tell the person what changed and where.
