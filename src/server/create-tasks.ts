import type { PluginInput } from "@opencode-ai/plugin"
import { commandPlan, configuredScopes, sanitizeTaskScope, type TaskScope } from "../domain/task/commands"
import type { CommandSpec } from "../domain/task/types"
import { nextTaskNumber } from "../domain/task/metadata"
import { createBoardTask, type CreateSessionPayload } from "../task/create"
import { bunGitRunner, currentBranch, type GitRunner } from "../git/runner"
import { listSessions } from "./data"

const serializeTails = new Map<string, Promise<unknown>>()

function serializeByKey<T>(key: string, task: () => Promise<T>): Promise<T> {
  const prior = serializeTails.get(key) ?? Promise.resolve()
  const run = prior.then(task, task)
  const settled = run.then(
    () => undefined,
    () => undefined,
  )
  serializeTails.set(key, settled)
  void settled.then(() => {
    if (serializeTails.get(key) === settled) serializeTails.delete(key)
  })
  return run
}

export type CreateTaskTicket = {
  title: string
  description: string
  baseBranch?: string
  scope?: { values?: string[]; custom?: string }
}

function nonBlank(value: string, label: string): string {
  const trimmed = value.trim()
  if (!trimmed) throw new Error(`${label} is required`)
  return trimmed
}

function validateTicketScope(scope: unknown, allowed: readonly string[]): TaskScope | undefined {
  if (!scope) return undefined
  const sanitized = sanitizeTaskScope(scope)
  if (!sanitized) return undefined
  if (allowed.length === 0) {
    if (sanitized.values.length > 0) throw new Error("No configured scopes exist for this project")
    return sanitized.custom ? sanitized : undefined
  }
  for (const value of sanitized.values) {
    if (!allowed.includes(value)) throw new Error(`Scope "${value}" is not configured for this project`)
  }
  if (sanitized.custom && !allowed.includes(sanitized.custom)) {
    throw new Error(`Custom scope "${sanitized.custom}" is not a configured command cwd`)
  }
  return sanitized
}

async function createSessionViaClient(input: PluginInput, payload: CreateSessionPayload): Promise<{ id: string }> {
  const created = await input.client.session.create({
    query: { directory: payload.directory },
    body: {
      title: payload.title,
      ...(payload.model ? { model: { providerID: payload.model.providerID, modelID: payload.model.modelID } } : {}),
      metadata: payload.metadata,
    },
    throwOnError: true,
  } as Parameters<typeof input.client.session.create>[0])
  const id = created.data?.id
  if (!id) throw new Error("session.create returned no id")
  return { id }
}

export function ticketSummary(tickets: CreateTaskTicket[]): Array<{ title: string; baseBranch?: string }> {
  return tickets.map((ticket) => ({
    title: ticket.title.trim(),
    ...(ticket.baseBranch ? { baseBranch: ticket.baseBranch.trim() } : {}),
  }))
}

export async function runCreateTasks(
  input: PluginInput,
  options: Record<string, unknown> | undefined,
  tickets: CreateTaskTicket[],
): Promise<string> {
  if (tickets.length < 1 || tickets.length > 10) throw new Error("Provide between 1 and 10 tickets")

  const allowedScopes = configuredScopes(options)
  const run = bunGitRunner
  const defaultBranch = (await currentBranch(run, input.worktree)) ?? "HEAD"
  const setupCommands = commandPlan(options, "setup")

  // Serialize the whole run per project so overlapping bulk requests can't read the same session
  // snapshot and mint duplicate task numbers — numbers are only unique once the prior run's sessions exist.
  return serializeByKey(input.worktree, () =>
    createSerially(input, run, allowedScopes, defaultBranch, setupCommands, tickets),
  )
}

async function createSerially(
  input: PluginInput,
  run: GitRunner,
  allowedScopes: readonly string[],
  defaultBranch: string,
  setupCommands: CommandSpec[],
  tickets: CreateTaskTicket[],
): Promise<string> {
  const sessions = await listSessions(input)
  let nextNumber = nextTaskNumber(sessions)

  const lines: string[] = []
  let created = 0

  for (const [index, raw] of tickets.entries()) {
    const title = nonBlank(raw.title, "Title")
    const description = nonBlank(raw.description, "Description")
    const baseBranch = raw.baseBranch?.trim() || defaultBranch
    try {
      const scope = validateTicketScope(raw.scope, allowedScopes)
      const taskNumber = nextNumber
      const result = await createBoardTask({
        run,
        mainWorktree: input.worktree,
        title,
        description,
        baseBranch,
        taskNumber,
        scope,
        setupCommands,
        createSession: (payload) => createSessionViaClient(input, payload),
      })
      created++
      nextNumber++
      lines.push(`- **#${taskNumber} ${title}** — created (\`${result.id}\`)`)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      lines.push(`- **${title || `Ticket ${index + 1}`}** — failed: ${message}`)
    }
  }

  return [`Created ${created} of ${tickets.length} task(s):`, ...lines].join("\n")
}
