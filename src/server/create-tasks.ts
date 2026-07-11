import type { PluginInput } from "@opencode-ai/plugin"
import { commandPlan, configuredScopes, sanitizeTaskScope, type TaskScope } from "../domain/task/commands"
import { nextTaskNumber } from "../domain/task/metadata"
import { createBoardTask } from "../task/create"
import { currentBranch, shellGitRunner } from "../git/runner"
import { listSessions } from "./data"

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
  const run = shellGitRunner(input.$)
  const defaultBranch = (await currentBranch(run, input.worktree)) ?? "HEAD"
  const sessions = await listSessions(input)
  let nextNumber = nextTaskNumber(sessions)
  const setupCommands = commandPlan(options, "setup")

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
        createSession: async (payload) => {
          const createdSession = await input.client.session.create({
            query: { directory: payload.directory },
            body: {
              title: payload.title,
              ...(payload.model
                ? { model: { providerID: payload.model.providerID, modelID: payload.model.modelID } }
                : {}),
              metadata: payload.metadata,
            },
            throwOnError: true,
          } as Parameters<typeof input.client.session.create>[0])
          const id = createdSession.data?.id
          if (!id) throw new Error("session.create returned no id")
          return { id }
        },
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
