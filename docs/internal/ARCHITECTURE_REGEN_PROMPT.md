# Architecture Documentation Regeneration Prompt

Use this prompt to regenerate `docs/internal/ARCHITECTURE.md` when the codebase structure changes significantly.

## The Prompt

```
Regenerate the architecture documentation for the Kagan TUI application.

## Instructions

1. **Explore the codebase** - Read key structural files:
   - `src/kagan/app.py` (main app class, lifecycle)
   - `src/kagan/database/models.py` (data models, enums)
   - `src/kagan/keybindings.py` (all keybindings)
   - `src/kagan/config.py` (configuration model)
   - `src/kagan/ui/modals/__init__.py` (modal registry)
   - `src/kagan/ui/screens/` (all screens)
   - `src/kagan/agents/scheduler.py` (agent lifecycle)
   - `src/kagan/sessions/manager.py` (tmux integration)

2. **Update `docs/internal/ARCHITECTURE.md`** with:
   - Current project structure (file tree)
   - All ticket model fields and enums
   - All keybinding collections and their purposes
   - Screen navigation flow
   - Modal registry with return types
   - State machine transitions for AUTO/PAIR modes
   - Agent capabilities matrix
   - Configuration options
   - Database operation patterns
   - Common code patterns

3. **Preserve the document structure** - Keep existing sections, update content.

4. **Add timestamp** - Update "Last updated" to current date.

## Output Format

Update the existing `docs/internal/ARCHITECTURE.md` file in place.
```

## When to Regenerate

- After adding new screens, modals, or widgets
- After changing the ticket model
- After adding/modifying keybindings
- After significant refactoring
- Before major feature development

## Quick Validation Prompt

For quick checks without full regeneration:

```
Validate that docs/internal/ARCHITECTURE.md is accurate:
1. Check if all files in src/kagan/ui/modals/__init__.py are documented
2. Check if all keybinding collections in keybindings.py are listed
3. Check if all TicketStatus/TicketType enums match models.py
4. Check if screen navigation flow matches actual push_screen() calls
Report any discrepancies.
```
