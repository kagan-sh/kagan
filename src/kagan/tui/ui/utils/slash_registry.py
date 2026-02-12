"""Slash command registry and parser."""

from __future__ import annotations

import re
from collections.abc import Awaitable, Callable, Sequence
from dataclasses import dataclass
from typing import overload


@dataclass(frozen=True, slots=True, kw_only=True)
class SlashCommand[F: Callable[..., None | Awaitable[None]]]:
    """Slash command metadata plus executable function."""

    command: str
    help: str
    func: F
    aliases: list[str]


class SlashCommandRegistry[F: Callable[..., None | Awaitable[None]]]:
    """Registry for slash commands and aliases."""

    def __init__(self) -> None:
        self._commands: dict[str, SlashCommand[F]] = {}
        self._command_aliases: dict[str, SlashCommand[F]] = {}

    @overload
    def command(self, func: F, /) -> F: ...

    @overload
    def command(
        self,
        *,
        name: str | None = None,
        aliases: Sequence[str] | None = None,
    ) -> Callable[[F], F]: ...

    def command(
        self,
        func: F | None = None,
        *,
        name: str | None = None,
        aliases: Sequence[str] | None = None,
    ) -> F | Callable[[F], F]:
        """Decorator to register a slash command."""

        def _register(f: F) -> F:
            primary = name or f.__name__
            alias_list = list(aliases) if aliases else []
            entry = SlashCommand[F](
                command=primary,
                help=(f.__doc__ or "").strip(),
                func=f,
                aliases=alias_list,
            )
            self._commands[primary] = entry
            self._command_aliases[primary] = entry
            for alias in alias_list:
                self._command_aliases[alias] = entry
            return f

        if func is not None:
            return _register(func)
        return _register

    def find_command(self, name: str) -> SlashCommand[F] | None:
        return self._command_aliases.get(name)

    def list_commands(self) -> list[SlashCommand[F]]:
        return list(self._commands.values())


@dataclass(frozen=True, slots=True, kw_only=True)
class SlashCommandCall:
    """Parsed slash command invocation."""

    name: str
    args: str
    raw_input: str


def parse_slash_command_call(user_input: str) -> SlashCommandCall | None:
    """Parse slash command call text."""
    user_input = user_input.strip()
    if not user_input or not user_input.startswith("/"):
        return None

    name_match = re.match(r"^\/([a-zA-Z0-9_-]+(?::[a-zA-Z0-9_-]+)*)", user_input)
    if not name_match:
        return None

    command_name = name_match.group(1)
    if len(user_input) > name_match.end() and not user_input[name_match.end()].isspace():
        return None

    raw_args = user_input[name_match.end() :].lstrip()
    return SlashCommandCall(name=command_name, args=raw_args, raw_input=user_input)
