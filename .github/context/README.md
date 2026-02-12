# .github/context — Agent Planning & Coordination Archives

This directory stores planning artifacts for complex multi-file work items.
Each subdirectory is a self-contained archive or active initiative.

## Creating a New Initiative

1. Create a subdirectory: `.github/context/<initiative-name>/`
2. Add the required files (see template below)
3. Use the shared scratchpad for real-time coordination
4. Archive here when complete — git history preserves the timeline

## Required Structure

```
.github/context/<initiative-name>/
├── PLAYBOOK.md
├── COMMON-SCRATCHPAD.md
├── ASSIGNMENTS.md
├── SCOPE-FREEZE.md
├── TICKET-INDEX.md
└── tickets/
```

## Existing Initiatives / Archives

| Directory | Initiative | Tickets | Status |
|-----------|-----------|---------|--------|
| `alpha-strangler-migration/` | Facade-first strangler migration + alpha hardening | T-000..T-083 | Complete |
| `alpha-unified-consolidation/` | Unified remaining work (polish + model consistency) via 3 non-overlapping streams | U-WS1..U-WS3 | Active |
