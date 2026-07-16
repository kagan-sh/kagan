/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { approveSession } from "../../session/tasks"
import { approveDenyReason } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import { mergeTask } from "../../tasks"
import { baseBranchFreshness, bunGitRunner, currentBranch, listLocalBranches } from "../../../git/runner"
import { openFindingsReviewDialog } from "../../dialogs/findings-review"
import type { BoardSession } from "../../types"
import type { BoardActions, BoardStore } from "./context"

type MergeChoice = "current" | "another" | "none"

type MergeDialogHandlers = {
  runMerge: (session: BoardSession, targetBranch: string) => Promise<void>
  promptAnotherBranch: (session: BoardSession) => Promise<void>
  finalizeApprove: (session: BoardSession, mergeMessage?: string) => Promise<void>
}

function mergeChoiceOptions(current: string | undefined, squash: boolean): { title: string; value: MergeChoice }[] {
  const verb = squash ? "Squash-merge" : "Merge"
  const options: { title: string; value: MergeChoice }[] = []
  if (current) options.push({ title: `${verb} into ${current}`, value: "current" })
  options.push({ title: `${verb} into another branch…`, value: "another" })
  options.push({ title: "No action", value: "none" })
  return options
}

function openMergeDialog(
  api: TuiPluginApi,
  store: BoardStore,
  session: BoardSession,
  freshness: { ahead: number },
  handlers: MergeDialogHandlers,
): void {
  const current = api.state.vcs?.branch
  const options = mergeChoiceOptions(current, store.squashMerge)
  const baseBranch = kagan(session.metadata).baseBranch
  const title =
    freshness.ahead > 0 && baseBranch
      ? `Approve — merge the task branch? ${baseBranch} is ${freshness.ahead} commit(s) ahead — the reviewed diff may be stale`
      : "Approve — merge the task branch?"
  api.ui.dialog.replace(() => (
    <api.ui.DialogSelect<MergeChoice>
      title={title}
      options={options}
      onSelect={(option) => {
        if (option.value === "current" && current) {
          api.ui.dialog.clear()
          return handlers.runMerge(session, current)
        }
        if (option.value === "another") {
          return handlers.promptAnotherBranch(session)
        }
        api.ui.dialog.clear()
        return handlers.finalizeApprove(session)
      }}
    />
  ))
}

const finalizeApprove = async (ctx: BoardActions, session: BoardSession, mergeMessage?: string) => {
  try {
    await approveSession(ctx.api, session.id, session)
    await ctx.store.refresh()
    await ctx.store.moveTo("done")
    ctx.store.notify({
      variant: "success",
      title: "Kagan",
      message: mergeMessage ? `Task approved — ${mergeMessage}` : "Task approved",
    })
  } catch (error) {
    ctx.notifyErrorFrom(error)
  }
}

const runMerge = async (ctx: BoardActions, session: BoardSession, targetBranch: string) => {
  const result = await mergeTask(ctx.api, session, targetBranch, ctx.store.squashMerge)
  if (!result.ok) {
    ctx.notifyError(result.message)
    return
  }
  await finalizeApprove(ctx, session, result.message)
}

const promptAnotherBranch = async (ctx: BoardActions, session: BoardSession) => {
  const runner = bunGitRunner()
  const worktree = kagan(session.metadata).worktree
  const taskBranch = worktree ? await currentBranch(runner, worktree) : undefined
  const branches = (await listLocalBranches(runner, ctx.api.state.path.worktree)).filter(
    (branch) => branch !== taskBranch,
  )
  if (branches.length === 0) {
    ctx.notifyWarning("No other local branches to merge into")
    return
  }
  ctx.api.ui.dialog.replace(() => (
    <ctx.api.ui.DialogSelect<string>
      title="Merge into which branch?"
      options={branches.map((branch) => ({ title: branch, value: branch }))}
      onSelect={(option) => {
        ctx.api.ui.dialog.clear()
        void runMerge(ctx, session, option.value)
      }}
    />
  ))
}

const promptMerge = async (ctx: BoardActions, session: BoardSession) => {
  const runner = bunGitRunner()
  const view = kagan(session.metadata)
  const freshness = await baseBranchFreshness(runner, view.worktree, view.baseBranch)
  openMergeDialog(ctx.api, ctx.store, session, freshness, {
    runMerge: (target, branch) => runMerge(ctx, target, branch),
    promptAnotherBranch: (target) => promptAnotherBranch(ctx, target),
    finalizeApprove: (target, message) => finalizeApprove(ctx, target, message),
  })
}

const afterTriage = async (ctx: BoardActions, session: BoardSession) => {
  const reason = approveDenyReason(session.metadata)
  if (reason) {
    ctx.notifyWarning(reason)
    return
  }
  await promptMerge(ctx, session)
}

export const approve = (ctx: BoardActions, onSendBack: () => void) => {
  const session = ctx.store.selectedSession()
  if (!session) return
  if (session.kaganStatus !== "review") {
    ctx.notifyWarning("Approve only applies to tasks in review")
    return
  }
  openFindingsReviewDialog(ctx.api, ctx.store, session, ctx.store.checkCommand, {
    onApprove: (approvedSession) => afterTriage(ctx, approvedSession),
    onSendBack,
  })
}
