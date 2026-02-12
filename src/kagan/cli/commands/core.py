"""Core process management commands."""

from __future__ import annotations

import contextlib
import json
import os
import signal
import sys
from typing import TYPE_CHECKING

import click

from kagan.core.paths import (
    get_core_endpoint_path,
    get_core_runtime_dir,
    get_core_token_path,
)
from kagan.core.process_liveness import pid_exists

if TYPE_CHECKING:
    from pathlib import Path


@click.group()
def core() -> None:
    """Manage the Kagan core process."""


def _print_endpoint_details(endpoint, *, include_runtime: bool = False) -> None:
    """Print endpoint details in a consistent format."""
    click.echo(f"  Transport: {endpoint.transport}")
    click.echo(f"  Address:   {endpoint.address}")
    if endpoint.port is not None:
        click.echo(f"  Port:      {endpoint.port}")
    if endpoint.pid is not None:
        click.echo(f"  PID:       {endpoint.pid}")
    if include_runtime:
        click.echo(f"  Runtime:   {get_core_runtime_dir()}")


def _read_pid(path: Path) -> int | None:
    try:
        raw = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    if not raw:
        return None
    try:
        return int(raw)
    except ValueError:
        return None


def _pid_exists(pid: int) -> bool:
    return pid_exists(pid)


def _discover_running_pid_fallback() -> int | None:
    lock_path = get_core_runtime_dir() / "core.instance.lock"
    lease_path = get_core_runtime_dir() / "core.lease.json"
    try:
        lease_data = json.loads(lease_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, TypeError, ValueError):
        lease_data = None
    if isinstance(lease_data, dict):
        raw_owner_pid = lease_data.get("owner_pid")
        if isinstance(raw_owner_pid, int) and _pid_exists(raw_owner_pid):
            return raw_owner_pid
        if isinstance(raw_owner_pid, str):
            with contextlib.suppress(ValueError):
                parsed = int(raw_owner_pid)
                if _pid_exists(parsed):
                    return parsed
    pid = _read_pid(lock_path)
    if pid is not None and _pid_exists(pid):
        return pid
    return None


@core.command()
@click.option(
    "--foreground",
    is_flag=True,
    help="Run core in foreground (blocks until stopped).",
)
def start(foreground: bool) -> None:
    """Start the core process if it is not running."""
    from kagan.core.ipc.discovery import discover_core_endpoint
    from kagan.core.launcher import ensure_core_running_sync, launch_core_subprocess

    endpoint = discover_core_endpoint()
    if endpoint is not None:
        click.secho("Core is already running.", fg="green", bold=True)
        _print_endpoint_details(endpoint)
        return

    if foreground:
        raise SystemExit(launch_core_subprocess())

    endpoint = ensure_core_running_sync()
    click.secho("Core started.", fg="green", bold=True)
    _print_endpoint_details(endpoint, include_runtime=True)


@core.command()
def status() -> None:
    """Show the status of the running core process."""
    from kagan.core.ipc.discovery import discover_core_endpoint

    endpoint = discover_core_endpoint()
    if endpoint is None:
        fallback_pid = _discover_running_pid_fallback()
        if fallback_pid is not None:
            click.secho("Core process is running, but runtime metadata is incomplete.", fg="yellow")
            click.echo(f"  PID:       {fallback_pid}")
            click.echo(f"  Runtime:   {get_core_runtime_dir()}")
            click.echo("  Hint:      run `kagan core stop` then `kagan core start`")
            sys.exit(2)
        click.secho("Core is not running.", fg="yellow")
        sys.exit(1)

    click.secho("Core is running.", fg="green", bold=True)
    _print_endpoint_details(endpoint, include_runtime=True)


@core.command()
def stop() -> None:
    """Stop the running core process."""
    from kagan.core.ipc.discovery import discover_core_endpoint

    endpoint = discover_core_endpoint()
    pid = endpoint.pid if endpoint is not None else None
    if pid is None:
        pid = _discover_running_pid_fallback()
    if pid is None:
        click.secho("Core is not running.", fg="yellow")
        sys.exit(1)

    click.echo(f"Stopping core process (PID {pid})...")
    try:
        os.kill(pid, signal.SIGTERM)
        click.secho("Stop signal sent.", fg="green")
    except ProcessLookupError:
        click.secho("Process already stopped.", fg="yellow")
        # Clean up stale runtime files
        _cleanup_stale_files()
    except PermissionError:
        click.secho(f"Permission denied to stop PID {pid}.", fg="red")
        sys.exit(1)


def _cleanup_stale_files() -> None:
    """Remove stale runtime files when the core is no longer running."""
    for path_fn in (
        get_core_endpoint_path,
        get_core_token_path,
    ):
        with contextlib.suppress(OSError):
            path_fn().unlink(missing_ok=True)
    with contextlib.suppress(OSError):
        (get_core_runtime_dir() / "core.lease.json").unlink(missing_ok=True)
