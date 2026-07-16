/** @jsxImportSource @opentui/solid */
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { archiveSession, retryHelper } from "../../session/tasks"
import { canRestartHelper } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import { bunGitRunner } from "../../../git/runner"
import { worktreeDiffs } from "../../../git/diffs"
import { buildTaskDetails } from "../../dialogs/task-details"
import { openTaskDetailsView } from "../../dialogs/task-details-view"
import type { BoardSession } from "../../types"
import type { BoardActions } from "./context"

const taskDetailsDiffs = async (metadata: Record<string, unknown> | undefined): Promise<Array<SnapshotFileDiff>> => {
  const worktree = kagan(metadata).worktree
  if (!worktree) return []
  try {
    return await worktreeDiffs(bunGitRunner(), worktree, kagan(metadata).baseBranch ?? "HEAD")
  } catch {
    return []
  }
}

export const viewDetails = async (ctx: BoardActions, session: BoardSession) => {
  const details = buildTaskDetails(session.metadata ?? {}, await taskDetailsDiffs(session.metadata), session.title)
  const title = details.taskNumber !== undefined ? `#${details.taskNumber} ${session.title}` : session.title
  openTaskDetailsView(ctx.api, details, title)
}

export const archiveSelected = async (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  try {
    await archiveSession(ctx.api, session.id)
    await ctx.store.refresh()
    ctx.store.notify({ variant: "success", title: "Kagan", message: "Archived — still available in the session list" })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

export const retryHelperTask = async (ctx: BoardActions) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  const status = session.kaganStatus
  if (!canRestartHelper(status, session.metadata)) {
    ctx.notifyWarning("Nothing to restart")
    return
  }
  try {
    await retryHelper(ctx.api, session.id, session, status)
    await ctx.store.refresh()
    ctx.store.notify({
      variant: "success",
      title: "Kagan",
      message: status === "backlog" ? "Restarting intake" : "Restarting review",
    })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}
