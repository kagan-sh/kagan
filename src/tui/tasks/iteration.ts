import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { bunGitRunner, currentBranch } from "../../git/runner"
import { mergeTaskBranch } from "../../git/merge"
import type { MergeResult } from "../../git/runner"
import { worktreeDiffs } from "../../git/diffs"
import { composeHandoffPrompt } from "../../domain/handoff"
import { lastAssistantText } from "../../domain/session/messages"
import { tuiPatchKagan } from "../session/patch"
import { kagan } from "../../domain/task/metadata"
import { nextGenerationPatch } from "../../domain/task/policy"
import type { BoardSession } from "../types"

export async function sendBack(api: TuiPluginApi, session: BoardSession): Promise<void> {
  const metadata = session.metadata as Record<string, unknown> | undefined
  const view = kagan(metadata)
  const worktree = view.worktree
  if (!worktree) throw new Error("Task has no isolated worktree")
  const baseBranch = view.baseBranch ?? "HEAD"
  const previousID = view.activeIteration ?? session.id
  const messages = await api.client.session.messages({ sessionID: previousID }, { throwOnError: true })
  const previousReport = lastAssistantText(messages.data ?? [])
  const changedFiles = (await worktreeDiffs(bunGitRunner(), worktree, baseBranch))
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
  const runner = bunGitRunner()
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
