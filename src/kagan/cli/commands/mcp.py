"""MCP server command."""

from __future__ import annotations

import click

_VALID_CAPABILITIES = {"viewer", "planner", "pair_worker", "operator", "maintainer"}
_VALID_IDENTITIES = {"kagan", "kagan_admin"}


@click.command()
@click.option(
    "--readonly",
    is_flag=True,
    help="Expose only read-only coordination tools (for ACP agents)",
)
@click.option(
    "--session-id",
    default=None,
    help="Session ID to bind this MCP server instance to a specific task",
)
@click.option(
    "--capability",
    default=None,
    help="Capability profile for this session (viewer|planner|pair_worker|operator|maintainer)",
)
@click.option(
    "--endpoint",
    default=None,
    help="Core endpoint address to connect to (overrides discovery)",
)
@click.option(
    "--identity",
    default=None,
    help="Session identity lane for policy ceilings (kagan|kagan_admin)",
)
@click.option(
    "--enable-internal-instrumentation",
    is_flag=True,
    default=False,
    help="Enable internal instrumentation diagnostics MCP tool (disabled by default).",
)
def mcp(
    readonly: bool,
    session_id: str | None,
    capability: str | None,
    endpoint: str | None,
    identity: str | None,
    enable_internal_instrumentation: bool,
) -> None:
    """Run the MCP server (STDIO transport).

    This command is typically invoked by AI agents (Claude Code, OpenCode, etc.)
    to communicate with Kagan via the Model Context Protocol.

    The MCP server uses centralized storage and assumes the current working
    directory is a Kagan-managed project.

    Use --readonly for ACP agents to expose only coordination tools
    (task_list, task_get).
    """
    if capability is not None and capability not in _VALID_CAPABILITIES:
        valid = ", ".join(sorted(_VALID_CAPABILITIES))
        raise click.BadParameter(f"Invalid capability '{capability}'. Expected one of: {valid}")
    if identity is not None and identity not in _VALID_IDENTITIES:
        valid = ", ".join(sorted(_VALID_IDENTITIES))
        raise click.BadParameter(f"Invalid identity '{identity}'. Expected one of: {valid}")

    from kagan.mcp.server import main as mcp_main

    try:
        if enable_internal_instrumentation:
            mcp_main(
                readonly=readonly,
                endpoint=endpoint,
                session_id=session_id,
                capability=capability,
                identity=identity,
                enable_internal_instrumentation=True,
            )
            return
        mcp_main(
            readonly=readonly,
            endpoint=endpoint,
            session_id=session_id,
            capability=capability,
            identity=identity,
        )
    except TypeError as exc:
        message = str(exc)
        missing_new_flag = "enable_internal_instrumentation" in message
        if (not enable_internal_instrumentation) or not missing_new_flag:
            raise
        mcp_main(
            readonly=readonly,
            endpoint=endpoint,
            session_id=session_id,
            capability=capability,
            identity=identity,
        )
