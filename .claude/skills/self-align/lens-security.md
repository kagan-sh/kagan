# Lens: Security

Scope: `src/`, `scripts/`.

Threat model: Kagan is a local, single-user dev tool that spawns git worktrees and AI agent
sessions. What matters is untrusted or agent-generated content (task titles, descriptions, branch
names, agent output) reaching a shell, git, the filesystem, or another agent's prompt — and
destructive operations running without their intended guard. Not in scope: network hardening,
multi-tenant isolation, or checklist items with no local attack path. Report only findings with a
concrete trigger: name the input, the sink, and the path between them.

## Checks

1. **Command construction** — `src/git.ts`, `src/check.ts`, `scripts/`: untrusted strings
   interpolated into shell commands, or passed as git arguments where a value like `--force` or
   `--upload-pack=...` would be parsed as a flag (missing `--` separation, unvalidated branch
   names).
2. **Path handling** — worktree or file paths derived from task/user input that can escape the
   intended root (`../`, absolute paths, symlinks).
3. **Prompt injection** — user-typed task text is trusted input by design; do not flag it. Flag
   only agent-generated content (task output, diffs, findings) flowing into another agent's prompt
   (`handoff.ts`, `intake.ts`, `validator.ts`) where it could override the supervisor's
   instructions or forge its output format.
4. **Destructive git operations** — merge, reset, branch/worktree deletion, or send-back flows
   that can destroy uncommitted user work without the guard the spec intends (cross-check
   `.specs/` for the intended gate).
5. **Secret leakage** — tokens or environment values written into session metadata, logs, error
   messages, or files that get committed.
6. **Install/setup writes** — `scripts/install-plugin.ts`, `scripts/setup.mjs`: writes outside the
   expected target directory, or silent overwrite of user files.
