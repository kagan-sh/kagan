"""MCP server naming helpers.

This module is intentionally placed at the root of the kagan package
(not inside kagan.mcp) to avoid circular import issues. The mcp package
imports heavy dependencies that trigger the circular chain.
"""

from __future__ import annotations

import os

from kagan.config import KaganConfig

DEFAULT_MCP_SERVER_NAME = "kagan"
ENV_MCP_SERVER_NAME = "KAGAN_MCP_SERVER_NAME"


def get_mcp_server_name() -> str:
    """Resolve the MCP server name.

    Priority:
    1) Environment variable KAGAN_MCP_SERVER_NAME
    2) Config file: general.mcp_server_name
    3) Default: "kagan"
    """
    env_name = os.environ.get(ENV_MCP_SERVER_NAME)
    if env_name:
        stripped = env_name.strip()
        if stripped:
            return stripped

    try:
        config = KaganConfig.load()
        cfg_name = config.general.mcp_server_name.strip()
        if cfg_name:
            return cfg_name
    except Exception:
        return DEFAULT_MCP_SERVER_NAME

    return DEFAULT_MCP_SERVER_NAME
