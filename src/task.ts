import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { z } from "zod"
import { COLUMNS, DEFAULT_IN_PROGRESS_CAP, type ColumnType } from "./types"
import { newSideHunkRanges } from "./git"

export function inProgressCap(options?: Record<string, unknown>): number {
  const value = options?.inProgressLimit
  if (typeof value === "number" && Number.isInteger(value) && value >= 1) return value
  return DEFAULT_IN_PROGRESS_CAP
}

const DEFAULT_HELPER_RETRIES = 1
const DEFAULT_SEND_BACK_STOP_THRESHOLD = 3

export function helperRetries(options?: Record<string, unknown>): number {
  const value = options?.helperRetries
  if (typeof value === "number" && Number.isInteger(value) && value >= 0) return value
  return DEFAULT_HELPER_RETRIES
}

export function sendBackStopThreshold(options?: Record<string, unknown>): number {
  const value = options?.sendBackStopThreshold
  if (typeof value === "number" && Number.isInteger(value) && value >= 1) return value
  return DEFAULT_SEND_BACK_STOP_THRESHOLD
}

export function squashMerge(options?: Record<string, unknown>): boolean {
  const value = options?.squashMerge
  return typeof value === "boolean" ? value : true
}

export function setupCommand(options?: Record<string, unknown>): string | undefined {
  const value = options?.setupCommand
  return typeof value === "string" && value.trim() ? value.trim() : undefined
}

export function checkCommand(options?: Record<string, unknown>): string | undefined {
  const value = options?.checkCommand
  return typeof value === "string" && value.trim() ? value.trim() : undefined
}

export type ModelRef = { providerID: string; modelID: string }

export type FindingCategory = "misalignment" | "bug" | "uncertainty"
export type FindingResolution = "ignored" | "intended" | "clarified"

export type Finding = {
  id: string
  summary: string
  detail?: string
  location?: string
  severity?: "low" | "medium" | "high"
  confidence?: number
  category?: FindingCategory
  resolution?: FindingResolution
  note?: string
  outOfDiff?: true
}

export type AwaitingInput = { id: string; title: string }

export type IntakeDecision = {
  id: string
  question: string
  assumption: string
  required: boolean
  resolution?: "approved" | "overridden"
  answer?: string
}

export type IntakeMode = {
  recommended: "autonomous" | "assisted" | "manual"
  rationale: string
}

export type Intake = {
  understanding: string
  decisions: IntakeDecision[]
  refinedPrompt?: string
  mode?: IntakeMode
}

export type HelperOutcome = "pending" | "failed" | "ran"

export type HelperRole = "intake" | "validator"

export type HelperError = { role: HelperRole; message: string }

const nonBlank = (s: string) => s.trim().length > 0

const MIN_WORDS = 5
const MIN_REAL_WORDS = 3
const PLACEHOLDERS = new Set([
  "n/a",
  "na",
  "none",
  "lgtm",
  "tbd",
  "ok",
  "okay",
  "looks good",
  "looks good to me",
  "good",
  "fine",
  "done",
])

export function isSubstantive(text: string): boolean {
  const trimmed = text.trim().toLowerCase()
  if (!trimmed) return false
  if (PLACEHOLDERS.has(trimmed)) return false
  const words = trimmed.split(/\s+/)
  if (words.length < MIN_WORDS) return false
  const realWords = words.filter((w) => /[a-z]{3,}/i.test(w))
  return realWords.length >= MIN_REAL_WORDS
}

// Everything beyond id/summary is validator-authored and already checked against the
// kagan_findings tool schema at write time; salvage here only strips fields a hand-edited or
// legacy payload could have corrupted, leaving anything else (severity, confidence, category,
// resolution, note) exactly as stored — matching the pre-Zod hand validator's behavior.
const FindingSchema = z
  .object({ id: z.string(), summary: z.string() })
  .loose()
  .transform((finding) => {
    const sanitized: Record<string, unknown> = { ...finding }
    if (typeof sanitized.detail !== "string") delete sanitized.detail
    if (typeof sanitized.location !== "string") delete sanitized.location
    if (sanitized.outOfDiff !== true) delete sanitized.outOfDiff
    return sanitized as Finding
  })

const FindingsArraySchema = z
  .array(FindingSchema.optional().catch(undefined))
  .transform((findings) => findings.filter((finding): finding is Finding => finding !== undefined))
  .optional()
  .catch(undefined)

const IntakeModeSchema = z
  .object({
    recommended: z.enum(["autonomous", "assisted", "manual"]),
    rationale: z.string().refine(isSubstantive),
  })
  .optional()
  .catch(undefined)

// Decision elements are already normalized by sanitizeIntakeDecisions before they're ever
// written, so — like getIntake before it — this only requires the array shape, not each element.
const IntakeSchema = z
  .object({
    understanding: z.string(),
    decisions: z.array(z.unknown()),
    refinedPrompt: z.string().optional().catch(undefined),
    mode: IntakeModeSchema,
  })
  .transform((intake) => ({ ...intake, decisions: intake.decisions as IntakeDecision[] }) as Intake)
  .optional()
  .catch(undefined)

const CheckResultSchema = z
  .object({
    command: z.string(),
    output: z.string(),
    exitCode: z.union([z.number(), z.null()]),
  })
  .optional()
  .catch(undefined)

const MetadataSchema = z.object({
  approved: z.boolean().optional().catch(undefined),
  boardTask: z.boolean().optional().catch(undefined),
  taskNumber: z.number().int().min(1).optional().catch(undefined),
  report: z.string().refine(nonBlank).optional().catch(undefined),
  description: z.string().refine(nonBlank).optional().catch(undefined),
  baseBranch: z.string().refine(nonBlank).optional().catch(undefined),
  worktree: z.string().refine(nonBlank).optional().catch(undefined),
  activeIteration: z.string().min(1).optional().catch(undefined),
  workerParent: z.string().min(1).optional().catch(undefined),
  startedAt: z.number().optional().catch(undefined),
  generation: z
    .number()
    .optional()
    .catch(undefined)
    .transform((value) => (value !== undefined && value >= 1 ? value : 1)),
  role: z.enum(["intake", "validator", "worker"]).optional().catch(undefined),
  lastGatedStatus: z
    .enum(COLUMNS as unknown as [ColumnType, ...ColumnType[]])
    .optional()
    .catch(undefined),
  intakeSessionID: z.string().min(1).optional().catch(undefined),
  validatorSessionID: z.string().min(1).optional().catch(undefined),
  intakeOutcome: z.enum(["pending", "failed", "ran"]).optional().catch(undefined),
  validatorOutcome: z.enum(["pending", "failed", "ran"]).optional().catch(undefined),
  intakeAttempts: z.number().optional().catch(undefined),
  validatorAttempts: z.number().optional().catch(undefined),
  intakeParent: z.string().min(1).optional().catch(undefined),
  validatorParent: z.string().min(1).optional().catch(undefined),
  awaitingInput: z.object({ id: z.string(), title: z.string() }).optional().catch(undefined),
  helperError: z
    .object({ role: z.enum(["intake", "validator"]), message: z.string().min(1) })
    .optional()
    .catch(undefined),
  model: z.object({ providerID: z.string(), modelID: z.string() }).optional().catch(undefined),
  intake: IntakeSchema,
  findings: FindingsArraySchema,
  priorTriage: FindingsArraySchema,
  check: CheckResultSchema,
  setup: CheckResultSchema,
})

type Metadata = z.infer<typeof MetadataSchema>

// Lossless raw read of the kagan metadata blob, used by the merge path in session-api.ts —
// unknown keys must survive so a patch never drops fields this schema doesn't know about.
export function rawKagan(metadata?: Record<string, unknown>): Record<string, unknown> | undefined {
  const value = metadata?.kagan
  if (typeof value !== "object" || value === null) return undefined
  return value as Record<string, unknown>
}

export function kagan(metadata?: Record<string, unknown>): Metadata {
  return MetadataSchema.parse(rawKagan(metadata) ?? {})
}

export function helper(
  metadata: Record<string, unknown> | undefined,
  role: HelperRole,
): { sessionID?: string; outcome?: HelperOutcome; attempts: number; parent?: string } {
  const view = kagan(metadata)
  const raw =
    role === "intake"
      ? {
          sessionID: view.intakeSessionID,
          outcome: view.intakeOutcome,
          attempts: view.intakeAttempts,
          parent: view.intakeParent,
        }
      : {
          sessionID: view.validatorSessionID,
          outcome: view.validatorOutcome,
          attempts: view.validatorAttempts,
          parent: view.validatorParent,
        }
  return { ...raw, attempts: raw.attempts !== undefined && raw.attempts >= 0 ? raw.attempts : 0 }
}

export function needsHuman(status: ColumnType, metadata?: Record<string, unknown>): boolean {
  const view = kagan(metadata)
  return (status === "review" && view.approved !== true) || view.awaitingInput !== undefined
}

export function isSupervisedSession(metadata?: Record<string, unknown>): boolean {
  const view = kagan(metadata)
  return (
    view.boardTask === true ||
    view.role !== undefined ||
    helper(metadata, "intake").parent !== undefined ||
    helper(metadata, "validator").parent !== undefined ||
    view.workerParent !== undefined
  )
}

const MAX_INTAKE_DECISIONS = 6

export function sanitizeIntakeDecisions(decisions: readonly unknown[]): IntakeDecision[] {
  const seen = new Set<string>()
  const sanitized: IntakeDecision[] = []
  for (const raw of decisions) {
    if (typeof raw !== "object" || raw === null) continue
    const candidate = raw as Record<string, unknown>
    const id = typeof candidate.id === "string" ? candidate.id.trim() : ""
    const question = typeof candidate.question === "string" ? candidate.question.trim() : ""
    const assumption = typeof candidate.assumption === "string" ? candidate.assumption.trim() : ""
    if (!id || !question || !assumption || seen.has(id)) continue
    seen.add(id)
    sanitized.push({ id, question, assumption, required: candidate.required !== false })
    if (sanitized.length >= MAX_INTAKE_DECISIONS) break
  }
  return sanitized
}

export function validMode(raw: unknown): IntakeMode | undefined {
  return IntakeModeSchema.parse(raw)
}

/**
 * Retry covers any helper that started but did not succeed — failed, or spawned and stuck
 * without an outcome — so recovery never depends on a failure event having been detected first.
 */
export function canRetryHelper(metadata: Record<string, unknown> | undefined, role: HelperRole): boolean {
  const { outcome, sessionID } = helper(metadata, role)
  if (outcome === "ran") return false
  const helperError = kagan(metadata).helperError
  return helperError?.role === role || outcome !== undefined || sessionID !== undefined
}

function isResolvedIntakeDecision(decision: IntakeDecision): boolean {
  if (decision.resolution === "approved") return true
  if (decision.resolution === "overridden") return typeof decision.answer === "string" && isSubstantive(decision.answer)
  return false
}

export function pendingRequiredIntakeDecisions(metadata?: Record<string, unknown>): IntakeDecision[] {
  const intake = kagan(metadata).intake
  if (!intake) return []
  return intake.decisions.filter((decision) => decision.required && !isResolvedIntakeDecision(decision))
}

export function intakeReady(metadata?: Record<string, unknown>): boolean {
  const outcome = helper(metadata, "intake").outcome
  if (outcome === "failed") return true
  if (outcome !== "ran") return false
  return pendingRequiredIntakeDecisions(metadata).length === 0
}

export function resolveIntakeDecision(
  decisions: readonly IntakeDecision[],
  decisionID: string,
  resolution: "approved" | "overridden",
  answer?: string,
): IntakeDecision[] {
  return decisions.map((decision) => {
    if (decision.id !== decisionID) return decision
    const updated: IntakeDecision = { ...decision, resolution }
    if (answer !== undefined) updated.answer = answer
    return updated
  })
}

export function getRefinedPrompt(metadata?: Record<string, unknown>): string | undefined {
  const prompt = kagan(metadata).intake?.refinedPrompt
  return typeof prompt === "string" && isSubstantive(prompt) ? prompt : undefined
}

const MAX_UNVERIFIED_CONFIDENCE = 2

function locationParts(location: string): { file: string; line?: number } {
  const separator = location.lastIndexOf(":")
  if (separator === -1) return { file: location }
  const linePart = location.slice(separator + 1)
  if (!/^\d+$/.test(linePart)) return { file: location }
  return { file: location.slice(0, separator), line: Number(linePart) }
}

function citationInDiff(location: string, diffs: readonly SnapshotFileDiff[]): boolean {
  const { file, line } = locationParts(location)
  const diff = diffs.find((d) => d.file === file)
  if (!diff) return false
  if (line === undefined) return true
  return newSideHunkRanges(diff.patch ?? "").some((range) => line >= range.start && line < range.end)
}

// An unverifiable citation caps confidence and flags the finding instead of dropping it — the
// substance may still be real even when the cited line is hallucinated.
export function verifyFindingCitations(findings: readonly Finding[], diffs: readonly SnapshotFileDiff[]): Finding[] {
  return findings.map((finding) => {
    if (!finding.location || citationInDiff(finding.location, diffs)) return finding
    return {
      ...finding,
      confidence: Math.min(finding.confidence ?? MAX_UNVERIFIED_CONFIDENCE, MAX_UNVERIFIED_CONFIDENCE),
      outOfDiff: true,
    }
  })
}

export function isResolvedFinding(finding: Finding): boolean {
  if (finding.resolution === "ignored" || finding.resolution === "clarified") {
    return typeof finding.note === "string" && isSubstantive(finding.note)
  }
  if (finding.resolution === "intended") {
    if (finding.severity === "high") return typeof finding.note === "string" && isSubstantive(finding.note)
    return true
  }
  return false
}

export function pendingFindingCount(metadata?: Record<string, unknown>): number {
  const findings = kagan(metadata).findings
  if (!findings) return 0
  return findings.filter((finding) => !isResolvedFinding(finding)).length
}

// Unscored means the validator ignored the scoring instruction — rank it below
// every scored finding rather than treating silence as certainty.
const CONFIDENCE_UNSCORED = -1

export function sortFindingsByConfidence(findings: readonly Finding[]): Finding[] {
  return [...findings].sort(
    (left, right) => (right.confidence ?? CONFIDENCE_UNSCORED) - (left.confidence ?? CONFIDENCE_UNSCORED),
  )
}

export function resolveFinding(
  findings: readonly Finding[],
  findingID: string,
  resolution: FindingResolution,
  note?: string,
): Finding[] {
  return findings.map((finding) => {
    if (finding.id !== findingID) return finding
    const updated: Finding = { ...finding, resolution }
    if (note !== undefined) updated.note = note
    return updated
  })
}

export function countInProgressForMove(
  sessions: readonly { id: string; parentID?: string | null; status: ColumnType }[],
  sessionID: string,
  source: ColumnType,
): number {
  let count = 0
  for (const session of sessions) {
    if (session.parentID) continue
    if (session.status !== "in_progress") continue
    if (session.id === sessionID && source !== "in_progress") continue
    count++
  }
  return count
}

export type ColumnMoveContext = {
  inProgressCount: number
  source?: ColumnType
  cap?: number
}

export function columnMoveDenyReason(
  to: ColumnType,
  metadata?: Record<string, unknown>,
  ctx?: ColumnMoveContext,
): string | undefined {
  const view = kagan(metadata)
  const cap = ctx?.cap ?? DEFAULT_IN_PROGRESS_CAP
  const inProgressCount = ctx?.inProgressCount ?? 0
  if (to === "in_progress" && ctx?.source !== "in_progress" && inProgressCount >= cap) {
    return `In Progress WIP limit of ${cap} reached`
  }
  if (to === "in_progress" && ctx?.source === "backlog" && view.worktree === undefined) {
    return "Task has no isolated worktree — create tasks from the board so agents run sandboxed"
  }
  if (to === "in_progress" && ctx?.source === "backlog" && !intakeReady(metadata)) {
    const pending = pendingRequiredIntakeDecisions(metadata).length
    if (pending > 0) return `${pending} intake decision(s) need your answer before starting`
    return "Intake is still being prepared"
  }
  if (to === "done" && view.approved !== true) {
    return "Task must be approved before moving to Done"
  }
  if (to === "backlog" && ctx?.source === "in_progress" && view.startedAt !== undefined) {
    return "Agent already started — let it finish, send it back from Review, or delete the task"
  }
  if (to !== "done" && ctx?.source === "done") {
    return "Approved tasks stay in Done — create a follow-up task instead"
  }
  return undefined
}

export function approveDenyReason(metadata?: Record<string, unknown>): string | undefined {
  const view = kagan(metadata)
  if (view.boardTask !== true) return "Only board tasks can be approved"
  const outcome = helper(metadata, "validator").outcome
  if (outcome !== "ran" && outcome !== "failed") return "Review hasn't finished — no validator outcome yet"
  const pending = pendingFindingCount(metadata)
  if (pending > 0) return `${pending} finding(s) need triage`
  return undefined
}

export function nextGenerationPatch(metadata?: Record<string, unknown>): Record<string, unknown> {
  const view = kagan(metadata)
  const carried = (view.findings ?? []).filter(
    (finding) => finding.resolution === "intended" || finding.resolution === "ignored",
  )
  const priorTriage = [...(view.priorTriage ?? []), ...carried]
  return {
    generation: view.generation + 1,
    priorTriage: priorTriage.length > 0 ? priorTriage : undefined,
    findings: undefined,
    check: undefined,
    validatorSessionID: undefined,
    validatorOutcome: undefined,
    validatorAttempts: undefined,
    helperError: undefined,
    approved: undefined,
  }
}
