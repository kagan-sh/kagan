"""Update command for Kagan - checks PyPI and upgrades installation."""

from __future__ import annotations

import subprocess
import sys
from dataclasses import dataclass
from importlib.metadata import PackageNotFoundError, distribution
from pathlib import Path

import click
import httpx
from packaging.version import Version
from packaging.version import parse as parse_version

from kagan import __version__

PYPI_URL = "https://pypi.org/pypi/kagan/json"
TIMEOUT_SECONDS = 5.0


@dataclass
class InstallationInfo:
    """Information about how kagan was installed."""

    method: str  # "uv", "pipx", "pip"
    upgrade_command: list[str]  # Command parts to run

    def format_command(self) -> str:
        """Return human-readable command string."""
        return " ".join(self.upgrade_command)


@dataclass
class UpdateCheckResult:
    """Result of checking for updates."""

    current_version: str
    latest_version: str | None
    is_dev: bool
    error: str | None = None

    @property
    def update_available(self) -> bool:
        """Check if an update is available."""
        if self.is_dev or self.latest_version is None:
            return False
        try:
            return parse_version(self.latest_version) > parse_version(self.current_version)
        except Exception:
            return self.latest_version != self.current_version


def get_installed_version() -> str:
    """Get the installed kagan version."""
    return __version__


def is_dev_version(version: str) -> bool:
    """Check if this is a development/editable install."""
    return version == "dev" or ".dev" in version or "+editable" in version


def fetch_latest_version(prerelease: bool = False, timeout: float = TIMEOUT_SECONDS) -> str | None:
    """Query PyPI for the latest kagan version.

    Args:
        prerelease: If True, consider pre-release versions.
        timeout: HTTP request timeout in seconds.

    Returns:
        Latest version string, or None if fetch failed.
    """
    try:
        with httpx.Client(timeout=timeout) as client:
            response = client.get(PYPI_URL)
            response.raise_for_status()
            data = response.json()

            if prerelease:
                # Get latest from all releases (including prereleases)
                releases = data.get("releases", {})
                if not releases:
                    return data.get("info", {}).get("version")

                versions = []
                for ver_str in releases:
                    try:
                        versions.append(parse_version(ver_str))
                    except Exception:
                        continue

                if versions:
                    return str(max(versions))
                return None
            else:
                # Get stable version from info
                return data.get("info", {}).get("version")
    except httpx.TimeoutException:
        return None
    except httpx.HTTPError:
        return None
    except Exception:
        return None


def check_for_updates(prerelease: bool = False) -> UpdateCheckResult:
    """Check if a newer version of kagan is available.

    Args:
        prerelease: If True, consider pre-release versions.

    Returns:
        UpdateCheckResult with version information.
    """
    current = get_installed_version()
    is_dev = is_dev_version(current)

    if is_dev:
        return UpdateCheckResult(
            current_version=current,
            latest_version=None,
            is_dev=True,
            error="Running from development version",
        )

    latest = fetch_latest_version(prerelease=prerelease)
    if latest is None:
        return UpdateCheckResult(
            current_version=current,
            latest_version=None,
            is_dev=False,
            error="Failed to fetch version from PyPI",
        )

    return UpdateCheckResult(
        current_version=current,
        latest_version=latest,
        is_dev=False,
    )


def _get_installer_info() -> tuple[str, Path] | None:
    """Get installer name and distribution path for kagan."""
    try:
        dist = distribution("kagan")
        installer = (dist.read_text("INSTALLER") or "pip").strip().lower()
        dist_path = Path(str(dist.locate_file(""))).resolve()
        return (installer, dist_path)
    except PackageNotFoundError:
        return None


def detect_installation_method(target_version: str) -> InstallationInfo | None:
    """Detect how kagan was installed and return appropriate upgrade command.

    Args:
        target_version: The version to upgrade to.

    Returns:
        InstallationInfo with upgrade command, or None if detection failed.
    """
    installer_info = _get_installer_info()
    if installer_info is None:
        return None

    installer, dist_path = installer_info
    dist_path_str = str(dist_path).lower()

    # UV tool installation
    if installer == "uv":
        if "tool" in dist_path_str or ".local/share/uv/tools" in dist_path_str:
            return InstallationInfo(
                method="uv tool",
                upgrade_command=["uv", "tool", "upgrade", f"kagan@{target_version}"],
            )
        # UV in venv
        return InstallationInfo(
            method="uv",
            upgrade_command=["uv", "pip", "install", f"kagan=={target_version}"],
        )

    # PIPX installation - use install --force to upgrade to specific version
    if installer == "pipx" or "pipx" in dist_path_str:
        return InstallationInfo(
            method="pipx",
            upgrade_command=["pipx", "install", f"kagan=={target_version}", "--force"],
        )

    # Check for venv/pip installation
    pyvenv_cfg = Path(sys.prefix) / "pyvenv.cfg"
    if pyvenv_cfg.exists() or installer == "pip":
        return InstallationInfo(
            method="pip",
            upgrade_command=[sys.executable, "-m", "pip", "install", f"kagan=={target_version}"],
        )

    return None


def run_upgrade(info: InstallationInfo) -> tuple[bool, str]:
    """Execute the upgrade command.

    Args:
        info: Installation info with upgrade command.

    Returns:
        Tuple of (success, message).
    """
    try:
        result = subprocess.run(
            info.upgrade_command,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            return True, "Upgrade completed successfully"
        else:
            error_msg = result.stderr.strip() or result.stdout.strip() or "Unknown error"
            return False, f"Upgrade failed: {error_msg}"
    except FileNotFoundError:
        return False, f"Command not found: {info.upgrade_command[0]}"
    except Exception as e:
        return False, f"Upgrade failed: {e}"


def prompt_and_update(
    check_result: UpdateCheckResult,
    force: bool = False,
    prerelease: bool = False,
) -> bool:
    """Prompt user and perform update if confirmed.

    Args:
        check_result: Result from check_for_updates().
        force: Skip confirmation prompt.
        prerelease: Whether prerelease versions are being considered.

    Returns:
        True if update was performed successfully, False otherwise.
    """
    if not check_result.update_available or check_result.latest_version is None:
        return False

    current = check_result.current_version
    latest = check_result.latest_version

    click.echo()
    click.secho("Update available!", fg="yellow", bold=True)
    click.echo(f"  Current version: {click.style(current, fg='red')}")
    click.echo(f"  Latest version:  {click.style(latest, fg='green', bold=True)}")

    # Check if latest is a prerelease
    try:
        latest_ver = parse_version(latest)
        if isinstance(latest_ver, Version) and latest_ver.is_prerelease:
            click.echo(f"  {click.style('(pre-release)', fg='yellow')}")
    except Exception:
        pass

    click.echo()

    # Detect installation method
    install_info = detect_installation_method(latest)
    if install_info is None:
        click.secho("Could not detect installation method.", fg="red")
        click.echo("Please upgrade manually using one of:")
        click.echo(f"  uv tool upgrade kagan@{latest}")
        click.echo(f"  pipx install kagan=={latest} --force")
        click.echo(f"  pip install kagan=={latest}")
        return False

    click.echo(f"Detected installation method: {click.style(install_info.method, fg='cyan')}")
    click.echo(f"Will run: {click.style(install_info.format_command(), fg='cyan')}")
    click.echo()

    if not force:
        if not click.confirm("Proceed with upgrade?", default=True):
            click.echo("Upgrade cancelled.")
            return False

    click.echo("Upgrading...")
    success, message = run_upgrade(install_info)

    if success:
        click.secho(f"✓ Successfully upgraded to kagan {latest}", fg="green", bold=True)
        return True
    else:
        click.secho(f"✗ {message}", fg="red")
        return False


@click.command()
@click.option("-f", "--force", is_flag=True, help="Skip confirmation prompt")
@click.option("--check", "check_only", is_flag=True, help="Only check for updates, don't install")
@click.option("--prerelease", is_flag=True, help="Include pre-release versions")
def update(force: bool, check_only: bool, prerelease: bool) -> None:
    """Check for and install kagan updates from PyPI.

    Exit codes for --check mode:
      0 = Already on latest version
      1 = Update available
      2 = Error occurred
    """
    result = check_for_updates(prerelease=prerelease)

    # Handle dev version
    if result.is_dev:
        click.secho("Running from development version. Cannot auto-update.", fg="yellow")
        click.echo("If you installed from source, use 'git pull' instead.")
        if check_only:
            sys.exit(2)
        return

    # Handle fetch errors
    if result.error and result.latest_version is None:
        click.secho(f"Error: {result.error}", fg="red")
        if check_only:
            sys.exit(2)
        return

    # Check mode output
    if check_only:
        click.echo(f"Current version: {result.current_version}")
        click.echo(f"Latest version:  {result.latest_version}")

        if result.update_available:
            click.secho("Update available!", fg="yellow")
            sys.exit(1)
        else:
            click.secho("Already on the latest version.", fg="green")
            sys.exit(0)

    # Already up to date
    if not result.update_available:
        click.secho(
            f"kagan {result.current_version} is already the latest version.",
            fg="green",
        )
        return

    # Perform update
    prompt_and_update(result, force=force, prerelease=prerelease)
