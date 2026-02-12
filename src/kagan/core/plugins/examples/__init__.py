"""Example plugins used only for scaffold wiring validation."""

from kagan.core.plugins.examples.noop import NoOpExamplePlugin, register_example_plugins

__all__ = ["NoOpExamplePlugin", "register_example_plugins"]
