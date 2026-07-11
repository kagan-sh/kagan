import { runCommandPlan, truncateCheckResultForMetadata } from "../checks/runner"
import { commandInTaskScope } from "../domain/task/commands"
import { buildTaskMetadata } from "../domain/task/metadata"
import type { CommandSpec, ModelRef } from "../domain/task/types"
import type { TaskScope } from "../domain/task/commands"
import { createTaskWorktree, ensureWorktreePluginConfig, uniqueTaskSlug, type GitRunner } from "../git/runner"

export type CreateSessionPayload = {
  directory: string
  title: string
  model?: { providerID: string; modelID: string }
  metadata: { kagan: Record<string, unknown> }
}

export async function createBoardTask(input: {
  run: GitRunner
  mainWorktree: string
  title: string
  description: string
  baseBranch: string
  taskNumber: number
  model?: ModelRef
  scope?: TaskScope
  setupCommands: CommandSpec[]
  createSession: (payload: CreateSessionPayload) => Promise<{ id: string }>
}): Promise<{ id: string }> {
  const slug = uniqueTaskSlug(input.title)
  const { directory } = await createTaskWorktree(input.run, input.mainWorktree, slug, input.baseBranch)
  await ensureWorktreePluginConfig(directory)
  const setup = await runCommandPlan(
    input.setupCommands,
    directory,
    (command) => commandInTaskScope(command, input.scope),
    "task scope does not include this cwd",
    false,
  )
  const patch = buildTaskMetadata({
    taskNumber: input.taskNumber,
    baseBranch: input.baseBranch,
    worktree: directory,
    description: input.description,
    model: input.model,
    scope: input.scope,
    setup: setup ? truncateCheckResultForMetadata(setup) : undefined,
  })
  const payload: CreateSessionPayload = {
    directory,
    title: input.title,
    ...(input.model ? { model: { providerID: input.model.providerID, modelID: input.model.modelID } } : {}),
    metadata: { kagan: patch },
  }
  return input.createSession(payload)
}
