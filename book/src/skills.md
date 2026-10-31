# Skills

A skill is a folder with a `SKILL.md` file. The top of the file is YAML with `name` and `description`. The name is lowercase and hyphenated, and it matches the folder name. The rest is instructions for the agent.

Four places supply skills. A later place overrides an earlier one with the same name:

| Place | Where |
| --- | --- |
| Bundled | Inside the `kg` binary |
| Plugin | An enabled plugin's manifest |
| User | `<dataDir>/skills` |
| Project | `<project>/.kagan/skills` |

Two skills with the same name in one place are both dropped. `kg skill list` shows that error.

The agent does not see the instructions until it asks. The tool list names each skill and its description. The agent loads one with the `skill_load` tool. `kg skill files <name>` prints the same paths, so the agent can read them with its own file tools.

```bash
kg skill list
kg skill show kg-plugin-authoring
kg skill files kg-plugin-authoring
kg skill new my-skill
kg skill new my-skill --project
kg skill add <path>
kg skill add <path> --project
kg skill remove my-skill
```

`new` writes a folder into your user skills, or into the project when you pass `--project`. `add` copies a folder you already have. `remove` deletes a user or project skill. Bundled skills and plugin skills stay.

Four skills ship in the binary:

- `kg-cli` for threads, projects, and environments
- `kg-plugin-authoring` for building a plugin
- `kg-skill-creator` for writing a skill
- `kg-ui` for driving the open window without the operating system's mouse or keyboard

`kg guide skills` prints the same rules.
