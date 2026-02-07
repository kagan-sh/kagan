"""Global MCP configuration helpers for interactive (PAIR) agents."""

from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from kagan.core.models.enums import McpFileFormat, McpInstallMethod
from kagan.mcp_naming import get_mcp_server_name


@dataclass(frozen=True)
class GlobalMcpSpec:
    agent: str
    method: McpInstallMethod
    config_path: Path | None = None
    file_format: McpFileFormat | None = None
    cli_add: list[str] | None = None
    cli_list: list[str] | None = None
    note: str | None = None


def _resolve_kagan_path() -> str:
    """Resolve absolute path to the kagan executable."""
    return shutil.which("kagan") or "kagan"


def get_global_mcp_spec(agent_short_name: str) -> GlobalMcpSpec | None:
    """Return global MCP config spec for supported agents."""
    kagan_bin = _resolve_kagan_path()
    server_name = get_mcp_server_name()

    if agent_short_name == "opencode":
        return GlobalMcpSpec(
            agent="opencode",
            method=McpInstallMethod.FILE,
            config_path=_resolve_opencode_config_path(),
            file_format=McpFileFormat.OPENCODE,
        )
    if agent_short_name == "claude":
        return GlobalMcpSpec(
            agent="claude",
            method=McpInstallMethod.CLI,
            config_path=_resolve_claude_config_path(),
            file_format=McpFileFormat.CLAUDE,
            cli_add=[
                "claude",
                "mcp",
                "add",
                "--transport",
                "stdio",
                "--scope",
                "user",
                server_name,
                "--",
                kagan_bin,
                "mcp",
            ],
            cli_list=["claude", "mcp", "list"],
        )
    if agent_short_name == "codex":
        return GlobalMcpSpec(
            agent="codex",
            method=McpInstallMethod.FILE,
            config_path=_resolve_codex_config_path(),
            file_format=McpFileFormat.CODEX,
        )
    if agent_short_name == "gemini":
        return GlobalMcpSpec(
            agent="gemini",
            method=McpInstallMethod.FILE,
            config_path=_resolve_gemini_config_path(),
            file_format=McpFileFormat.CLAUDE,
        )
    if agent_short_name == "kimi":
        return GlobalMcpSpec(
            agent="kimi",
            method=McpInstallMethod.CLI,
            config_path=_resolve_kimi_config_path(),
            file_format=McpFileFormat.CLAUDE,
            cli_add=[
                "kimi",
                "mcp",
                "add",
                "--transport",
                "stdio",
                server_name,
                "--",
                kagan_bin,
                "mcp",
            ],
            cli_list=["kimi", "mcp", "list"],
        )
    if agent_short_name == "copilot":
        return GlobalMcpSpec(
            agent="copilot",
            method=McpInstallMethod.FILE,
            config_path=_resolve_copilot_config_path(),
            file_format=McpFileFormat.COPILOT,
        )
    return None


def get_global_mcp_install_command(agent_short_name: str) -> str | None:
    """Return a CLI install command for the agent if available."""
    spec = get_global_mcp_spec(agent_short_name)
    if spec is None or not spec.cli_add:
        return None
    return shlex.join(spec.cli_add)


def get_install_description(spec: GlobalMcpSpec) -> str:
    """Return a human-readable description of what the install will do."""
    if spec.method == McpInstallMethod.CLI and spec.cli_add:
        return f"$ {shlex.join(spec.cli_add)}"
    if spec.config_path:
        return f'Add "{get_mcp_server_name()}" entry to:\n{spec.config_path}'
    return "Configure MCP for this agent"


def is_global_mcp_configured(agent_short_name: str) -> bool:
    """Return True if a global MCP config exists and contains an MCP entry."""
    spec = get_global_mcp_spec(agent_short_name)
    if spec is None:
        return False
    server_name = get_mcp_server_name()

    if spec.method == McpInstallMethod.CLI and spec.cli_list:
        ok, output = _run_cli_command(spec.cli_list)
        if ok and _output_has_server(output, server_name):
            return True

    if spec.config_path is None:
        return False

    if spec.file_format == McpFileFormat.CODEX:
        return _is_codex_configured(spec.config_path, server_name)

    if spec.file_format is None:
        return False

    config = _load_config(spec.config_path)
    if config is None:
        return False

    key = _mcp_key_for_format(spec.file_format)
    section = config.get(key)
    if not isinstance(section, dict):
        return False

    return server_name in section


def install_global_mcp(agent_short_name: str) -> tuple[bool, str, Path | None]:
    """Install or update the global MCP config for the given agent."""
    spec = get_global_mcp_spec(agent_short_name)
    if spec is None:
        return False, f"No global MCP config known for {agent_short_name}", None

    if spec.method == McpInstallMethod.MANUAL:
        return False, f"Manual MCP setup required for {agent_short_name}", spec.config_path

    if spec.method == McpInstallMethod.CLI and spec.cli_add:
        ok, output = _run_cli_command(spec.cli_add)
        if ok:
            return True, f"Ran: {shlex.join(spec.cli_add)}", spec.config_path
        if spec.config_path is None or spec.file_format is None:
            return False, output, spec.config_path
        fallback_ok, fallback_message, _path = _install_global_mcp_file(spec)
        if fallback_ok:
            return True, f"CLI failed; wrote {spec.config_path}", spec.config_path
        return False, f"{output}; {fallback_message}", spec.config_path

    if spec.method == McpInstallMethod.FILE:
        return _install_global_mcp_file(spec)

    return False, "Unsupported MCP install method", spec.config_path


def _install_global_mcp_file(spec: GlobalMcpSpec) -> tuple[bool, str, Path | None]:
    if spec.config_path is None or spec.file_format is None:
        return False, "Missing MCP config path", None

    config_path = spec.config_path
    config_path.parent.mkdir(parents=True, exist_ok=True)
    server_name = get_mcp_server_name()

    if spec.file_format == McpFileFormat.CODEX:
        return _install_codex_toml(config_path, server_name)

    config = _load_config(config_path)
    if config is None and config_path.exists():
        return False, f"Invalid JSON in {config_path}", config_path
    if config is None:
        config = {}

    key = _mcp_key_for_format(spec.file_format)
    entry = _mcp_entry_for_format(spec.file_format)

    section = config.get(key)
    if not isinstance(section, dict):
        section = {}
    section[server_name] = entry
    config[key] = section

    if spec.file_format == McpFileFormat.OPENCODE and "$schema" not in config:
        config["$schema"] = "https://opencode.ai/config.json"

    config_path.write_text(json.dumps(config, indent=2))
    return True, f"Updated {config_path}", config_path


def _is_codex_configured(config_path: Path, server_name: str) -> bool:
    """Check if Codex TOML config already has an MCP entry."""
    if not config_path.exists():
        return False
    try:
        content = config_path.read_text(encoding="utf-8")
    except OSError:
        return False
    header = _codex_section_header(server_name)
    return header in content


def _install_codex_toml(config_path: Path, server_name: str) -> tuple[bool, str, Path]:
    """Append MCP server section to Codex config.toml."""
    kagan_bin = _resolve_kagan_path()
    existing = ""
    if config_path.exists():
        try:
            existing = config_path.read_text(encoding="utf-8")
        except OSError:
            return False, f"Cannot read {config_path}", config_path

    header = _codex_section_header(server_name)
    if header in existing:
        return True, "Already configured", config_path

    section = f'\n{header}\ncommand = "{kagan_bin}"\nargs = ["mcp"]\nenabled = true\n'
    if existing and not existing.endswith("\n"):
        section = "\n" + section

    config_path.write_text(existing + section, encoding="utf-8")
    return True, f"Updated {config_path}", config_path


def _run_cli_command(args: list[str]) -> tuple[bool, str]:
    try:
        result = subprocess.run(
            args,
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return False, f"Command not found: {args[0]}"

    output = (result.stdout or "").strip()
    error = (result.stderr or "").strip()
    if result.returncode != 0:
        message = error or output or f"Command failed: {shlex.join(args)}"
        return False, message
    return True, output


def _output_has_server(output: str, server_name: str) -> bool:
    return server_name.lower() in output.lower()


def _load_config(path: Path) -> dict[str, object] | None:
    if not path.exists():
        return None
    try:
        content = path.read_text(encoding="utf-8")
        return json.loads(content) if content.strip() else {}
    except json.JSONDecodeError:
        return None


def _mcp_key_for_format(fmt: McpFileFormat) -> str:
    return "mcp" if fmt == McpFileFormat.OPENCODE else "mcpServers"


def _mcp_entry_for_format(fmt: McpFileFormat) -> dict[str, object]:
    kagan_bin = _resolve_kagan_path()
    if fmt == McpFileFormat.OPENCODE:
        return {
            "type": "local",
            "command": [kagan_bin, "mcp"],
            "enabled": True,
        }
    if fmt == McpFileFormat.COPILOT:
        return {
            "type": "stdio",
            "command": kagan_bin,
            "args": ["mcp"],
            "tools": ["*"],
        }
    return {
        "command": kagan_bin,
        "args": ["mcp"],
    }


def _is_bare_toml_key(value: str) -> bool:
    return re.fullmatch(r"[A-Za-z0-9_-]+", value) is not None


def _codex_section_header(server_name: str) -> str:
    if _is_bare_toml_key(server_name):
        key = server_name
    else:
        escaped = server_name.replace('"', '\\"')
        key = f'"{escaped}"'
    return f"[mcp_servers.{key}]"


def _resolve_opencode_config_path() -> Path:
    env_path = os.environ.get("OPENCODE_CONFIG")
    if env_path:
        return Path(env_path)
    return Path.home() / ".config" / "opencode" / "opencode.json"


def _resolve_claude_config_path() -> Path:
    env_path = os.environ.get("KAGAN_CLAUDE_CONFIG_PATH")
    if env_path:
        return Path(env_path)
    return Path.home() / ".claude.json"


def _resolve_gemini_config_path() -> Path:
    return Path.home() / ".gemini" / "settings.json"


def _resolve_kimi_config_path() -> Path:
    return Path.home() / ".kimi" / "mcp.json"


def _resolve_codex_config_path() -> Path:
    return Path.home() / ".codex" / "config.toml"


def _resolve_copilot_config_path() -> Path:
    if os.environ.get("XDG_CONFIG_HOME"):
        return Path(os.environ["XDG_CONFIG_HOME"]) / "copilot" / "mcp-config.json"
    return Path.home() / ".copilot" / "mcp-config.json"
