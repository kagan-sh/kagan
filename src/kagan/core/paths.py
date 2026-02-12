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
        return Path(override).resolve()
    return Path(user_data_dir("kagan"))


def get_config_dir() -> Path:
    """Get the config directory for Kagan (config.toml, profiles.toml)."""
    override = os.environ.get("KAGAN_CONFIG_DIR")
    if override:
        return Path(override).resolve()
    return Path(user_config_dir("kagan"))


def get_cache_dir() -> Path:
    """Get the cache directory for Kagan (temporary files, downloads)."""
    override = os.environ.get("KAGAN_CACHE_DIR")
    if override:
        return Path(override).resolve()
    return Path(user_cache_dir("kagan"))


def get_worktree_base_dir() -> Path:
    """Get the base directory for git worktrees."""
    override = os.environ.get("KAGAN_WORKTREE_BASE")
    if override:
        return Path(override).resolve()

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


def get_debug_log_path() -> Path:
    """Get the path to the debug log export file."""
    return get_data_dir() / "debug.log"


def get_core_runtime_dir() -> Path:
    """Get the runtime directory for the Kagan core process.

    Houses the endpoint file, lease file, and authentication token used by
    clients to discover and connect to a running core instance.
    """
    override = os.environ.get("KAGAN_CORE_RUNTIME_DIR")
    if override:
        return Path(override).resolve()
    return get_data_dir() / "core"


def get_core_endpoint_path() -> Path:
    """Get the path to the core endpoint descriptor file.

    The file contains a JSON object describing how to connect to the running
    core (transport type, address/path, port, etc.).
    """
    return get_core_runtime_dir() / "endpoint.json"


def get_core_lease_path() -> Path:
    """Get the path to the core lease metadata file.

    Stores core ownership and heartbeat metadata used for deterministic
    startup/recovery and stale-lease reclamation.
    """
    return get_core_runtime_dir() / "core.lease.json"


def get_core_token_path() -> Path:
    """Get the path to the core authentication token file.

    Stores a short-lived bearer token that clients include in IPC requests
    to authenticate with the core process.
    """
    return get_core_runtime_dir() / "token"


def get_core_instance_lock_path() -> Path:
    """Get the path to the core singleton instance lock file."""
    return get_core_runtime_dir() / "core.instance.lock"


def ensure_directories() -> None:
    """Create all necessary directories if they don't exist."""
    get_data_dir().mkdir(parents=True, exist_ok=True)
    get_config_dir().mkdir(parents=True, exist_ok=True)
    get_cache_dir().mkdir(parents=True, exist_ok=True)
    get_worktree_base_dir().mkdir(parents=True, exist_ok=True)
    get_core_runtime_dir().mkdir(parents=True, exist_ok=True)
