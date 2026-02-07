"""XDG-compliant path helpers for Kagan data storage."""

from __future__ import annotations

import os
import platform
import tempfile
from pathlib import Path

from platformdirs import user_cache_dir, user_config_dir, user_data_dir


def get_data_dir() -> Path:
    """Get the data directory for Kagan (database, images, etc.)."""
    override = os.environ.get("KAGAN_DATA_DIR")
    if override:
        return Path(override)
    return Path(user_data_dir("kagan"))


def get_config_dir() -> Path:
    """Get the config directory for Kagan (config.toml, profiles.toml)."""
    override = os.environ.get("KAGAN_CONFIG_DIR")
    if override:
        return Path(override)
    return Path(user_config_dir("kagan"))


def get_cache_dir() -> Path:
    """Get the cache directory for Kagan (temporary files, downloads)."""
    override = os.environ.get("KAGAN_CACHE_DIR")
    if override:
        return Path(override)
    return Path(user_cache_dir("kagan"))


def get_worktree_base_dir() -> Path:
    """Get the base directory for git worktrees.

    - Linux: /var/tmp/kagan/
    - macOS: /var/folders/.../kagan/ (via tempfile)
    - Windows: %TEMP%/kagan/
    """
    override = os.environ.get("KAGAN_WORKTREE_BASE")
    if override:
        return Path(override)

    system = platform.system()
    if system == "Linux" and Path("/var/tmp").exists():
        base = Path("/var/tmp")
    else:
        base = Path(tempfile.gettempdir())
    return base / "kagan"


def get_database_path() -> Path:
    """Get the path to the SQLite database."""
    return get_data_dir() / "kagan.db"


def get_config_path() -> Path:
    """Get the path to the main config file."""
    return get_config_dir() / "config.toml"


def get_profiles_path() -> Path:
    """Get the path to the agent profiles file."""
    return get_config_dir() / "profiles.toml"


def get_lock_path() -> Path:
    """Get the path to the instance lock file."""
    return get_data_dir() / "kagan.lock"


def get_debug_log_path() -> Path:
    """Get the path to the debug log export file."""
    return get_data_dir() / "debug.log"


def ensure_directories() -> None:
    """Create all necessary directories if they don't exist."""
    get_data_dir().mkdir(parents=True, exist_ok=True)
    get_config_dir().mkdir(parents=True, exist_ok=True)
    get_cache_dir().mkdir(parents=True, exist_ok=True)
    get_worktree_base_dir().mkdir(parents=True, exist_ok=True)
