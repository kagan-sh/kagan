import type { PluginInput } from "@opencode-ai/plugin"
import { composeStartPrompt } from "../domain/handoff"
import { lastAssistantText } from "../domain/session/messages"
import { countInProgressForMove, columnMoveDenyReason, inProgressCap } from "../domain/task/policy"
import { kagan } from "../domain/task/metadata"
import type { ColumnType } from "../domain/task/types"
import { patchKagan } from "./session/patch"
import {
  errorMessage,
  extractErrorMessage,
  getSessionData,
  getStatus,
  listSessions,
  resolveTaskRefs,
  sessionMessages,
  type EventInfo,
} from "./data"
import { handleHelperEvent, owningRootTaskID, resolveOwningBoardTask } from "./helpers/events"
import { onEnterBacklog, onEnterReview } from "./helpers/spawn"

async function listInProgressCount(input: PluginInput, sessionID: string, source: ColumnType): Promise<number> {
  return countInProgressForMove(
    (await listSessions(input)).map((session) => ({
      id: session.id,
      parentID: session.parentID,
      status: getStatus(session.metadata),
    })),
    sessionID,
    source,
  )
}

async function handleSessionCreated(input: PluginInput, event: { properties: { info: unknown } }): Promise<void> {
  const info = event.properties.info as EventInfo
  const view = kagan(info.metadata)
  if (!view.role && !info.parentID && view.boardTask === true && view.lastGatedStatus === undefined) {
    await patchKagan(input.client, info.id, { lastGatedStatus: getStatus(info.metadata) })
  }
}

async function startTask(input: PluginInput, info: EventInfo): Promise<void> {
  const view = kagan(info.metadata)
  await patchKagan(input.client, info.id, { startedAt: Date.now() })
  try {
    const references = await resolveTaskRefs(input, view.description)
    const prompt = composeStartPrompt(info.title ?? "", info.metadata)
    const body: Record<string, unknown> = {
      parts: [{ type: "text", text: references ? `${prompt}\n\n${references}` : prompt }],
    }
    if (view.model) body.model = { providerID: view.model.providerID, modelID: view.model.modelID }
    await input.client.session.promptAsync({
      path: { id: info.id },
      body,
      throwOnError: true,
    } as Parameters<typeof input.client.session.promptAsync>[0])
  } catch (error) {
    console.error(`[kagan] auto-start prompt failed for ${info.id}, reverting to backlog: ${errorMessage(error)}`)
    await patchKagan(input.client, info.id, { startedAt: undefined, status: "backlog", lastGatedStatus: "backlog" })
  }
}

async function handleSessionUpdated(
  input: PluginInput,
  event: { properties: { info: unknown } },
  options: Record<string, unknown> | undefined,
): Promise<void> {
  const info = event.properties.info as EventInfo
  const view = kagan(info.metadata)
  if (view.role || info.parentID) return
  const status = getStatus(info.metadata)
  const previous = view.lastGatedStatus
  if (previous === undefined && view.boardTask === true)
    await patchKagan(input.client, info.id, { lastGatedStatus: status })
  if (previous !== undefined && status !== previous) {
    const reason = columnMoveDenyReason(status, info.metadata, {
      inProgressCount: await listInProgressCount(input, info.id, previous),
      source: previous,
      cap: inProgressCap(options),
    })
    if (reason) return patchKagan(input.client, info.id, { status: previous })
    if (view.boardTask === true) await patchKagan(input.client, info.id, { lastGatedStatus: status })
    if (status === "in_progress" && view.startedAt === undefined && view.boardTask === true)
      await startTask(input, info)
  }
  if (status === "backlog") await onEnterBacklog(input, info.id, options)
  if (status === "review") await onEnterReview(input, info.id, options)
}

async function handleSessionError(
  input: PluginInput,
  event: { properties: { sessionID?: string; error?: unknown } },
  options?: Record<string, unknown>,
): Promise<void> {
  const { sessionID, error } = event.properties
  if (!sessionID) return
  const session = await getSessionData(input, sessionID)
  const role = kagan(session?.metadata).role
  if (role === "intake" || role === "validator")
    await handleHelperEvent(input, role, sessionID, session?.metadata, extractErrorMessage(error), options)
}

async function handlePermission(
  input: PluginInput,
  sessionID: string,
  awaitingInput: { id: string; title: string } | undefined,
): Promise<void> {
  const rootID = await resolveOwningBoardTask(input, sessionID)
  if (rootID) await patchKagan(input.client, rootID, { awaitingInput })
}

async function handleSessionIdle(
  input: PluginInput,
  event: { properties: { sessionID: string } },
  options?: Record<string, unknown>,
): Promise<void> {
  const sessionID = event.properties.sessionID
  const session = await getSessionData(input, sessionID)
  const role = kagan(session?.metadata).role
  if (role === "intake" || role === "validator") {
    return handleHelperEvent(
      input,
      role,
      sessionID,
      session?.metadata,
      role === "intake"
        ? "intake finished without recording an assessment"
        : "review finished without recording findings",
      options,
    )
  }
  const rootID = owningRootTaskID(session?.metadata, sessionID, session?.parentID)
  if (!rootID) return
  const root = rootID === sessionID ? session : await getSessionData(input, rootID)
  const view = kagan(root?.metadata)
  if (getStatus(root?.metadata) !== "in_progress" || view.startedAt === undefined) return
  if (view.activeIteration !== undefined && view.activeIteration !== sessionID) return
  const report = lastAssistantText(await sessionMessages(input, sessionID))
  await patchKagan(input.client, rootID, { status: "review", awaitingInput: undefined, ...(report ? { report } : {}) })
}

export function createServerEvents(input: PluginInput, options: Record<string, unknown> | undefined) {
  return async ({ event }: { event: { type: string; properties: any } }) => {
    switch (event.type) {
      case "session.created":
        return handleSessionCreated(input, event)
      case "session.updated":
        return handleSessionUpdated(input, event, options)
      case "session.error":
        return handleSessionError(input, event, options)
      case "permission.updated":
        return handlePermission(input, event.properties.sessionID, {
          id: event.properties.id,
          title: event.properties.title,
        })
      case "permission.replied":
        return handlePermission(input, event.properties.sessionID, undefined)
      case "session.idle":
        return handleSessionIdle(input, event, options)
    }
  }
}
