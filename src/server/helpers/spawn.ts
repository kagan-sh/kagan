import type { PluginInput } from "@opencode-ai/plugin"
import { commandMatchesChangedFile, commandPlan } from "../../domain/task/commands"
import { helper } from "../../domain/task/policy"
import { kagan } from "../../domain/task/metadata"
import { runCommandPlan, truncateCheckResultForMetadata, type CheckResult } from "../../checks/runner"
import { worktreeDiffs } from "../../git/diffs"
import { shellGitRunner } from "../../git/runner"
import { spawnIntake } from "../intake"
import { claimHelperSpawn, patchKagan } from "../session/patch"
import { spawnValidator } from "../validator/spawn"
import { errorMessage, getSessionData, resolveTaskRefs } from "../data"
import { handleHelperFailure } from "./events"

const entryClaims = ((globalThis as Record<string, unknown>).__kaganHelperEntryClaims ??=
  new Set<string>()) as Set<string>

async function spawnHelper(
  input: PluginInput,
  role: "intake" | "validator",
  sessionID: string,
  options: Record<string, unknown> | undefined,
  eligible: (session: Awaited<ReturnType<typeof getSessionData>>) => boolean,
  spawn: (session: Awaited<ReturnType<typeof getSessionData>>) => Promise<string | undefined>,
): Promise<void> {
  const key = `${sessionID}:${role}`
  if (entryClaims.has(key)) return
  entryClaims.add(key)
  let failure: { attempts: number; message: string } | undefined
  try {
    const session = await getSessionData(input, sessionID)
    const metadata = session?.metadata
    const view = kagan(metadata)
    const before = helper(metadata, role)
    if (view.boardTask !== true || !eligible(session) || before.outcome !== undefined || before.sessionID !== undefined)
      return
    if (!(await claimHelperSpawn(input.client, sessionID, role))) return
    const attempts = before.attempts + 1
    let childID: string | undefined
    try {
      childID = await spawn(session)
    } catch (error) {
      failure = { attempts, message: errorMessage(error) }
      return
    }
    if (!childID) {
      await patchKagan(input.client, sessionID, { [`${role}Outcome`]: "failed" })
      return
    }
    await patchKagan(input.client, sessionID, { [`${role}Attempts`]: attempts, helperError: undefined })
  } finally {
    entryClaims.delete(key)
    if (failure) await handleHelperFailure(input, role, sessionID, failure.attempts, failure.message, options)
  }
}

export async function onEnterBacklog(
  input: PluginInput,
  sessionID: string,
  options?: Record<string, unknown>,
): Promise<void> {
  await spawnHelper(
    input,
    "intake",
    sessionID,
    options,
    () => true,
    async (session) => {
      const view = kagan(session?.metadata)
      return spawnIntake(
        input,
        sessionID,
        {
          title: session?.title ?? "",
          description: view.description,
          references: await resolveTaskRefs(input, view.description),
          scope: view.scope,
        },
        options,
      )
    },
  )
}

async function collectCheck(
  input: PluginInput,
  sessionID: string,
  worktree: string,
  baseBranch: string | undefined,
  options: Record<string, unknown> | undefined,
): Promise<{ diffs: Awaited<ReturnType<typeof worktreeDiffs>>; check: CheckResult | undefined }> {
  const diffs = await worktreeDiffs(shellGitRunner(input.$), worktree, baseBranch ?? "HEAD")
  const commands = commandPlan(options, "check")
  if (commands.length === 0) return { diffs, check: undefined }
  const changedFiles = diffs.map((diff) => diff.file).filter((file): file is string => typeof file === "string")
  const result = await runCommandPlan(commands, worktree, (command) => commandMatchesChangedFile(command, changedFiles))
  if (!result) return { diffs, check: undefined }
  const check = truncateCheckResultForMetadata(result)
  try {
    await patchKagan(input.client, sessionID, { check })
  } catch {
    // check evidence is best-effort; a failed write must not block validator spawn
  }
  return { diffs, check }
}

export async function onEnterReview(
  input: PluginInput,
  sessionID: string,
  options?: Record<string, unknown>,
): Promise<void> {
  await spawnHelper(
    input,
    "validator",
    sessionID,
    options,
    (session) => {
      const view = kagan(session?.metadata)
      return !view.role && !session?.parentID && Boolean(view.worktree)
    },
    async (session) => {
      const view = kagan(session?.metadata)
      if (!view.worktree) return undefined
      const { diffs, check } = await collectCheck(input, sessionID, view.worktree, view.baseBranch, options)
      return spawnValidator(
        input,
        sessionID,
        diffs,
        {
          title: session?.title ?? "",
          description: view.description,
          intake: view.intake,
          priorTriage: view.priorTriage,
          generation: view.generation,
          check,
          builderModel: view.model,
        },
        options,
      )
    },
  )
}
