import type { Plugin, PluginInput, PluginModule } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin/tool"
import {
  columnMoveDenyReason,
  countInProgressForMove,
  helper,
  helperRetries,
  inProgressCap,
  isSupervisedSession,
  kagan,
  sanitizeIntakeDecisions,
  validMode,
  verifyFindingCitations,
  type Finding,
  type HelperRole,
} from "./task"
import { isGitPushCommand, worktreeDiffs, shellGitRunner } from "./git"
import { composeStartPrompt, formatTaskRef, parseTaskRefs } from "./handoff"
import { spawnIntake } from "./intake"
import { getStatus, lastAssistantText, patchKagan } from "./session-api"
import type { ColumnType } from "./types"
import { spawnValidator } from "./validator"
import { runCheckCommand, type CheckResult } from "./check"

type SessionData = {
  title?: string
  metadata?: Record<string, unknown>
  parentID?: string | null
}

type EventInfo = SessionData & { id: string }

async function getSessionData(input: PluginInput, sessionID: string): Promise<SessionData | undefined> {
  const result = await input.client.session.get({ path: { id: sessionID }, throwOnError: true })
  return result.data as SessionData | undefined
}

type ListedSession = {
  id: string
  title?: string
  parentID?: string | null
  metadata?: Record<string, unknown>
}

async function listInProgressCount(input: PluginInput, sessionID: string, source: ColumnType): Promise<number> {
  const listResult = await input.client.session.list({
    query: { scope: "project" },
    throwOnError: true,
  } as Parameters<typeof input.client.session.list>[0])
  const sessions = (listResult.data ?? []) as ListedSession[]
  return countInProgressForMove(
    sessions.map((session) => ({
      id: session.id,
      parentID: session.parentID,
      status: getStatus(session.metadata),
    })),
    sessionID,
    source,
  )
}

async function sessionMessages(input: PluginInput, sessionID: string): Promise<unknown> {
  return input.client.session
    .messages({ path: { id: sessionID }, throwOnError: true })
    .then((result) => result.data)
    .catch(() => undefined)
}

async function resolveTaskRefs(input: PluginInput, description: string | undefined): Promise<string | undefined> {
  if (!description) return undefined
  const refs = parseTaskRefs(description)
  if (refs.length === 0) return undefined
  try {
    const listResult = await input.client.session.list({
      query: { scope: "project" },
      throwOnError: true,
    } as Parameters<typeof input.client.session.list>[0])
    const sessions = (listResult.data ?? []) as ListedSession[]
    const byNumber = new Map<number, ListedSession>()
    for (const session of sessions) {
      if (session.parentID) continue
      const number = kagan(session.metadata).taskNumber
      if (number !== undefined) byNumber.set(number, session)
    }

    const blocks = refs.map((number) => {
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
    return blocks.length > 0 ? blocks.join("\n\n") : undefined
  } catch {
    return undefined
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function extractErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    const data = (error as { data?: unknown }).data
    if (typeof data === "object" && data !== null) {
      const message = (data as { message?: unknown }).message
      if (typeof message === "string" && message.trim()) return message
    }
    const name = (error as { name?: unknown }).name
    if (typeof name === "string" && name.trim()) return name
  }
  return "unknown error"
}

/**
 * Single funnel for every intake/validator failure (spawn-time throw, error
 * event, idle-without-tool-call). `attemptsUsed` is the attempt count of the
 * spawn that just failed; retrying only bumps session state, never this
 * counter directly — the next spawn (triggered by the state-based spawn
 * conditions below) recomputes it from metadata.
 */
async function handleHelperFailure(
  input: PluginInput,
  role: HelperRole,
  parentSessionID: string,
  attemptsUsed: number,
  message: string,
  options?: Record<string, unknown>,
): Promise<void> {
  const retries = helperRetries(options)
  if (attemptsUsed <= retries) {
    await patchKagan(input.client, parentSessionID, {
      [`${role}SessionID`]: undefined,
      [`${role}Outcome`]: undefined,
      [`${role}Attempts`]: attemptsUsed,
    })
    console.error(
      `[kagan] ${role} helper failed for ${parentSessionID} (attempt ${attemptsUsed}/${retries + 1}), retrying: ${message}`,
    )
    return
  }
  await patchKagan(input.client, parentSessionID, {
    [`${role}Outcome`]: "failed",
    helperError: { role, message },
  })
}

/**
 * Single detector-agnostic funnel for a helper (intake/validator) failure signal — a `session.error`
 * on the helper, or `session.idle` on the helper with its outcome still unset. Both detectors land
 * here once the parent's recorded helper sessionID is confirmed to still be this session and its
 * outcome is still "pending" (a stale event from a helper already superseded by a retry is ignored).
 */
async function handleHelperEvent(
  input: PluginInput,
  role: HelperRole,
  sessionID: string,
  metadata: Record<string, unknown> | undefined,
  message: string,
  options?: Record<string, unknown>,
): Promise<void> {
  const parentID = helper(metadata, role).parent
  if (!parentID) return
  const parentMetadata = (await getSessionData(input, parentID))?.metadata
  const parentHelper = helper(parentMetadata, role)
  if (parentHelper.sessionID !== sessionID) return
  if (parentHelper.outcome !== "pending") return
  await handleHelperFailure(input, role, parentID, parentHelper.attempts, message, options)
}

/**
 * Resolves the root board task that "owns" `sessionID`: the task itself when
 * the session is a root board task, or the parent for an intake/validator/worker
 * child. Returns undefined for anything else so callers can no-op rather than
 * mis-stamp a stray session.
 */
function owningRootTaskID(
  metadata: Record<string, unknown> | undefined,
  sessionID: string,
  parentID?: string | null,
): string | undefined {
  const view = kagan(metadata)
  if (view.role === "intake" || view.role === "validator") return helper(metadata, view.role).parent
  if (view.role === "worker") return view.workerParent
  if (view.boardTask === true && !parentID) return sessionID
  return undefined
}

async function resolveOwningBoardTask(input: PluginInput, sessionID: string): Promise<string | undefined> {
  const session = await getSessionData(input, sessionID)
  return owningRootTaskID(session?.metadata, sessionID, session?.parentID)
}

const helperEntryClaims = new Set<string>()

async function onEnterBacklog(input: PluginInput, sessionID: string, options?: Record<string, unknown>): Promise<void> {
  const key = `${sessionID}:intake`
  if (helperEntryClaims.has(key)) return
  helperEntryClaims.add(key)
  let failure: { attempts: number; message: string } | undefined
  try {
    const session = await getSessionData(input, sessionID)
    const metadata = session?.metadata
    if (kagan(metadata).boardTask !== true) return
    const before = helper(metadata, "intake")
    if (before.outcome !== undefined) return
    if (before.sessionID !== undefined) return

    const attempts = before.attempts + 1
    const description = kagan(metadata).description
    const references = await resolveTaskRefs(input, description)
    let childID: string | undefined
    try {
      childID = await spawnIntake(input, sessionID, { title: session?.title ?? "", description, references }, options)
    } catch (error) {
      failure = { attempts, message: errorMessage(error) }
      return
    }
    if (!childID) {
      await patchKagan(input.client, sessionID, { intakeOutcome: "failed" })
      return
    }
    await patchKagan(input.client, sessionID, { intakeAttempts: attempts, helperError: undefined })
  } finally {
    helperEntryClaims.delete(key)
    // The failure funnel's clear-state patch is delivered back to this plugin as a session.updated
    // before the patch call resolves, and that event is what respawns the helper — so it must run
    // after the claim is released or the retry is swallowed.
    if (failure) await handleHelperFailure(input, "intake", sessionID, failure.attempts, failure.message, options)
  }
}

async function onEnterReview(input: PluginInput, sessionID: string, options?: Record<string, unknown>): Promise<void> {
  const key = `${sessionID}:validator`
  if (helperEntryClaims.has(key)) return
  helperEntryClaims.add(key)
  let failure: { attempts: number; message: string } | undefined
  try {
    const session = await getSessionData(input, sessionID)
    const metadata = session?.metadata
    const view = kagan(metadata)
    if (view.role || session?.parentID) return
    if (view.boardTask !== true) return
    const before = helper(metadata, "validator")
    if (before.outcome !== undefined) return
    if (before.sessionID !== undefined) return
    const worktree = view.worktree
    if (!worktree) return

    const attempts = before.attempts + 1
    const diffs = await worktreeDiffs(shellGitRunner(input.$), worktree, view.baseBranch ?? "HEAD")
    const checkCommand = typeof options?.checkCommand === "string" ? options.checkCommand.trim() : undefined
    let check: CheckResult | undefined
    if (checkCommand) {
      check = await runCheckCommand(checkCommand, worktree)
      await patchKagan(input.client, sessionID, { check })
    }
    let childID: string | undefined
    try {
      childID = await spawnValidator(
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
    } catch (error) {
      failure = { attempts, message: errorMessage(error) }
      return
    }
    if (!childID) {
      await patchKagan(input.client, sessionID, { validatorOutcome: "failed" })
      return
    }
    await patchKagan(input.client, sessionID, { validatorAttempts: attempts, helperError: undefined })
  } finally {
    helperEntryClaims.delete(key)
    // Same release-before-retry ordering as intake above.
    if (failure) await handleHelperFailure(input, "validator", sessionID, failure.attempts, failure.message, options)
  }
}

async function handleSessionCreated(input: PluginInput, event: { properties: { info: unknown } }): Promise<void> {
  const info = event.properties.info as EventInfo
  const view = kagan(info.metadata)
  if (view.role || info.parentID) return
  if (view.boardTask === true && view.lastGatedStatus === undefined) {
    await patchKagan(input.client, info.id, { lastGatedStatus: getStatus(info.metadata) })
  }
}

async function handleSessionUpdated(
  input: PluginInput,
  event: { properties: { info: unknown } },
  options: Record<string, unknown> | undefined,
): Promise<void> {
  const info = event.properties.info as EventInfo
  const infoView = kagan(info.metadata)
  if (infoView.role || info.parentID) return
  const sessionID = info.id
  const newCol = getStatus(info.metadata)
  const prev = infoView.lastGatedStatus
  const isBoard = infoView.boardTask === true

  if (prev === undefined) {
    if (isBoard) await patchKagan(input.client, sessionID, { lastGatedStatus: newCol })
  } else if (newCol !== prev) {
    const inProgressCount = await listInProgressCount(input, sessionID, prev)
    const reason = columnMoveDenyReason(newCol, info.metadata, {
      inProgressCount,
      source: prev,
      cap: inProgressCap(options),
    })
    if (reason) {
      await patchKagan(input.client, sessionID, { status: prev })
      return
    }

    if (isBoard) await patchKagan(input.client, sessionID, { lastGatedStatus: newCol })
    if (newCol === "in_progress" && infoView.startedAt === undefined && isBoard) {
      await patchKagan(input.client, sessionID, { startedAt: Date.now() })
      try {
        const model = infoView.model
        const references = await resolveTaskRefs(input, infoView.description)
        const startPrompt = composeStartPrompt(info.title ?? "", info.metadata)
        const body: Record<string, unknown> = {
          parts: [{ type: "text", text: references ? `${startPrompt}\n\n${references}` : startPrompt }],
        }
        if (model) body.model = { providerID: model.providerID, modelID: model.modelID }
        await input.client.session.promptAsync({
          path: { id: sessionID },
          body,
          throwOnError: true,
        } as Parameters<typeof input.client.session.promptAsync>[0])
      } catch (error) {
        console.error(`[kagan] auto-start prompt failed for ${sessionID}, reverting to backlog: ${errorMessage(error)}`)
        await patchKagan(input.client, sessionID, {
          startedAt: undefined,
          status: "backlog",
          lastGatedStatus: "backlog",
        })
      }
    }
  }

  if (newCol === "backlog") {
    await onEnterBacklog(input, sessionID, options)
  }
  if (newCol === "review") {
    await onEnterReview(input, sessionID, options)
  }
}

async function handleSessionError(
  input: PluginInput,
  event: { properties: { sessionID?: string; error?: unknown } },
  options: Record<string, unknown> | undefined,
): Promise<void> {
  const { sessionID, error } = event.properties
  if (!sessionID) return
  const session = await getSessionData(input, sessionID)
  const role = kagan(session?.metadata).role
  if (role !== "intake" && role !== "validator") return
  await handleHelperEvent(input, role, sessionID, session?.metadata, extractErrorMessage(error), options)
}

async function handlePermissionUpdated(
  input: PluginInput,
  event: { properties: { id: string; sessionID: string; title: string } },
): Promise<void> {
  const permission = event.properties
  const rootID = await resolveOwningBoardTask(input, permission.sessionID)
  if (!rootID) return
  await patchKagan(input.client, rootID, { awaitingInput: { id: permission.id, title: permission.title } })
}

async function handlePermissionReplied(
  input: PluginInput,
  event: { properties: { sessionID: string } },
): Promise<void> {
  const rootID = await resolveOwningBoardTask(input, event.properties.sessionID)
  if (!rootID) return
  await patchKagan(input.client, rootID, { awaitingInput: undefined })
}

async function handleSessionIdle(
  input: PluginInput,
  event: { properties: { sessionID: string } },
  options: Record<string, unknown> | undefined,
): Promise<void> {
  const sessionID = event.properties.sessionID
  const session = await getSessionData(input, sessionID)
  const role = kagan(session?.metadata).role
  if (role === "intake" || role === "validator") {
    const message =
      role === "intake"
        ? "intake finished without recording an assessment"
        : "review finished without recording findings"
    await handleHelperEvent(input, role, sessionID, session?.metadata, message, options)
    return
  }

  const rootID = owningRootTaskID(session?.metadata, sessionID, session?.parentID)
  if (!rootID) return

  const root = rootID === sessionID ? session : await getSessionData(input, rootID)
  const rootView = kagan(root?.metadata)
  if (getStatus(root?.metadata) !== "in_progress") return
  if (rootView.startedAt === undefined) return
  const active = rootView.activeIteration
  const isActive = active === sessionID || (active === undefined && sessionID === rootID)
  if (!isActive) return

  const report = lastAssistantText(await sessionMessages(input, sessionID))
  await patchKagan(input.client, rootID, {
    status: "review",
    awaitingInput: undefined,
    ...(report ? { report } : {}),
  })
}

const PUSH_DENIED_MESSAGE =
  "kagan task sandboxes cannot push to a remote — merging happens through the board's merge dialog after review."

async function guardGitPush(
  input: PluginInput,
  hookInput: { tool: string; sessionID: string; callID: string },
  output: { args: any },
): Promise<void> {
  if (hookInput.tool !== "bash") return
  const command = output.args?.command
  if (typeof command !== "string" || !isGitPushCommand(command)) return
  const session = await getSessionData(input, hookInput.sessionID)
  if (!isSupervisedSession(session?.metadata)) return
  throw new Error(PUSH_DENIED_MESSAGE)
}

const server: Plugin = async (input, options) => {
  return {
    "tool.execute.before": (hookInput, output) => guardGitPush(input, hookInput, output),

    event: async ({ event }) => {
      switch (event.type) {
        case "session.created":
          return handleSessionCreated(input, event)
        case "session.updated":
          return handleSessionUpdated(input, event, options)
        case "session.error":
          return handleSessionError(input, event, options)
        case "permission.updated":
          return handlePermissionUpdated(input, event)
        case "permission.replied":
          return handlePermissionReplied(input, event)
        case "session.idle":
          return handleSessionIdle(input, event, options)
        default:
          return
      }
    },

    tool: {
      kagan_intake: tool({
        description: "Record the read-only intake assessment for the parent kagan task",
        args: {
          understanding: tool.schema
            .string()
            .describe("A short summary of what this task means and how you would approach it"),
          decisions: tool.schema
            .array(
              tool.schema.object({
                id: tool.schema.string().describe("Stable decision id"),
                question: tool.schema.string().describe("The question the human should confirm"),
                assumption: tool.schema.string().describe("The default assumption being made"),
                required: tool.schema
                  .boolean()
                  .optional()
                  .describe("False only when work can proceed safely either way"),
              }),
            )
            .describe("Assumptions that test both your and the human's comprehension before work starts"),
          refinedPrompt: tool.schema
            .string()
            .describe(
              "The final, self-contained instruction prompt for the implementing agent, incorporating the codebase analysis",
            ),
          mode: tool.schema
            .object({
              recommended: tool.schema
                .enum(["autonomous", "assisted", "manual"])
                .describe("Recommended supervision mode for this task"),
              rationale: tool.schema
                .string()
                .describe("One-line rationale for the recommendation, based on the five factors"),
            })
            .optional()
            .describe("Advisory mode recommendation for the human — not a gate"),
        },
        async execute(args, ctx) {
          const session = await getSessionData(input, ctx.sessionID)
          const parentID = helper(session?.metadata, "intake").parent
          if (!parentID) {
            throw new Error("kagan_intake is only available in intake sessions")
          }
          const decisions = sanitizeIntakeDecisions(args.decisions)
          const mode = validMode(args.mode)
          const intake: Record<string, unknown> = {
            understanding: args.understanding,
            decisions,
            refinedPrompt: args.refinedPrompt,
          }
          if (mode) intake.mode = mode
          await patchKagan(input.client, parentID, {
            intake,
            intakeOutcome: "ran",
            helperError: undefined,
          })
          return { output: `Recorded intake with ${decisions.length} decision(s).` }
        },
      }),

      kagan_findings: tool({
        description: "Record validator findings for the parent kagan task",
        args: {
          findings: tool.schema
            .array(
              tool.schema.object({
                id: tool.schema.string().describe("Stable finding id"),
                summary: tool.schema.string().describe("What the validator found"),
                detail: tool.schema
                  .string()
                  .optional()
                  .describe("Full problem statement — the complete reasoning, quoting the offending lines if useful"),
                location: tool.schema.string().optional().describe("Pointer to the offending code, as `file:line`"),
                severity: tool.schema.enum(["low", "medium", "high"]).optional(),
                confidence: tool.schema
                  .number()
                  .min(0)
                  .max(10)
                  .optional()
                  .describe("Confidence this finding is real, 0 (likely false positive) to 10 (certain)"),
                category: tool.schema
                  .enum(["misalignment", "bug", "uncertainty"])
                  .describe(
                    "misalignment = diverges from the agreed task; bug = defect; uncertainty = cannot verify correctness",
                  ),
              }),
            )
            .describe("Findings from the diff review"),
        },
        async execute(args, ctx) {
          const session = await getSessionData(input, ctx.sessionID)
          const parentID = helper(session?.metadata, "validator").parent
          if (!parentID) {
            throw new Error("kagan_findings is only available in validator sessions")
          }
          const parent = await getSessionData(input, parentID)
          const worktree = kagan(parent?.metadata).worktree
          let findings: Finding[] = args.findings
          if (worktree) {
            try {
              const diffs = await worktreeDiffs(
                shellGitRunner(input.$),
                worktree,
                kagan(parent?.metadata).baseBranch ?? "HEAD",
              )
              findings = verifyFindingCitations(findings, diffs)
            } catch {
              // diff unavailable — persist findings unverified rather than blocking the validator
            }
          }
          // A send-back can reset the review while the diff recomputation above is in flight;
          // re-check that this validator is still the recorded one so a stale write cannot
          // attach old-generation findings to the fresh generation.
          const parentNow = await getSessionData(input, parentID)
          if (helper(parentNow?.metadata, "validator").sessionID !== ctx.sessionID) {
            return { output: "Findings not recorded — this review was superseded by a newer iteration." }
          }
          await patchKagan(input.client, parentID, {
            findings,
            validatorOutcome: "ran",
            helperError: undefined,
          })
          return { output: `Recorded ${args.findings.length} finding(s).` }
        },
      }),
    },
  }
}

const plugin: PluginModule = {
  id: "kagan",
  server,
}

export default plugin
