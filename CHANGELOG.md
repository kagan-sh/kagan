# CHANGELOG

<!-- version list -->

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
