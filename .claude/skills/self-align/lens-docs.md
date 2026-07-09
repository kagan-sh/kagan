# Lens: Docs freshness

Scope: `README.md`, `docs/` (VitePress site), `CONTRIBUTING.md`, `AGENTS.md`, and
`.claude/skills/*/SKILL.md`.

Every finding must pair the doc line with the contradicting fact in code or a manifest
(`package.json`, lock-adjacent pins) — "feels stale" is not a finding.

## Checks

1. **Commands** — every documented command (`bun run ...`, install steps, CLI invocations) exists
   in `package.json` scripts or the actual tool, and its described behavior matches. Verify by
   reading, not by running.
2. **Paths and names** — referenced files, directories, exports, and keybindings exist as stated.
   Check the AGENTS.md Map against the real contents of `src/`.
3. **Behavior claims** — described flows (install, quickstart, lifecycle, gates, board
   interactions) match what the code does; flag any doc statement `src/` contradicts.
4. **Version and pin claims** — versions stated in docs match the manifests they describe.
5. **Unproven claims** — capabilities described but not implemented, superlatives with nothing
   behind them, numbers with no source.
6. **Coverage gaps** — user-visible features (commands, settings, keybindings) absent from user
   docs entirely.
7. **Agent-guide accuracy** — AGENTS.md instructions an agent would follow that now fail or
   mislead; this file steers every agent session, so staleness here compounds.
