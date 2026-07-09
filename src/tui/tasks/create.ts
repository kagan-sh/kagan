import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Session } from "@opencode-ai/sdk/v2"
import { runCommandPlan, truncateCheckResultForMetadata, type CommandSpec } from "../../checks/runner"
import { bunGitRunner, createTaskWorktree, ensureWorktreePluginConfig, uniqueTaskSlug } from "../../git/runner"
import { listSessions } from "../session/tasks"
import { commandInTaskScope } from "../../domain/task/commands"
import { kagan } from "../../domain/task/metadata"
import type { ModelRef } from "../../domain/task/types"
import type { TaskScope } from "../../domain/task/commands"

export async function createTask(
  api: TuiPluginApi,
  input: {
    title: string
    description: string
    model?: ModelRef
    baseBranch: string
    setupCommands?: CommandSpec[]
    scope?: TaskScope
  },
): Promise<Session> {
  const existing = await listSessions(api)
  const taskNumber = existing.reduce((max, session) => Math.max(max, kagan(session.metadata).taskNumber ?? 0), 0) + 1
  const slug = uniqueTaskSlug(input.title)
  const { directory } = await createTaskWorktree(bunGitRunner(), api.state.path.worktree, slug, input.baseBranch)
  await ensureWorktreePluginConfig(directory)
  const description = input.description.trim()
  const patch: Record<string, unknown> = {
    status: "backlog",
    boardTask: true,
    taskNumber,
    baseBranch: input.baseBranch,
    worktree: directory,
  }
  if (description) patch.description = description
  if (input.model) patch.model = input.model
  if (input.scope) patch.scope = input.scope
  const setup = await runCommandPlan(
    input.setupCommands ?? [],
    directory,
    (command) => commandInTaskScope(command, input.scope),
    "task scope does not include this cwd",
    false,
  )
  if (setup) patch.setup = truncateCheckResultForMetadata(setup)
  const result = await api.client.session.create(
    {
      directory,
      title: input.title,
      ...(input.model ? { model: { id: input.model.modelID, providerID: input.model.providerID } } : {}),
      metadata: { kagan: patch },
    },
    { throwOnError: true },
  )
  return result.data
}
