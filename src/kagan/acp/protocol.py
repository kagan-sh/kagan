"""ACP Protocol type aliases.

Simplified types - data flows as dict[str, Any] at runtime.
Literal types document valid values; dict aliases provide minimal type safety.
"""

from __future__ import annotations

from typing import Any, Literal

# Literal types for documenting valid values
type ToolKind = Literal[
    "read", "edit", "delete", "move", "search", "execute", "think", "fetch", "switch_mode", "other"
]
type ToolCallStatus = Literal["pending", "in_progress", "completed", "failed"]
type PermissionOptionKind = Literal["allow_once", "allow_always", "reject_once", "reject_always"]

# All protocol data types are dict[str, Any] at runtime
type ToolCall = dict[str, Any]
type ToolCallUpdate = dict[str, Any]
type ToolCallUpdatePermissionRequest = dict[str, Any]
type SessionUpdate = dict[str, Any]
type PermissionOption = dict[str, Any]
type PlanEntry = dict[str, Any]
type AvailableCommand = dict[str, Any]
type EnvVariable = dict[str, Any]
type TerminalOutputResponse = dict[str, Any]

# Capability types
type FileSystemCapability = dict[str, bool]
type ClientCapabilities = dict[str, Any]
type AgentCapabilities = dict[str, Any]

# Model-related types (from ACP schema)
type ModelId = str
type ModelInfo = dict[str, Any]  # {modelId, name, description?}
type SessionModelState = dict[str, Any]  # {availableModels, currentModelId}
