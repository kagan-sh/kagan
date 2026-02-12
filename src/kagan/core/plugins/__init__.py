"""Provider-neutral plugin SDK surface for core extensions."""

from kagan.core.plugins.sdk import (
    JsonPluginManifestLoader,
    Plugin,
    PluginManifest,
    PluginManifestLoader,
    PluginOperation,
    PluginOperationHandler,
    PluginPolicyContext,
    PluginPolicyDecision,
    PluginPolicyHook,
    PluginRegistrationApi,
    PluginRegistry,
)

__all__ = [
    "JsonPluginManifestLoader",
    "Plugin",
    "PluginManifest",
    "PluginManifestLoader",
    "PluginOperation",
    "PluginOperationHandler",
    "PluginPolicyContext",
    "PluginPolicyDecision",
    "PluginPolicyHook",
    "PluginRegistrationApi",
    "PluginRegistry",
]
