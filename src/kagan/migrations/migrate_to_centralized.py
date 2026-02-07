"""Legacy data migration to centralized storage (alpha no-op)."""

from __future__ import annotations


async def migrate_legacy_data() -> None:
    """Migrate data from legacy repo-local locations.

    This is intentionally a no-op in the alpha refactor where legacy paths
    are no longer supported.
    """
    return None
