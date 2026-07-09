# Lens: Spec alignment

Scope: `.specs/` vs actual behavior in `src/`.

Read `.specs/README.md` first — it defines the authority order: `requirements.md` (numbered
R-criteria) wins over `design.md`, which wins over `mental-model.md`.

Method: work from the spec toward the code. Sweep every numbered requirement shallowly (grep for
the implementing code) rather than deep-reading a few files; deepen only where the trace looks
wrong. Cite the requirement number in every finding that involves one.

## Checks

1. **Contradiction** — code behavior that violates a numbered requirement. Highest-value finding
   this lens can produce; verify the trace before reporting.
2. **Spec drift** — user-visible behavior in `src/` (commands, gates, lifecycle transitions,
   settings) with no corresponding requirement: the spec fell behind the code.
3. **Design claims** — `design.md` statements about architecture, the metadata model, or key flows
   that are no longer true of the code.
4. **Anti-goal violations** — features that contradict `mental-model.md`'s intent or anti-goals,
   or resurrect ideas it records as already evaluated and rejected.
