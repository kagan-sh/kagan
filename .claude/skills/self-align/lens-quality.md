# Lens: Code quality & YAGNI

Scope: `src/`, `test/`, `scripts/`.

The bar is AGENTS.md "Style and discipline" — read it before auditing; apply it, not generic
best practice. The codebase is deliberately strict-YAGNI: unused flexibility is a defect, not a
nicety.

## Checks

1. **Speculative generality** — options, parameters, branches, or config knobs with no current
   caller or setter (grep call sites before flagging); abstraction layers with a single
   implementation; extensibility hooks nothing uses.
2. **Dead code** — unused exports, unreachable branches, tests pinning deleted behavior. Grep for
   callers before flagging; same-file usage counts as usage.
3. **Redundant guards** — branches re-checking what a schema or an earlier guard already
   guarantees.
4. **Duplication** — near-identical logic or user-facing messages for indistinguishable cases that
   should share one path.
5. **Compat residue** — shims, fallbacks, adapters, or commented-out legacy kept after a
   replacement landed.
6. **Idiom drift** — ad hoc `session.metadata.kagan` reads instead of `task.ts`'s `kagan()` parsed
   view; metadata writes bypassing `patchKagan`/`tuiPatchKagan`; blocking `prompt` instead of
   `promptAsync`; `api.ui.toast` from board code instead of `store.notify`; shapes that ignore the
   established `DialogSelect`/`DialogPrompt` and mock-`PluginInput` patterns.
7. **Comment noise** — comments narrating what code does; missing comments where the code alone
   would make a reader mispredict behavior (external constraint, deliberate deviation).
8. **Test depth mismatch** — dedicated suites for thin glue; pure logic without direct tests;
   assertions that cannot fail meaningfully.
9. **Unnecessary complexity** — cleverness where straight-line code does the same job; state or
   indirection a reader must hold in their head without payoff.

## Skip

- Formatting — prettier is config-as-law.
- Anything oxlint or tsc would report.
- The `.tsx`-extension rule for Solid/OpenTUI imports — a test gate already enforces it.
- Style preferences not grounded in AGENTS.md.
