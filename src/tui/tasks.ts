import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Session } from "@opencode-ai/sdk/v2"
import { kagan, nextTaskNumber } from "../domain/task/metadata"
import type { CommandSpec, ModelRef } from "../domain/task/types"
import { bunGitRunner, currentBranch } from "../git/runner"
import { createBoardTask } from "../task/create"
import { listSessions } from "./session/tasks"
import type { TaskScope } from "../domain/task/commands"
import { composeHandoffPrompt } from "../domain/handoff"
import { nextGenerationPatch } from "../domain/task/policy"
import { lastAssistantText } from "../domain/session/messages"
import { worktreeDiffs } from "../git/diffs"
import { mergeTaskBranch, type MergeResult } from "../git/merge"
import { tuiPatchKagan } from "./session/patch"
import type { BoardSession } from "./types"

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
  const taskNumber = nextTaskNumber(existing)
  const result = await createBoardTask({
    run: bunGitRunner,
    mainWorktree: api.state.path.worktree,
    title: input.title,
    description: input.description,
    baseBranch: input.baseBranch,
    taskNumber,
    model: input.model,
    scope: input.scope,
    setupCommands: input.setupCommands ?? [],
    createSession: async (payload) => {
      const created = await api.client.session.create(
        {
          directory: payload.directory,
          title: payload.title,
          ...(payload.model ? { model: { id: payload.model.modelID, providerID: payload.model.providerID } } : {}),
          metadata: payload.metadata,
        },
        { throwOnError: true },
      )
      return { id: created.data.id }
    },
  })
  return { id: result.id } as Session
}

async function stopSession(api: TuiPluginApi, id: string): Promise<void> {
  try {
    await api.client.session.abort({ sessionID: id }, { throwOnError: true })
  } catch {
    // already idle or gone
  }
}

async function removeSession(api: TuiPluginApi, id: string): Promise<void> {
  try {
    await api.client.session.delete({ sessionID: id }, { throwOnError: true })
  } catch {
    // may already be removed with the parent
  }
}

async function relatedSessionIDs(api: TuiPluginApi, sessionID: string): Promise<Set<string>> {
  const related = new Set<string>()
  try {
    const result = await api.client.session.get({ sessionID }, { throwOnError: true })
    const view = kagan(result.data?.metadata)
    if (view.intakeSessionID) related.add(view.intakeSessionID)
    if (view.validatorSessionID) related.add(view.validatorSessionID)
    if (view.activeIteration && view.activeIteration !== sessionID) related.add(view.activeIteration)
  } catch {
    // proceed with delete anyway
  }
  try {
    const children = await api.client.session.children({ sessionID }, { throwOnError: true })
    for (const child of children.data ?? []) if (child.id) related.add(child.id)
  } catch {
    // children lookup is best-effort
  }
  related.delete(sessionID)
  return related
}

export async function deleteSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  const related = await relatedSessionIDs(api, sessionID)
  await Promise.all([...related, sessionID].map((id) => stopSession(api, id)))
  for (const id of related) await removeSession(api, id)
  await api.client.session.delete({ sessionID }, { throwOnError: true })
}

export async function sendBack(api: TuiPluginApi, session: BoardSession): Promise<void> {
  const metadata = session.metadata as Record<string, unknown> | undefined
  const view = kagan(metadata)
  const worktree = view.worktree
  if (!worktree) throw new Error("Task has no isolated worktree")
  const baseBranch = view.baseBranch ?? "HEAD"
  const previousID = view.activeIteration ?? session.id
  const messages = await api.client.session.messages({ sessionID: previousID }, { throwOnError: true })
  const previousReport = lastAssistantText(messages.data ?? [])
  const changedFiles = (await worktreeDiffs(bunGitRunner, worktree, baseBranch))
    .map((diff) => diff.file)
    .filter((file): file is string => typeof file === "string")
  const worker = await api.client.session.create(
    {
      directory: worktree,
      parentID: session.id,
      title: `iteration ${view.generation + 1}`,
      metadata: { kagan: { role: "worker", workerParent: session.id } },
    },
    { throwOnError: true },
  )
  await api.client.session.promptAsync(
    {
      sessionID: worker.data.id,
      ...(view.model ? { model: view.model } : {}),
      parts: [
        { type: "text", text: composeHandoffPrompt({ title: session.title, metadata, previousReport, changedFiles }) },
      ],
    },
    { throwOnError: true },
  )
  await tuiPatchKagan(api, session.id, {
    ...nextGenerationPatch(metadata),
    status: "in_progress",
    activeIteration: worker.data.id,
  })
}

export async function mergeTask(
  api: TuiPluginApi,
  session: BoardSession,
  targetBranch: string,
  squash: boolean,
): Promise<MergeResult> {
  const worktree = kagan(session.metadata as Record<string, unknown> | undefined).worktree
  if (!worktree) return { ok: false, message: "Task has no isolated worktree" }
  const runner = bunGitRunner
  const branch = await currentBranch(runner, worktree)
  if (!branch) return { ok: false, message: "Cannot determine the task branch" }
  return mergeTaskBranch(
    runner,
    api.state.path.worktree,
    worktree,
    branch,
    targetBranch,
    `kagan: ${session.title}`,
    squash,
  )
}
