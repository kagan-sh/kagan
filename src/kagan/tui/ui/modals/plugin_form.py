"""Schema-driven plugin form modal."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Input, Label, Select, Static, Switch

from kagan.tui.ui.modals.base import KaganModalScreen

if TYPE_CHECKING:
    from textual.app import ComposeResult


def _field_label(name: str) -> str:
    return name.replace("_", " ").strip().title() or name


class PluginFormModal(KaganModalScreen[dict[str, Any] | None]):
    """Render a plugin-provided form schema and return a validated input payload."""

    def __init__(
        self,
        *,
        form: dict[str, Any],
        initial_values: dict[str, Any] | None = None,
    ) -> None:
        super().__init__()
        self._form = dict(form)
        self._initial_values = dict(initial_values or {})

    def compose(self) -> ComposeResult:
        title = str(self._form.get("title") or "Plugin Action").strip()
        fields = self._form.get("fields")
        if not isinstance(fields, list):
            fields = []

        with Vertical(id="dialog"):
            yield Static(title, classes="dialog-title")

            for field in fields:
                if not isinstance(field, dict):
                    continue
                name = field.get("name")
                if not isinstance(name, str) or not name.strip():
                    continue
                kind = field.get("kind")
                required = field.get("required") is True
                placeholder = field.get("placeholder")
                placeholder_text = str(placeholder) if isinstance(placeholder, str) else ""

                with Vertical(classes="field"):
                    label = _field_label(name)
                    required_suffix = " *" if required else ""
                    yield Label(f"{label}{required_suffix}", classes="field-label")

                    widget_id = f"plugin-form-{name}"
                    if kind == "boolean":
                        yield Switch(
                            id=widget_id,
                            value=bool(self._initial_values.get(name, False)),
                        )
                    elif kind == "select":
                        raw_options = field.get("options") or []
                        options = [
                            (str(item.get("label", "")).strip(), str(item.get("value", "")).strip())
                            for item in raw_options
                            if isinstance(item, dict)
                            and str(item.get("label", "")).strip()
                            and str(item.get("value", "")).strip()
                        ]
                        selected = self._initial_values.get(name)
                        value = (
                            str(selected).strip()
                            if isinstance(selected, str) and selected
                            else None
                        )
                        yield Select(
                            options=options,
                            value=value,
                            id=widget_id,
                            allow_blank=not required,
                        )
                    else:
                        initial = self._initial_values.get(name)
                        value = str(initial) if initial is not None else ""
                        yield Input(
                            value=value,
                            placeholder=placeholder_text,
                            id=widget_id,
                        )

            with Horizontal(id="dialog-actions"):
                yield Button("Cancel", id="btn-cancel")
                yield Button("Run", id="btn-submit", variant="primary")

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-cancel":
            self.dismiss(None)
            return
        if event.button.id == "btn-submit":
            await self._submit()

    async def _submit(self) -> None:
        fields = self._form.get("fields")
        if not isinstance(fields, list):
            self.dismiss({})
            return

        payload: dict[str, Any] = {}
        missing: list[str] = []

        for field in fields:
            if not isinstance(field, dict):
                continue
            name = field.get("name")
            if not isinstance(name, str) or not name.strip():
                continue
            kind = field.get("kind")
            required = field.get("required") is True
            widget_id = f"#plugin-form-{name}"

            if kind == "boolean":
                value = bool(self.query_one(widget_id, Switch).value)
                payload[name] = value
                continue

            if kind == "select":
                select = self.query_one(widget_id, Select)
                value = select.value
                if value is not None:
                    payload[name] = str(value)
                elif required:
                    missing.append(name)
                continue

            value = self.query_one(widget_id, Input).value.strip()
            if value:
                payload[name] = value
            elif required:
                missing.append(name)

        if missing:
            missing_str = ", ".join(_field_label(name) for name in missing)
            self.notify(f"Missing required field(s): {missing_str}", severity="error")
            return

        self.dismiss(payload)


__all__ = ["PluginFormModal"]
