import type { PluginInput } from "@opencode-ai/plugin"
import { formatTaskRef, parseTaskRefs } from "../domain/handoff"
import { kagan } from "../domain/task/metadata"
import type { ColumnType } from "../domain/task/types"

type SessionData = {
  title?: string
  metadata?: Record<string, unknown>
  parentID?: string | null
}

export type EventInfo = SessionData & { id: string }

type ListedSession = SessionData & { id: string }

export function getStatus(metadata?: Record<string, unknown>): ColumnType {
  return kagan(metadata).status ?? "backlog"
}

export async function getSessionData(input: PluginInput, sessionID: string): Promise<SessionData | undefined> {
  const result = await input.client.session.get({ path: { id: sessionID }, throwOnError: true })
  return result.data as SessionData | undefined
}

export async function listSessions(input: PluginInput): Promise<ListedSession[]> {
  const result = await input.client.session.list({
    query: { scope: "project" },
    throwOnError: true,
  } as Parameters<typeof input.client.session.list>[0])
  return (result.data ?? []) as ListedSession[]
}

export async function sessionMessages(input: PluginInput, sessionID: string): Promise<unknown> {
  return input.client.session
    .messages({ path: { id: sessionID }, throwOnError: true })
    .then((result) => result.data)
    .catch(() => undefined)
}

export async function resolveTaskRefs(
  input: PluginInput,
  description: string | undefined,
): Promise<string | undefined> {
  if (!description) return undefined
  const refs = parseTaskRefs(description)
  if (refs.length === 0) return undefined
  try {
    const byNumber = new Map<number, ListedSession>()
    for (const session of await listSessions(input)) {
      if (session.parentID) continue
      const number = kagan(session.metadata).taskNumber
      if (number !== undefined) byNumber.set(number, session)
    }
    return refs
      .map((number) => {
        const session = byNumber.get(number)
        if (!session) return formatTaskRef({ number })
        const view = kagan(session.metadata)
        return formatTaskRef({
          number,
          title: session.title ?? "",
          status: getStatus(session.metadata),
          understanding: view.intake?.understanding,
          report: view.report,
        })
      })
      .join("\n\n")
  } catch {
    return undefined
  }
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function extractErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    const data = (error as { data?: unknown }).data
    const message = typeof data === "object" && data !== null ? (data as { message?: unknown }).message : undefined
    if (typeof message === "string" && message.trim()) return message
    const name = (error as { name?: unknown }).name
    if (typeof name === "string" && name.trim()) return name
  }
  return "unknown error"
}
