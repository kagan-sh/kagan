---
name: kg-customize
description: Change how kagan looks or behaves, with its theme, keys, preferences, agent rules, or skill folders. Use when the person asks to change the theme or colors, rebind a key, turn a setting on or off, change a sidebar, or set a standing rule for agents.
---

Read `kg guide customize` first. It has every command, key, file, and limit.

1. Find which kind of change the person wants: a theme, a key, a preference, an agent rule, or a skill. Change only that.
2. For a theme, copy the theme in use with `kg theme new <id>`, edit the copy, then select it with `kg theme set`. Leave the built-in themes alone.
3. For a key or a preference, read the current value first, then set it.
4. For an agent rule, edit the instructions file in the data folder, and check the chapter for its size and which agents read it.
5. For a skill, create or add it with the `kg skill` commands, then check it with `kg skill list`.
6. Read the value back, and tell the person what changed and where.
