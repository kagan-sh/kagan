from __future__ import annotations

from typing import Any, Literal, Required, TypedDict

# Capabilities


class FileSystemCapability(TypedDict, total=False):
    readTextFile: bool
    writeTextFile: bool


class ClientCapabilities(TypedDict, total=False):
    fs: FileSystemCapability
    terminal: bool


class Implementation(TypedDict, total=False):
    name: Required[str]
    title: str
    version: Required[str]


class PromptCapabilities(TypedDict, total=False):
    audio: bool
    embeddedContent: bool
    image: bool


class AgentCapabilities(TypedDict, total=False):
    loadSession: bool
    promptCapabilities: PromptCapabilities


# Content Types


class Annotations(TypedDict, total=False):
    audience: list[str]
    priority: float
    lastModified: str


class TextContent(TypedDict, total=False):
    type: Required[Literal["text"]]
    text: Required[str]
    annotations: Annotations


class ImageContent(TypedDict, total=False):
    type: Required[Literal["image"]]
    data: str
    mimeType: Required[str]
    url: str
    annotations: Annotations


class AudioContent(TypedDict, total=False):
    type: Required[Literal["audio"]]
    data: str
    mimeType: Required[str]
    annotations: Annotations


class EmbeddedResourceText(TypedDict, total=False):
    uri: str
    text: Required[str]
    mimeType: str


class EmbeddedResourceBlob(TypedDict, total=False):
    uri: str
    blob: Required[str]
    mimeType: str


class EmbeddedResourceContent(TypedDict, total=False):
    type: Required[Literal["resource"]]
    resource: EmbeddedResourceText | EmbeddedResourceBlob


class ResourceLinkContent(TypedDict, total=False):
    type: Required[Literal["resource_link"]]
    uri: str
    name: Required[str]
    mimeType: str
    size: int
    annotations: Annotations


type ContentBlock = (
    TextContent | ImageContent | AudioContent | EmbeddedResourceContent | ResourceLinkContent
)


# Session Updates


class UserMessageChunk(TypedDict, total=False):
    sessionUpdate: Required[Literal["user_message_chunk"]]
    content: Required[ContentBlock]


class AgentMessageChunk(TypedDict, total=False):
    sessionUpdate: Required[Literal["agent_message_chunk"]]
    content: Required[ContentBlock]


class AgentThoughtChunk(TypedDict, total=False):
    sessionUpdate: Required[Literal["agent_thought_chunk"]]
    content: Required[ContentBlock]


class ToolCallContentContent(TypedDict, total=False):
    type: Required[Literal["content"]]
    content: ContentBlock


class ToolCallContentDiff(TypedDict, total=False):
    type: Required[Literal["diff"]]
    diff: str


class ToolCallContentTerminal(TypedDict, total=False):
    type: Required[Literal["terminal"]]
    terminalOutput: str


type ToolCallContent = ToolCallContentContent | ToolCallContentDiff | ToolCallContentTerminal

type ToolKind = Literal[
    "read", "edit", "delete", "move", "search", "execute", "think", "fetch", "switch_mode", "other"
]

type ToolCallStatus = Literal["pending", "in_progress", "completed", "failed"]


class ToolCallLocation(TypedDict, total=False):
    path: Required[str]
    line: int


class ToolCall(TypedDict, total=False):
    toolCallId: str
    title: Required[str]
    sessionUpdate: Required[Literal["tool_call"]]
    status: ToolCallStatus
    kind: ToolKind
    content: list[ToolCallContent]
    locations: list[ToolCallLocation]
    rawInput: str
    rawOutput: str


class ToolCallUpdate(TypedDict, total=False):
    toolCallId: str
    title: Required[str]
    sessionUpdate: Required[Literal["tool_call_update"]]
    status: ToolCallStatus
    kind: ToolKind
    content: list[ToolCallContent]
    locations: list[ToolCallLocation]
    rawInput: str
    rawOutput: str


# Use in the session/request_permission call (not the same as ToolCallUpdate)
# https://agentclientprotocol.com/protocol/schema#param-tool-call
class ToolCallUpdatePermissionRequest(TypedDict, total=False):
    _meta: dict[str, Any]
    content: list[ToolCallContent] | None
    kind: ToolKind | None
    locations: list[ToolCallLocation] | None
    rawInput: dict[str, Any]
    rawOutput: dict[str, Any]
    status: ToolCallStatus | None
    title: str | None
    toolCallId: Required[str]


class PlanEntry(TypedDict, total=False):
    content: Required[str]
    priority: int
    status: Literal["pending", "in_progress", "completed", "failed"]


class Plan(TypedDict, total=False):
    entries: Required[list[PlanEntry]]
    sessionUpdate: Required[Literal["plan"]]


class SessionMode(TypedDict, total=False):
    id: Required[str]
    name: Required[str]
    description: str | None


class SessionModeState(TypedDict, total=False):
    availableModes: Required[list[SessionMode]]
    currentModeId: Required[str]


class ModelInfo(TypedDict, total=False):
    id: Required[str]
    name: str


class AvailableCommand(TypedDict, total=False):
    name: Required[str]
    description: str


class AvailableCommandsUpdate(TypedDict, total=False):
    sessionUpdate: Required[Literal["available_commands"]]
    commands: Required[list[AvailableCommand]]


class CurrentModeUpdate(TypedDict, total=False):
    sessionUpdate: Required[Literal["current_mode"]]
    mode: Required[str]


type SessionUpdate = (
    UserMessageChunk
    | AgentMessageChunk
    | AgentThoughtChunk
    | ToolCall
    | ToolCallUpdate
    | Plan
    | AvailableCommandsUpdate
    | CurrentModeUpdate
)


# Permission System

type PermissionOptionKind = Literal["allow_once", "allow_always", "reject_once", "reject_always"]
type PermissionOptionId = str


class PermissionOption(TypedDict):
    kind: Required[PermissionOptionKind]
    name: Required[str]
    optionId: Required[PermissionOptionId]


class OutcomeSelected(TypedDict, total=False):
    optionId: Required[PermissionOptionId]
    outcome: Required[Literal["selected"]]


class OutcomeCancelled(TypedDict, total=False):
    outcome: Required[Literal["cancelled"]]


type RequestPermissionOutcome = OutcomeSelected | OutcomeCancelled


# Response Types


class InitializeResponse(TypedDict, total=False):
    protocolVersion: Required[str]
    agentCapabilities: AgentCapabilities
    authMethods: list[str]


class NewSessionResponse(TypedDict, total=False):
    sessionId: Required[str]
    modes: SessionModeState | None
    models: list[ModelInfo]


class SessionPromptResponse(TypedDict, total=False):
    stopReason: Required[
        Literal["end_turn", "max_tokens", "max_turn_requests", "refusal", "cancelled"]
    ]


class RequestPermissionResponse(TypedDict, total=False):
    outcome: Required[RequestPermissionOutcome]


class CreateTerminalResponse(TypedDict, total=False):
    terminalId: Required[str]


class TerminalExitStatus(TypedDict, total=False):
    exitCode: int
    signal: str


class TerminalOutputResponse(TypedDict, total=False):
    output: str
    truncated: Required[bool]
    exitStatus: TerminalExitStatus


class SetSessionModeResponse(TypedDict, total=False):
    success: bool


class EnvVariable(TypedDict):
    name: Required[str]
    value: Required[str]


class McpServer(TypedDict, total=False):
    command: str
    name: str
    args: list[str]
    env: dict[str, str]


class KillTerminalCommandResponse(TypedDict, total=False):
    """Response for terminal/kill request."""

    pass


class ReleaseTerminalResponse(TypedDict, total=False):
    """Response for terminal/release request."""

    pass


class WaitForTerminalExitResponse(TypedDict, total=False):
    """Response for terminal/wait_for_exit request."""

    exitCode: int
    signal: str | None
