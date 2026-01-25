"""CLI entry point for Kagan."""

from __future__ import annotations

import argparse
import sys

from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH, DEFAULT_LOCK_PATH


def main() -> int:
    """Main entry point for Kagan CLI."""
    parser = argparse.ArgumentParser(
        prog="kagan",
        description="AI-powered Kanban TUI for autonomous development workflows",
    )
    parser.add_argument(
        "--version",
        action="store_true",
        help="Show version and exit",
    )
    parser.add_argument(
        "--db",
        type=str,
        default=DEFAULT_DB_PATH,
        help=f"Path to SQLite database (default: {DEFAULT_DB_PATH})",
    )
    parser.add_argument(
        "--config",
        type=str,
        default=DEFAULT_CONFIG_PATH,
        help=f"Path to config file (default: {DEFAULT_CONFIG_PATH})",
    )

    args = parser.parse_args()

    if args.version:
        from kagan import __version__

        print(f"kagan {__version__}")
        return 0

    # Import here to avoid slow startup for --help/--version
    from kagan.lock import InstanceLock, InstanceLockError

    # Try to acquire the instance lock
    lock = InstanceLock(DEFAULT_LOCK_PATH)
    try:
        lock.acquire()
    except InstanceLockError:
        # Another instance is running - show the locked screen
        from kagan.ui.screens.locked import InstanceLockedApp

        app = InstanceLockedApp()
        app.run()
        return 1

    # Lock acquired - run the main app
    try:
        from kagan.app import KaganApp

        app = KaganApp(db_path=args.db, config_path=args.config)
        app._instance_lock = lock
        app.run()
        return 0
    finally:
        lock.release()


if __name__ == "__main__":
    sys.exit(main())
