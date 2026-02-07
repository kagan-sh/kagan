# CHANGELOG

<!-- version list -->

## v0.3.0 (2026-02-07)


## v0.3.0-beta.6 (2026-02-07)

### Bug Fixes

- Add additional is_mounted guards in async worker methods
  ([#18](https://github.com/aorumbayev/kagan/pull/18),
  [`58c51ec`](https://github.com/aorumbayev/kagan/commit/58c51ec7e0692bd9f748373431e374654201de02))

- Add is_mounted guards to prevent database access during shutdown
  ([#18](https://github.com/aorumbayev/kagan/pull/18),
  [`58c51ec`](https://github.com/aorumbayev/kagan/commit/58c51ec7e0692bd9f748373431e374654201de02))

### Continuous Integration

- Race condition in async workers accessing database during widget unmount
  ([#18](https://github.com/aorumbayev/kagan/pull/18),
  [`58c51ec`](https://github.com/aorumbayev/kagan/commit/58c51ec7e0692bd9f748373431e374654201de02))

### Refactoring

- Replace OperationalError band-aids with ordered shutdown and lifecycle-aware DB sessions
  ([`ae9e3c5`](https://github.com/aorumbayev/kagan/commit/ae9e3c56bc5098a12136802a8cf714ed67b15b9e))


## v0.3.0-beta.5 (2026-02-07)

### Bug Fixes

- Harden review modal against ci race conditions
  ([`24a0674`](https://github.com/aorumbayev/kagan/commit/24a0674e7017e5e4f586f08573992db28981601f))


## v0.3.0-beta.4 (2026-02-07)

### Bug Fixes

- Stabilize enter review auto-start assertion in ci
  ([`c301968`](https://github.com/aorumbayev/kagan/commit/c301968302c77927aeac534791c6ccbf75a0fae9))


## v0.3.0-beta.3 (2026-02-07)

### Bug Fixes

- Prevent cd release hangs from snapshot tests
  ([`0bc6fa5`](https://github.com/aorumbayev/kagan/commit/0bc6fa5c3d7437598b22d5450325ac64277d40bf))


## v0.3.0-beta.2 (2026-02-07)

### Bug Fixes

- Port Windows compatibility, snapshot isolation, and CI hardening
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

- Resolve quality blockers and stabilize test suite
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

### Chores

- Windows tweaks
  ([`e8c1fe9`](https://github.com/aorumbayev/kagan/commit/e8c1fe9eb186b03eec17e7336a28de662f4991df))

### Documentation

- Cap Python to 3.12–3.13 and update all user-facing documentation
  ([`a873c6a`](https://github.com/aorumbayev/kagan/commit/a873c6a10c3447f4ae44cc3f7b5a746fae0564ec))

- Refresh documentation and remove outdated internal references
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

### Features

- Implement planner mode, pair flow, and ACP streaming UI
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

- Projects, multi-repos, workspaces, bug fixes, ui polishing, windows support
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

### Refactoring

- Centralize storage with XDG paths and schema evolution
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

- Consolidate git wrapper and modernize async patterns
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

- Remodel architecture with service-oriented patterns
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))

- **core**: Migrate to sqlmodel ORM and official ACP SDK
  ([#15](https://github.com/aorumbayev/kagan/pull/15),
  [`0602373`](https://github.com/aorumbayev/kagan/commit/0602373bef984f50f66a52867d2109e9c10ed029))


## v0.3.0-beta.1 (2026-02-02)

### Chores

- **deps**: Add hypothesis testing library and update test config
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

### Continuous Integration

- Fix cd failing due to lack of git profile
  ([`1ea33fc`](https://github.com/aorumbayev/kagan/commit/1ea33fcb2384d76ad1d0e8f723632042e4dc4e36))

### Documentation

- Update documentation and add architecture guide
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

### Features

- Ux improvements; minor new features; internal refactor
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **cli**: Add tools command for agent management
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

### Refactoring

- Update core modules and styling ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **acp**: Consolidate JSON-RPC and remove legacy RPC layer
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **agents**: Improve planner, scheduler, and add installer
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **database**: Enhance models and add migrations support
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **keybindings**: Consolidate into single module
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **tests**: Remove duplicate permission_prompt test
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **tests**: Update e2e tests for refactored UI ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **tests**: Update integration tests for refactored modules
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **tests**: Update test infrastructure and helpers
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **tests**: Update unit tests for refactored modules
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))

- **ui**: Restructure screens, modals, and widgets
  ([#11](https://github.com/aorumbayev/kagan/pull/11),
  [`d7c686a`](https://github.com/aorumbayev/kagan/commit/d7c686a6678a1ed7861f6ab2aff77126881344af))


## v0.2.1 (2026-01-31)

### Bug Fixes

- Use PEP 440 version syntax for uv tool upgrade command
  ([#10](https://github.com/aorumbayev/kagan/pull/10),
  [`92e4240`](https://github.com/aorumbayev/kagan/commit/92e4240d74501f767db90090c4420f2bec883b2e))

### Documentation

- Update readme
  ([`ae2c9a2`](https://github.com/aorumbayev/kagan/commit/ae2c9a2a33801184cc02edbbe2ce05dd9e1f6455))

### Refactoring

- **tests**: Mock fetch_latest_version instead of httpx transport
  ([`db9053a`](https://github.com/aorumbayev/kagan/commit/db9053a0ed4d8a9b503ca03bcf850e0f06112506))

- **tests**: Reduce httpx_mock usage to minimize event loop warnings
  ([`37332db`](https://github.com/aorumbayev/kagan/commit/37332db58650b42f481039e8a5874d05bb24044e))


## v0.2.0 (2026-01-31)


## v0.2.0-beta.3 (2026-01-31)

### Bug Fixes

- Add auto-mock for tmux in E2E tests for CI runners without tmux
  ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

- Improves agent detection and troubleshooting UX ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

- Include .gitignore in initial commit on first boot
  ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

- Resolve UI freezes from blocking operations ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

- Use explicit foreground colors for troubleshooting screen text
  ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

- **ci**: Separate test jobs to fix matrix conditional evaluation
  ([#9](https://github.com/aorumbayev/kagan/pull/9),
  [`fe57f66`](https://github.com/aorumbayev/kagan/commit/fe57f663aef153780eadd5699082932d0266ca1a))

- **tests**: Convert update CLI tests to async for proper event loop cleanup
  ([`769e46e`](https://github.com/aorumbayev/kagan/commit/769e46e5e3d0a6caf73c5f5b6f96bf5593c99afd))

### Chores

- Update GitHub Actions to latest versions ([#9](https://github.com/aorumbayev/kagan/pull/9),
  [`fe57f66`](https://github.com/aorumbayev/kagan/commit/fe57f663aef153780eadd5699082932d0266ca1a))

### Continuous Integration

- Add macOS ARM64 to PR test matrix ([#9](https://github.com/aorumbayev/kagan/pull/9),
  [`fe57f66`](https://github.com/aorumbayev/kagan/commit/fe57f663aef153780eadd5699082932d0266ca1a))

### Features

- Add dynamic agent detection and improve troubleshooting UX
  ([#8](https://github.com/aorumbayev/kagan/pull/8),
  [`6ea7f44`](https://github.com/aorumbayev/kagan/commit/6ea7f449d7af3e63ff97b88e7af0d27c81d00ee9))

### Performance Improvements

- **ci**: Optimize CI workflow for faster PR feedback
  ([#9](https://github.com/aorumbayev/kagan/pull/9),
  [`fe57f66`](https://github.com/aorumbayev/kagan/commit/fe57f663aef153780eadd5699082932d0266ca1a))

### Refactoring

- **ci**: Simplify workflow structure ([#9](https://github.com/aorumbayev/kagan/pull/9),
  [`fe57f66`](https://github.com/aorumbayev/kagan/commit/fe57f663aef153780eadd5699082932d0266ca1a))


## v0.2.0-beta.2 (2026-01-31)

### Bug Fixes

- Add packaging as explicit dependency ([#7](https://github.com/aorumbayev/kagan/pull/7),
  [`1e0e7ea`](https://github.com/aorumbayev/kagan/commit/1e0e7eadf47e11292bf3f42eb1a218a358859b58))


## v0.2.0-beta.1 (2026-01-30)

### Chores

- Update typo in src/kagan/ui/widgets/empty_state.py
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

### Documentation

- Refining docs
  ([`67ea1fe`](https://github.com/aorumbayev/kagan/commit/67ea1fea945c4188a137c442db6b787ba2ad359f))

- Update documentation with testing rules and agent capabilities
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

### Features

- Refactoring, cleanup and new features ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **agents**: Add prompt refiner for pre-send enhancement
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **ansi**: Add terminal output cleaner for escape sequences
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **cli**: Add update command with auto-upgrade support
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **core**: Enhance screens with keybindings registry and read-only agents
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **keybindings**: Add centralized keybindings registry
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

- **ui**: Add new modals, widgets, and clipboard utilities
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))

### Refactoring

- **tests**: Reorganize test suite into categorized structure
  ([#6](https://github.com/aorumbayev/kagan/pull/6),
  [`d47160f`](https://github.com/aorumbayev/kagan/commit/d47160f38c7c7d7d91939529001fd21cedb151ca))


## v0.1.0 (2026-01-29)


## v0.1.0-beta.3 (2026-01-29)

### Bug Fixes

- Fix missing readme on pyproject ([#5](https://github.com/aorumbayev/kagan/pull/5),
  [`b1693ca`](https://github.com/aorumbayev/kagan/commit/b1693ca918e9bad96b9f1195b82df8b9d150712f))

### Continuous Integration

- Add docs_only flag to CD workflow for independent documentation publishing
  ([#5](https://github.com/aorumbayev/kagan/pull/5),
  [`b1693ca`](https://github.com/aorumbayev/kagan/commit/b1693ca918e9bad96b9f1195b82df8b9d150712f))

### Documentation

- Refines documentation
  ([`f1ac3d1`](https://github.com/aorumbayev/kagan/commit/f1ac3d1945ec7a546344f704f29643373b232ba8))


## v0.1.0-beta.2 (2026-01-29)

### Bug Fixes

- Fix missing readme on pyproject
  ([`a1cbb66`](https://github.com/aorumbayev/kagan/commit/a1cbb6664e564beb9b8e8d7b2febf4d9bf93c26d))


## v0.1.0-beta.1 (2026-01-29)

- Initial Release
