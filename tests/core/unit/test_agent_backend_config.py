"""Behavior-first tests for backend configs and builtin agent availability."""

from __future__ import annotations

import pytest
from pydantic import TypeAdapter, ValidationError

import kagan.core.builtin_agents as builtin_agents
from kagan.core.agents.backend_config import (
    BACKEND_CONFIG_DEFAULTS,
    AgentBackendConfig,
    ClaudeAgentConfig,
    CodexAgentConfig,
    CopilotAgentConfig,
    GeminiAgentConfig,
    KimiAgentConfig,
    OpenCodeAgentConfig,
    get_backend_config,
)

_adapter = TypeAdapter(AgentBackendConfig)

_BACKEND_CLASS_CASES = [
    pytest.param("claude", ClaudeAgentConfig, id="claude"),
    pytest.param("opencode", OpenCodeAgentConfig, id="opencode"),
    pytest.param("copilot", CopilotAgentConfig, id="copilot"),
    pytest.param("gemini", GeminiAgentConfig, id="gemini"),
    pytest.param("kimi", KimiAgentConfig, id="kimi"),
    pytest.param("codex", CodexAgentConfig, id="codex"),
]


@pytest.mark.parametrize(
    ("payload", "expected_cls"),
    [
        pytest.param(
            {"type": "claude", "model": "opus", "allowed_tools": ["Bash", "Read"]},
            ClaudeAgentConfig,
            id="claude",
        ),
        pytest.param(
            {"type": "opencode", "model": "gpt-4o"},
            OpenCodeAgentConfig,
            id="opencode",
        ),
        pytest.param(
            {"type": "copilot", "model": "claude-opus-4"},
            CopilotAgentConfig,
            id="copilot",
        ),
        pytest.param(
            {"type": "gemini", "model": "gemini-2.5-flash"},
            GeminiAgentConfig,
            id="gemini",
        ),
        pytest.param({"type": "kimi", "model": "kimi-k2"}, KimiAgentConfig, id="kimi"),
        pytest.param({"type": "codex", "model": "o4-mini"}, CodexAgentConfig, id="codex"),
    ],
)
def test_when_deserializing_valid_backend_payload_then_union_selects_expected_model(
    payload: dict[str, object],
    expected_cls: type,
) -> None:
    config = _adapter.validate_python(payload)

    assert isinstance(config, expected_cls)
    assert config.model == payload["model"]
    if isinstance(config, ClaudeAgentConfig):
        assert config.allowed_tools == ["Bash", "Read"]


@pytest.mark.parametrize("agent_type", sorted(BACKEND_CONFIG_DEFAULTS))
def test_when_type_only_payload_then_defaults_are_applied(agent_type: str) -> None:
    config = _adapter.validate_python({"type": agent_type})
    expected_default = get_backend_config(agent_type)

    assert type(config) is type(expected_default)
    assert config.model == expected_default.model


def test_when_backend_type_is_unknown_then_validation_error_is_raised() -> None:
    with pytest.raises(ValidationError):
        _adapter.validate_python({"type": "unknown_agent"})


def test_when_backend_type_is_missing_then_validation_error_is_raised() -> None:
    with pytest.raises(ValidationError):
        _adapter.validate_python({"model": "some-model"})


@pytest.mark.parametrize(("agent_type", "expected_cls"), _BACKEND_CLASS_CASES)
def test_when_get_backend_config_receives_supported_type_then_it_returns_typed_defaults(
    agent_type: str,
    expected_cls: type,
) -> None:
    config = get_backend_config(agent_type)

    assert type(config) is expected_cls
    assert config.type == agent_type


def test_when_get_backend_config_receives_unknown_type_then_error_lists_supported_types() -> None:
    with pytest.raises(ValueError, match="Unknown agent type") as exc_info:
        get_backend_config("bad")

    supported = ", ".join(sorted(BACKEND_CONFIG_DEFAULTS))
    assert f"Supported: {supported}" in str(exc_info.value)


@pytest.mark.parametrize(("agent_type", "expected_cls"), _BACKEND_CLASS_CASES)
def test_when_backend_config_is_serialized_and_restored_then_type_and_model_are_preserved(
    agent_type: str,
    expected_cls: type,
) -> None:
    model_name = f"custom-{agent_type}"
    if agent_type == "claude":
        original = expected_cls(model=model_name, allowed_tools=["Bash"])
    else:
        original = expected_cls(model=model_name)

    restored = _adapter.validate_json(original.model_dump_json())

    assert type(restored) is expected_cls
    assert restored.type == agent_type
    assert restored.model == model_name


@pytest.mark.parametrize(("agent_name", "expected_cls"), _BACKEND_CLASS_CASES)
def test_when_loading_builtin_agent_then_backend_config_schema_matches_registration(
    agent_name: str,
    expected_cls: type,
) -> None:
    agent = builtin_agents.get_builtin_agent(agent_name)

    assert agent is not None
    assert isinstance(agent.backend_config, expected_cls)


def test_when_lookup_uses_unknown_builtin_agent_name_then_none_is_returned() -> None:
    assert builtin_agents.get_builtin_agent("not-a-real-agent") is None


def test_when_listing_builtin_agents_then_registration_order_is_preserved() -> None:
    listed_names = [agent.config.short_name for agent in builtin_agents.list_builtin_agents()]

    assert listed_names == list(builtin_agents.BUILTIN_AGENTS)


def _make_availability_checker(availability_by_name: dict[str, tuple[bool, bool]]):
    def _fake_check(agent: builtin_agents.BuiltinAgent) -> builtin_agents.AgentAvailability:
        interactive, acp = availability_by_name.get(agent.config.short_name, (False, False))
        return builtin_agents.AgentAvailability(
            agent=agent,
            interactive_available=interactive,
            acp_available=acp,
        )

    return _fake_check


def test_when_collecting_availability_then_priority_order_is_used_and_unknown_entries_are_skipped(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_agents,
        "AGENT_PRIORITY",
        ["missing-agent", "codex", "claude"],
    )

    checks: list[str] = []

    def _recording_check(agent: builtin_agents.BuiltinAgent) -> builtin_agents.AgentAvailability:
        checks.append(agent.config.short_name)
        return builtin_agents.AgentAvailability(agent=agent)

    monkeypatch.setattr(builtin_agents, "check_agent_availability", _recording_check)

    availability = builtin_agents.get_all_agent_availability()
    result_names = [item.agent.config.short_name for item in availability]

    assert result_names == ["codex", "claude"]
    assert checks == ["codex", "claude"]


def test_when_multiple_agents_are_available_then_first_available_selection_prefers_higher_priority(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_agents,
        "check_agent_availability",
        _make_availability_checker(
            {
                "opencode": (False, True),
                "codex": (True, False),
            }
        ),
    )

    selected = builtin_agents.get_first_available_agent()

    assert selected is builtin_agents.BUILTIN_AGENTS["opencode"]


def test_when_no_agents_are_available_then_first_available_selection_returns_none(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_agents,
        "check_agent_availability",
        _make_availability_checker({}),
    )

    assert builtin_agents.get_first_available_agent() is None


def test_when_only_acp_mode_is_available_then_any_agent_available_is_true(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_agents,
        "check_agent_availability",
        _make_availability_checker(
            {
                "gemini": (False, True),
            }
        ),
    )

    assert builtin_agents.any_agent_available() is True


def test_when_no_modes_are_available_then_any_agent_available_is_false(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_agents,
        "check_agent_availability",
        _make_availability_checker({}),
    )

    assert builtin_agents.any_agent_available() is False
