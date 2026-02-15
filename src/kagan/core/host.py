"""Core host process — owns AppContext, IPC server, and event lifecycle."""

from __future__ import annotations

import asyncio
import contextlib
import copy
import enum
import json
import logging
import os
from collections import OrderedDict
from typing import TYPE_CHECKING

from kagan.core.bootstrap import create_app_context
from kagan.core.config import KaganConfig
from kagan.core.events import (
    CoreHostDraining,
    CoreHostRunning,
    CoreHostStarting,
    CoreHostStopped,
)
from kagan.core.instance_lease import (
    CORE_LEASE_HEARTBEAT_SECONDS,
    CoreInstanceLock,
)
from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.server import IPCServer
from kagan.core.models.enums import TaskType
from kagan.core.paths import (
    get_config_path,
    get_core_endpoint_path,
    get_core_instance_lock_path,
    get_core_runtime_dir,
    get_core_token_path,
    get_database_path,
)
from kagan.core.request_context import RequestContext, request_context
from kagan.core.request_dispatch_map import build_request_dispatch_map
from kagan.core.runtime_helpers import (
    IDEMPOTENCY_CACHE_LIMIT,
    IDEMPOTENT_MUTATION_METHODS,
    CachedResponseEnvelope,
    IdempotencyRecord,
    IdempotencyReservation,
)
from kagan.core.security import AuthorizationError, CapabilityProfile
from kagan.core.session_binding import (
    SessionBinding,
    SessionBindingError,
    SessionOrigin,
    enforce_task_scope,
)
from kagan.core.session_binding import (
    get_binding as get_session_binding,
)
from kagan.core.session_binding import (
    register_session as register_session_binding,
)
from kagan.core.session_binding import (
    unregister_session as unregister_session_binding,
)
from kagan.version import get_kagan_version

if TYPE_CHECKING:
    from pathlib import Path
    from typing import Any

    from kagan.core.bootstrap import AppContext
    from kagan.core.ipc.transports import ServerHandle
    from kagan.core.plugins.sdk import PluginOperation, PluginPolicyDecision, PluginRegistry
    from kagan.core.request_dispatch_map import RequestHandler

logger = logging.getLogger(__name__)


def _core_lease_path() -> Path:
    return get_core_runtime_dir() / "core.lease.json"


_REQUEST_DISPATCH_MAP: dict[tuple[str, str], RequestHandler] | None = None


class CoreHostStatus(enum.Enum):
    """State machine for the core host lifecycle."""

    STARTING = "starting"
    RUNNING = "running"
    DRAINING = "draining"
    STOPPED = "stopped"


class CoreHost:
    """Manages the lifecycle of the Kagan core process.

    Owns:
    - ``AppContext`` (all domain services and event bus)
    - ``IPCServer`` (accepts client connections)
    - Idle shutdown timer
    - Runtime file management (lease, endpoint, token)

    Usage::

        host = CoreHost()
        await host.start()
        await host.wait_until_stopped()
    """

    def __init__(
        self,
        *,
        config: KaganConfig | None = None,
        config_path: Path | None = None,
        db_path: Path | None = None,
    ) -> None:
        self._config = config
        self._config_path = config_path or get_config_path()
        self._db_path = db_path or get_database_path()

        self._status = CoreHostStatus.STOPPED
        self._ctx: AppContext | None = None
        self._ipc_server: IPCServer | None = None
        self._idle_task: asyncio.Task[None] | None = None
        self._lease_heartbeat_task: asyncio.Task[None] | None = None
        self._stop_event = asyncio.Event()
        self._client_count = 0
        self._last_disconnected_time: float | None = None
        self._session_bindings: dict[str, SessionBinding] = {}
        self._idempotency_records: OrderedDict[tuple[str, str], IdempotencyRecord] = OrderedDict()
        self._idempotency_lock = asyncio.Lock()
        self._runtime_version = get_kagan_version()
        self._instance_lock = CoreInstanceLock(
            get_core_instance_lock_path(),
            lease_path=_core_lease_path(),
        )

    @property
    def status(self) -> CoreHostStatus:
        """Current host lifecycle state."""
        return self._status

    @property
    def context(self) -> AppContext | None:
        """The application context, available after start."""
        return self._ctx

    @property
    def client_count(self) -> int:
        """Number of active IPC clients (approximation)."""
        return self._client_count

    def register_session(self, session_id: str, profile: str) -> None:
        """Associate a session with a capability profile.

        Args:
            session_id: The IPC session identifier.
            profile: Profile name (viewer, planner, pair_worker, operator, maintainer).

        Raises:
            ValueError: If *profile* is not a recognized capability profile.
        """
        register_session_binding(self._session_bindings, session_id, profile)

    def unregister_session(self, session_id: str) -> None:
        """Remove the profile binding for a session."""
        unregister_session_binding(self._session_bindings, session_id)

    def _plugin_registry(self) -> PluginRegistry | None:
        if self._ctx is None:
            return None
        return getattr(self._ctx, "plugin_registry", None)

    def _plugin_operation(self, request: CoreRequest) -> PluginOperation | None:
        registry = self._plugin_registry()
        if registry is None:
            return None
        return registry.resolve_operation(request.capability, request.method)

    def _plugin_policy_decision(
        self,
        request: CoreRequest,
        *,
        binding: SessionBinding,
    ) -> PluginPolicyDecision | None:
        registry = self._plugin_registry()
        if registry is None:
            return None
        resolved_profile = CapabilityProfile(binding.policy.profile)
        return registry.evaluate_policy(
            capability=request.capability,
            method=request.method,
            session_id=request.session_id,
            profile=resolved_profile,
            params=request.params,
        )

    async def start(self) -> None:
        """Start the core host: bootstrap context, start IPC server, write runtime files."""
        if self._status != CoreHostStatus.STOPPED:
            msg = f"Cannot start core host in state {self._status.value}"
            raise RuntimeError(msg)

        self._set_status(CoreHostStatus.STARTING)
        if not self._instance_lock.acquire():
            self._set_status(CoreHostStatus.STOPPED)
            msg = "Another core daemon is already running for this runtime directory"
            raise RuntimeError(msg)

        runtime_files_written = False
        try:
            if self._config is None:
                self._config = KaganConfig.load(self._config_path)

            self._ctx = await create_app_context(
                self._config_path,
                self._db_path,
                config=self._config,
            )
            await self._reconcile_startup_runtime_state()
            await self._ctx.automation_service.start()

            await self._ctx.event_bus.publish(CoreHostStarting())

            transport_pref = self._config.general.core_transport_preference
            self._ipc_server = IPCServer(
                handler=self.handle_request,
                transport_preference=transport_pref,
                on_client_connect=self._on_client_connected,
                on_client_disconnect=self._on_client_disconnected,
            )
            handle = await self._ipc_server.start()

            self._write_runtime_files(handle)
            runtime_files_written = True
            self._lease_heartbeat_task = asyncio.create_task(
                self._lease_heartbeat_loop(),
                name="core-lease-heartbeat",
            )

            self._last_disconnected_time = asyncio.get_event_loop().time()
            idle_timeout = self._config.general.core_idle_timeout_seconds
            if idle_timeout > 0:
                self._idle_task = asyncio.create_task(
                    self._idle_watchdog(idle_timeout),
                    name="core-idle-watchdog",
                )

            self._set_status(CoreHostStatus.RUNNING)
            await self._ctx.event_bus.publish(
                CoreHostRunning(
                    transport=handle.transport_type,
                    address=handle.address,
                    port=handle.port,
                )
            )

            logger.info(
                "Core host running: transport=%s address=%s port=%s pid=%d",
                handle.transport_type,
                handle.address,
                handle.port,
                os.getpid(),
            )
        except Exception:  # quality-allow-broad-except
            if self._lease_heartbeat_task is not None:
                self._lease_heartbeat_task.cancel()
                with contextlib.suppress(asyncio.CancelledError):
                    await self._lease_heartbeat_task
                self._lease_heartbeat_task = None
            if runtime_files_written:
                self._cleanup_runtime_files()
            if self._ipc_server is not None:
                with contextlib.suppress(Exception):  # quality-allow-broad-except
                    await self._ipc_server.stop()
                self._ipc_server = None
            if self._ctx is not None:
                with contextlib.suppress(Exception):  # quality-allow-broad-except
                    await self._ctx.close()
                self._ctx = None
            self._instance_lock.release()
            self._set_status(CoreHostStatus.STOPPED)
            raise

    async def stop(self, *, reason: str = "shutdown requested") -> None:
        """Gracefully stop the core host."""
        if self._status in (CoreHostStatus.DRAINING, CoreHostStatus.STOPPED):
            return

        self._set_status(CoreHostStatus.DRAINING)
        if self._ctx:
            await self._ctx.event_bus.publish(CoreHostDraining(reason=reason))

        if self._idle_task is not None:
            self._idle_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._idle_task
            self._idle_task = None

        if self._lease_heartbeat_task is not None:
            self._lease_heartbeat_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._lease_heartbeat_task
            self._lease_heartbeat_task = None

        if self._ipc_server is not None:
            await self._ipc_server.stop()
            self._ipc_server = None

        self._cleanup_runtime_files()

        if self._ctx is not None:
            await self._ctx.close()
            event_bus = self._ctx.event_bus
            self._ctx = None
            await event_bus.publish(CoreHostStopped())

        self._set_status(CoreHostStatus.STOPPED)
        self._client_count = 0
        self._last_disconnected_time = None
        self._idempotency_records.clear()
        self._instance_lock.release()
        self._stop_event.set()

        logger.info("Core host stopped: %s", reason)

    async def wait_until_stopped(self) -> None:
        """Block until the host has fully stopped."""
        await self._stop_event.wait()

    async def handle_request(self, request: CoreRequest) -> CoreResponse:
        """Dispatch an IPC request through the canonical request dispatch map."""
        response: CoreResponse

        if self._ctx is None:
            response = CoreResponse.failure(
                request.request_id,
                code="NOT_READY",
                message="Core host context not initialized",
            )
            return response

        try:
            binding = get_session_binding(self._session_bindings, request)
            plugin_decision = self._plugin_policy_decision(request, binding=binding)
            if plugin_decision is None:
                binding.policy.enforce(request.capability, request.method)
            elif not plugin_decision.allowed:
                raise SessionBindingError(plugin_decision.code, plugin_decision.message)
            enforce_task_scope(request, binding)
        except SessionBindingError as exc:
            response = CoreResponse.failure(
                request.request_id,
                code=exc.code,
                message=str(exc),
            )
            await self._record_audit_event(request, response)
            return response
        except AuthorizationError as exc:
            response = CoreResponse.failure(
                request.request_id,
                code=exc.code,
                message=str(exc),
            )
            await self._record_audit_event(request, response)
            return response

        incompatibility = self._validate_mcp_client_version(request, binding=binding)
        if incompatibility is not None:
            await self._record_audit_event(request, incompatibility)
            return incompatibility

        with request_context(RequestContext(request=request, binding=binding)):
            response = await self._dispatch_with_idempotency(request)
        await self._record_audit_event(request, response)
        return response

    def _validate_mcp_client_version(
        self,
        request: CoreRequest,
        *,
        binding: SessionBinding,
    ) -> CoreResponse | None:
        if binding.origin not in (SessionOrigin.KAGAN, SessionOrigin.KAGAN_ADMIN):
            return None
        client_version = request.client_version.strip() if request.client_version else ""
        if not client_version:
            return CoreResponse.failure(
                request.request_id,
                code="MCP_OUTDATED",
                message=(
                    "MCP client did not report its version. Restart the MCP client/session "
                    "to load the latest kagan package."
                ),
            )
        if client_version != self._runtime_version:
            return CoreResponse.failure(
                request.request_id,
                code="MCP_OUTDATED",
                message=(
                    f"MCP client version '{client_version}' does not match core version "
                    f"'{self._runtime_version}'. Restart the MCP client/session."
                ),
            )
        return None

    @staticmethod
    def _idempotency_cache_key(request: CoreRequest) -> tuple[str, str] | None:
        key = request.idempotency_key
        if key is None:
            return None
        normalized_key = key.strip()
        if not normalized_key:
            return None
        return (request.session_id, normalized_key)

    @staticmethod
    def _idempotency_fingerprint(request: CoreRequest) -> str:
        payload = {
            "capability": request.capability,
            "method": request.method,
            "params": request.params,
        }
        return json.dumps(payload, sort_keys=True, separators=(",", ":"), default=str)

    @staticmethod
    def _to_cached_response(response: CoreResponse) -> CachedResponseEnvelope:
        result: dict[str, Any] | None = None
        if response.result is not None:
            result = copy.deepcopy(response.result)
        if response.ok:
            return CachedResponseEnvelope(
                ok=True,
                result=result,
                error_code=None,
                error_message=None,
            )
        error = response.error
        return CachedResponseEnvelope(
            ok=False,
            result=result,
            error_code=error.code if error is not None else "UNKNOWN_ERROR",
            error_message=error.message if error is not None else "Unknown error",
        )

    @staticmethod
    def _from_cached_response(
        request_id: str,
        cached: CachedResponseEnvelope,
    ) -> CoreResponse:
        if cached.ok:
            result = copy.deepcopy(cached.result) if cached.result is not None else None
            return CoreResponse.success(request_id, result=result)
        return CoreResponse.failure(
            request_id,
            code=cached.error_code or "UNKNOWN_ERROR",
            message=cached.error_message or "Unknown error",
        )

    def _trim_idempotency_cache(self) -> None:
        overflow = len(self._idempotency_records) - IDEMPOTENCY_CACHE_LIMIT
        if overflow <= 0:
            return
        for cache_key in list(self._idempotency_records.keys()):
            if overflow <= 0:
                break
            record = self._idempotency_records[cache_key]
            if record.pending is not None:
                continue
            self._idempotency_records.pop(cache_key, None)
            overflow -= 1

    async def _dispatch_with_idempotency(self, request: CoreRequest) -> CoreResponse:
        cache_key = self._idempotency_cache_key(request)
        if cache_key is None or not self._is_idempotent_mutation(request):
            return await self._dispatch_request(request)

        fingerprint = self._idempotency_fingerprint(request)
        reservation_or_response = await self._reserve_idempotency(
            request=request,
            cache_key=cache_key,
            fingerprint=fingerprint,
        )
        if isinstance(reservation_or_response, CoreResponse):
            return reservation_or_response

        reservation = reservation_or_response
        if not reservation.owner:
            cached = await reservation.pending
            return self._from_cached_response(request.request_id, cached)

        return await self._dispatch_idempotent_owner(request, reservation)

    async def _reserve_idempotency(
        self,
        *,
        request: CoreRequest,
        cache_key: tuple[str, str],
        fingerprint: str,
    ) -> IdempotencyReservation | CoreResponse:
        async with self._idempotency_lock:
            existing = self._idempotency_records.get(cache_key)
            if existing is None:
                pending = asyncio.get_running_loop().create_future()
                self._idempotency_records[cache_key] = IdempotencyRecord(
                    fingerprint=fingerprint,
                    pending=pending,
                )
                self._idempotency_records.move_to_end(cache_key)
                return IdempotencyReservation(
                    cache_key=cache_key,
                    fingerprint=fingerprint,
                    pending=pending,
                    owner=True,
                )
            if existing.fingerprint != fingerprint:
                return CoreResponse.failure(
                    request.request_id,
                    code="INVALID_PARAMS",
                    message=(
                        "idempotency_key cannot be reused with different capability/method/params"
                    ),
                )
            if existing.response is not None:
                self._idempotency_records.move_to_end(cache_key)
                return self._from_cached_response(request.request_id, existing.response)
            if existing.pending is None:
                existing.pending = asyncio.get_running_loop().create_future()
                owner = True
            else:
                owner = False

            self._idempotency_records.move_to_end(cache_key)
            assert existing.pending is not None
            return IdempotencyReservation(
                cache_key=cache_key,
                fingerprint=fingerprint,
                pending=existing.pending,
                owner=owner,
            )

    async def _dispatch_idempotent_owner(
        self,
        request: CoreRequest,
        reservation: IdempotencyReservation,
    ) -> CoreResponse:
        try:
            response = await self._dispatch_request(request)
        except BaseException as exc:
            await self._cancel_idempotency_pending(
                cache_key=reservation.cache_key,
                pending=reservation.pending,
                error=exc,
            )
            raise

        cached = self._to_cached_response(response)
        await self._store_idempotency_response(
            cache_key=reservation.cache_key,
            fingerprint=reservation.fingerprint,
            pending=reservation.pending,
            cached=cached,
        )
        return response

    async def _cancel_idempotency_pending(
        self,
        *,
        cache_key: tuple[str, str],
        pending: asyncio.Future[CachedResponseEnvelope],
        error: BaseException,
    ) -> None:
        async with self._idempotency_lock:
            record = self._idempotency_records.get(cache_key)
            if record is None or record.pending is not pending:
                return
            self._idempotency_records.pop(cache_key, None)
            if not pending.done():
                pending.set_exception(error)

    async def _store_idempotency_response(
        self,
        *,
        cache_key: tuple[str, str],
        fingerprint: str,
        pending: asyncio.Future[CachedResponseEnvelope],
        cached: CachedResponseEnvelope,
    ) -> None:
        async with self._idempotency_lock:
            record = self._idempotency_records.get(cache_key)
            if record is None:
                self._idempotency_records[cache_key] = IdempotencyRecord(
                    fingerprint=fingerprint,
                    response=cached,
                )
            else:
                record.response = cached
                if record.pending is pending and not pending.done():
                    pending.set_result(cached)
                record.pending = None
            self._idempotency_records.move_to_end(cache_key)
            self._trim_idempotency_cache()

    def _is_idempotent_mutation(self, request: CoreRequest) -> bool:
        if (request.capability, request.method) in IDEMPOTENT_MUTATION_METHODS:
            return True
        operation = self._plugin_operation(request)
        if operation is None:
            return False
        return operation.mutating

    async def _dispatch_request(self, request: CoreRequest) -> CoreResponse:
        plugin_result: dict[str, Any] | None = None
        try:
            handle_result = await self._try_request_dispatch(request)
            if handle_result is None:
                plugin_result = await self._try_plugin_dispatch(request)
        except KeyError as exc:
            return CoreResponse.failure(
                request.request_id,
                code="INVALID_PARAMS",
                message=f"Missing required parameter: {exc}",
            )
        except ValueError as exc:
            return CoreResponse.failure(
                request.request_id,
                code="INVALID_PARAMS",
                message=str(exc),
            )
        except Exception:  # quality-allow-broad-except
            logger.exception("Handler error for %s.%s", request.capability, request.method)
            return CoreResponse.failure(
                request.request_id,
                code="INTERNAL_ERROR",
                message=f"Internal error processing {request.capability}.{request.method}",
            )

        if handle_result is not None:
            return CoreResponse.success(request.request_id, result=handle_result)
        if plugin_result is not None:
            return CoreResponse.success(request.request_id, result=plugin_result)

        return CoreResponse.failure(
            request.request_id,
            code="UNKNOWN_METHOD",
            message=f"No handler for {request.capability}.{request.method}",
        )

    async def _try_request_dispatch(
        self,
        request: CoreRequest,
    ) -> dict[str, Any] | None:
        """Attempt to dispatch a request through the KaganAPI.

        Returns the formatted result dict if the (capability, method) pair is
        in the request dispatch map, or ``None`` when no built-in handler
        exists for that pair.

        Authorization has already been enforced before this method is called.
        """
        global _REQUEST_DISPATCH_MAP
        assert self._ctx is not None
        api = getattr(self._ctx, "api", None)
        if api is None:
            return None

        if _REQUEST_DISPATCH_MAP is None:
            _REQUEST_DISPATCH_MAP = build_request_dispatch_map()

        key = (request.capability, request.method)
        if key not in _REQUEST_DISPATCH_MAP:
            return None

        handler = _REQUEST_DISPATCH_MAP[key]
        return await handler(api, request.params)

    async def _try_plugin_dispatch(
        self,
        request: CoreRequest,
    ) -> dict[str, Any] | None:
        """Attempt to dispatch a request through the plugin registry."""
        assert self._ctx is not None
        operation = self._plugin_operation(request)
        if operation is None:
            return None
        return await operation.handler(self._ctx, request.params)

    async def _record_audit_event(self, request: CoreRequest, response: CoreResponse) -> None:
        """Persist an immutable audit event for every handled request."""
        if self._ctx is None or not hasattr(self._ctx, "audit_repository"):
            return
        try:
            result_payload = response.result or {}
            if not response.ok and response.error is not None:
                result_payload = {
                    "error": {
                        "code": response.error.code,
                        "message": response.error.message,
                    }
                }
            binding = self._session_bindings.get(request.session_id)
            payload: dict[str, object] = {
                "params": request.params,
                "requested_profile": request.session_profile,
                "requested_origin": request.session_origin,
            }
            if binding is not None:
                payload["effective_profile"] = binding.policy.profile
                payload["effective_origin"] = binding.origin
                payload["namespace"] = binding.namespace
            operation_success = response.ok
            if response.ok and isinstance(result_payload, dict):
                nested_success = result_payload.get("success")
                if isinstance(nested_success, bool):
                    operation_success = nested_success
            await self._ctx.audit_repository.record(
                actor_type="session",
                actor_id=request.session_id,
                session_id=request.session_id,
                capability=request.capability,
                command_name=request.method,
                payload_json=json.dumps(payload, default=str),
                result_json=json.dumps(result_payload, default=str),
                success=operation_success,
            )
        except Exception:  # quality-allow-broad-except
            logger.exception(
                "Failed to record audit event for %s.%s", request.capability, request.method
            )

    async def _idle_watchdog(self, timeout_seconds: int) -> None:
        """Periodically stop core after all clients remain disconnected."""
        check_interval = min(timeout_seconds / 4, 30.0)
        loop = asyncio.get_event_loop()
        try:
            while True:
                await asyncio.sleep(check_interval)
                if self._client_count > 0:
                    continue
                if self._last_disconnected_time is None:
                    self._last_disconnected_time = loop.time()
                    continue
                elapsed = loop.time() - self._last_disconnected_time
                if elapsed >= timeout_seconds:
                    logger.info(
                        "Idle timeout reached with no clients (%.0fs >= %ds), shutting down",
                        elapsed,
                        timeout_seconds,
                    )
                    await self.stop(reason="idle timeout")
                    return
        except asyncio.CancelledError:
            raise

    def _on_client_connected(self) -> None:
        """Track active client count for idle shutdown decisions."""
        self._client_count += 1
        self._last_disconnected_time = None

    def _on_client_disconnected(self) -> None:
        """Track transitions to zero active clients for idle shutdown."""
        if self._client_count > 0:
            self._client_count -= 1
        if self._client_count == 0:
            self._last_disconnected_time = asyncio.get_event_loop().time()

    async def _lease_heartbeat_loop(self) -> None:
        while True:
            await asyncio.sleep(CORE_LEASE_HEARTBEAT_SECONDS)
            try:
                self._instance_lock.heartbeat()
            except OSError:
                logger.warning("Failed to write core lease heartbeat", exc_info=True)

    async def _reconcile_startup_runtime_state(self) -> None:
        """Reconcile persisted runtime context and projections at startup."""
        if self._ctx is None:
            return
        await self._ctx.runtime_service.reconcile_startup_state()
        tasks = await self._ctx.task_service.list_tasks()
        auto_task_ids = [task.id for task in tasks if task.task_type is TaskType.AUTO]
        await self._ctx.runtime_service.reconcile_running_tasks(auto_task_ids)

    def _write_runtime_files(self, handle: ServerHandle) -> None:
        """Write endpoint and token files for client discovery."""
        runtime_dir = get_core_runtime_dir()
        runtime_dir.mkdir(parents=True, exist_ok=True)

        assert self._ipc_server is not None
        get_core_token_path().write_text(self._ipc_server.token, encoding="utf-8")

        endpoint_data: dict[str, str | int] = {
            "transport": handle.transport_type,
            "address": handle.address,
        }
        if handle.port is not None:
            endpoint_data["port"] = handle.port
        get_core_endpoint_path().write_text(
            json.dumps(endpoint_data, indent=2),
            encoding="utf-8",
        )

    @staticmethod
    def _cleanup_runtime_files() -> None:
        """Remove runtime files on shutdown."""
        for path_fn in (
            get_core_endpoint_path,
            get_core_token_path,
            _core_lease_path,
        ):
            with contextlib.suppress(OSError):
                path_fn().unlink(missing_ok=True)

    def _set_status(self, new_status: CoreHostStatus) -> None:
        old = self._status
        self._status = new_status
        logger.debug("Core host status: %s -> %s", old.value, new_status.value)


__all__ = ["CoreHost", "CoreHostStatus"]
