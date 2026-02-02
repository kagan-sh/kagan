"""Numeric limits and timeouts - no circular dependencies."""

# Timeouts (seconds)
AGENT_TIMEOUT = 30.0
AGENT_TIMEOUT_LONG = 60.0
SHUTDOWN_TIMEOUT = 5.0

# Buffers
RESPONSE_BUFFER = 10000
MESSAGE_BUFFER = 500
SUBPROCESS_LIMIT = 10 * 1024 * 1024
SCRATCHPAD_LIMIT = 50000
