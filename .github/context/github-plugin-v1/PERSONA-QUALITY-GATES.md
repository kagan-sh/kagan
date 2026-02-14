# Persona Quality Gates (Alpha Pragmatic)

This file converts the 5-elite-persona lens into concrete V1 gates for this initiative.
It is intentionally strict on product clarity and safety, and intentionally light on heavyweight platform process.

## Dag Lens (Terminal Infra)
- Keep rendering changes local to required GitHub UX.
- No full-screen redraw loops introduced by sync status updates.
- Validate core GitHub flows in:
  - Terminal.app
  - iTerm2 or WezTerm
  - tmux session
- Record terminal-specific quirks in runbook docs.

## Mira Lens (CLI DX)
- `kagan plugins` and future GitHub commands must provide:
  - clear help text
  - deterministic exit behavior
  - actionable errors with next-step hints
- Guard startup impact:
  - do not add import-heavy module loads to root CLI path
  - keep GitHub-specific imports localized to GitHub command/plugin paths

## Tomas Lens (Python Runtime)
- Prefer stdlib over new dependencies for V1.
- Keep interfaces typed at plugin boundaries.
- Avoid abstraction layers that do not reduce current complexity.
- Keep default call path simple: runtime handler -> adapter -> existing core services.
- Subprocess safety: argv-only invocation, validated identifiers, and redaction of secret-like values in logs/errors.
- Tests must validate user-observable behavior, not internal tautologies.
- Prefer extending existing tests over adding near-duplicate wiring tests.

## Yuki Lens (TUI Architecture)
- Connected-repo mode must be visually explicit.
- REVIEW gate and lease conflicts must be obvious and recoverable.
- Keyboard-first UX:
  - primary GitHub actions reachable via existing command/action flows
  - no hidden mandatory path behind mouse-only interactions
- All async operations (sync/reconcile/connect) must preserve interaction responsiveness.

## Rafael Lens (Platform Architecture)
- Official plugin is bundled and versioned with core for V1.
- Third-party plugins remain opt-in and policy-gated.
- Plugin/tool contracts must be stable once V1 ships:
  - method names
  - required params
  - error codes
- Freeze public contracts, not private helper/module/class shapes.
- Failure isolation:
  - one plugin operation failure must not destabilize core host.

## Explicit Non-Goals for V1
- No marketplace/registry rollout.
- No webhook service/control-plane.
- No generalized distributed lock service beyond label/comment lease model.
- No large plugin framework refactor unrelated to GitHub MVP outcomes.
