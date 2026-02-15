"""Schema-driven plugin UI definitions (TUI).

This module defines the V1 contract for declarative plugin UI contributions.
The core validates, sanitizes, and merges plugin-provided payloads into a
stable catalog that the TUI can render without executing plugin code.

Security properties:
- Fail closed: invalid payloads yield empty contributions, not exceptions.
- Allowlisted plugins only: the caller must filter by plugin_id.
- No executable expressions: only data (actions/forms/badges).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, ValidationError

type UiSurface = Literal["kanban.repo_actions", "kanban.task_actions", "header.badges"]
type UiFieldKind = Literal["text", "select", "boolean"]
type UiBadgeState = Literal["ok", "warn", "error", "info"]


_CAPABILITY_PATTERN = r"^[a-z][a-z0-9_]{1,31}$"
_METHOD_PATTERN = r"^[a-z][a-z0-9_]{1,63}$"
_ACTION_ID_PATTERN = r"^[a-z][a-z0-9_]{1,63}$"
_FORM_ID_PATTERN = r"^[a-z][a-z0-9_.-]{2,96}$"
_BADGE_ID_PATTERN = r"^[a-z][a-z0-9_]{1,63}$"


class UiOperationRef(BaseModel):
    model_config = ConfigDict(extra="ignore")

    capability: str = Field(pattern=_CAPABILITY_PATTERN)
    method: str = Field(pattern=_METHOD_PATTERN)


class UiAction(BaseModel):
    model_config = ConfigDict(extra="ignore")

    plugin_id: str = Field(min_length=3, max_length=64)
    action_id: str = Field(pattern=_ACTION_ID_PATTERN)
    surface: UiSurface
    label: str = Field(min_length=1, max_length=120)
    operation: UiOperationRef
    form_id: str | None = Field(default=None, pattern=_FORM_ID_PATTERN)
    confirm: bool = False
    command: str | None = Field(default=None, max_length=80)
    help: str | None = Field(default=None, max_length=200)


class UiSelectOption(BaseModel):
    model_config = ConfigDict(extra="ignore")

    label: str = Field(min_length=1, max_length=120)
    value: str = Field(min_length=1, max_length=200)


class UiField(BaseModel):
    model_config = ConfigDict(extra="ignore")

    name: str = Field(pattern=r"^[a-z][a-z0-9_]{0,63}$")
    kind: UiFieldKind
    required: bool = False
    options: list[UiSelectOption] | None = None
    placeholder: str | None = Field(default=None, max_length=120)


class UiForm(BaseModel):
    model_config = ConfigDict(extra="ignore")

    form_id: str = Field(pattern=_FORM_ID_PATTERN)
    title: str = Field(min_length=1, max_length=120)
    fields: list[UiField] = Field(default_factory=list)


class UiBadge(BaseModel):
    model_config = ConfigDict(extra="ignore")

    plugin_id: str = Field(min_length=3, max_length=64)
    badge_id: str = Field(pattern=_BADGE_ID_PATTERN)
    surface: UiSurface
    label: str = Field(min_length=1, max_length=60)
    state: UiBadgeState
    text: str = Field(min_length=1, max_length=120)


@dataclass(frozen=True, slots=True)
class UiCatalog:
    schema_version: str
    actions: list[dict[str, Any]]
    forms: list[dict[str, Any]]
    badges: list[dict[str, Any]]
    diagnostics: list[str]


def sanitize_plugin_ui_payload(
    payload: object,
    *,
    plugin_id: str,
) -> UiCatalog:
    """Validate and sanitize a single plugin-provided UI payload.

    Invalid objects are dropped individually; the result is always usable.
    """
    diagnostics: list[str] = []
    if not isinstance(payload, dict):
        diagnostics.append("payload must be an object")
        return UiCatalog(
            schema_version="1",
            actions=[],
            forms=[],
            badges=[],
            diagnostics=diagnostics,
        )

    version_raw = payload.get("schema_version", "1")
    version = str(version_raw).strip() if version_raw is not None else "1"
    if version != "1":
        diagnostics.append(f"unsupported schema_version: {version}")
        return UiCatalog(
            schema_version="1",
            actions=[],
            forms=[],
            badges=[],
            diagnostics=diagnostics,
        )

    actions: list[dict[str, Any]] = []
    actions_raw = payload.get("actions", [])
    if actions_raw is None:
        actions_raw = []
    if not isinstance(actions_raw, list):
        diagnostics.append("actions must be a list")
        actions_raw = []
    for idx, item in enumerate(actions_raw):
        if not isinstance(item, dict):
            diagnostics.append(f"actions[{idx}] must be an object")
            continue
        if "plugin_id" not in item:
            item = dict(item)
            item["plugin_id"] = plugin_id
        try:
            parsed = UiAction.model_validate(item)
        except ValidationError as exc:
            diagnostics.append(f"actions[{idx}] invalid: {exc.errors()[0]['msg']}")
            continue
        if parsed.plugin_id != plugin_id:
            diagnostics.append(f"actions[{idx}] plugin_id mismatch")
            continue
        actions.append(parsed.model_dump(mode="json"))

    forms: list[dict[str, Any]] = []
    forms_raw = payload.get("forms", [])
    if forms_raw is None:
        forms_raw = []
    if not isinstance(forms_raw, list):
        diagnostics.append("forms must be a list")
        forms_raw = []
    for idx, item in enumerate(forms_raw):
        if not isinstance(item, dict):
            diagnostics.append(f"forms[{idx}] must be an object")
            continue
        try:
            parsed = UiForm.model_validate(item)
        except ValidationError as exc:
            diagnostics.append(f"forms[{idx}] invalid: {exc.errors()[0]['msg']}")
            continue
        dumped = parsed.model_dump(mode="json")
        dumped["plugin_id"] = plugin_id
        forms.append(dumped)

    badges: list[dict[str, Any]] = []
    badges_raw = payload.get("badges", [])
    if badges_raw is None:
        badges_raw = []
    if not isinstance(badges_raw, list):
        diagnostics.append("badges must be a list")
        badges_raw = []
    for idx, item in enumerate(badges_raw):
        if not isinstance(item, dict):
            diagnostics.append(f"badges[{idx}] must be an object")
            continue
        if "plugin_id" not in item:
            item = dict(item)
            item["plugin_id"] = plugin_id
        try:
            parsed = UiBadge.model_validate(item)
        except ValidationError as exc:
            diagnostics.append(f"badges[{idx}] invalid: {exc.errors()[0]['msg']}")
            continue
        if parsed.plugin_id != plugin_id:
            diagnostics.append(f"badges[{idx}] plugin_id mismatch")
            continue
        badges.append(parsed.model_dump(mode="json"))

    return UiCatalog(
        schema_version="1",
        actions=actions,
        forms=forms,
        badges=badges,
        diagnostics=diagnostics,
    )


__all__ = [
    "UiAction",
    "UiBadge",
    "UiCatalog",
    "UiField",
    "UiForm",
    "UiOperationRef",
    "sanitize_plugin_ui_payload",
]
