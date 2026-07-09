import { z } from "zod"
import { COLUMNS, type ColumnType } from "./types"
import { sanitizeTaskScope } from "./commands"
import { isSubstantive, type Intake, type IntakeDecision, type IntakeMode } from "./intake"
import type { Finding } from "./findings"

const nonBlank = (text: string) => text.trim().length > 0

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
    steps: z
      .array(
        z.object({
          name: z.string(),
          cwd: z.string(),
          command: z.string(),
          status: z.enum(["ran", "skipped"]),
          exitCode: z.union([z.number(), z.null()]),
          output: z.string(),
          reason: z.string().optional().catch(undefined),
        }),
      )
      .optional()
      .catch(undefined),
  })
  .optional()
  .catch(undefined)

const TaskScopeSchema = z
  .object({ values: z.array(z.string()).optional().catch([]), custom: z.string().optional().catch(undefined) })
  .transform((scope) => sanitizeTaskScope(scope))
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
  status: z
    .enum(COLUMNS as unknown as [ColumnType, ...ColumnType[]])
    .optional()
    .catch(undefined),
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
  scope: TaskScopeSchema,
})

type Metadata = z.infer<typeof MetadataSchema>

function rawKagan(metadata?: Record<string, unknown>): Record<string, unknown> | undefined {
  const value = metadata?.kagan
  if (typeof value !== "object" || value === null) return undefined
  return value as Record<string, unknown>
}

export function kagan(metadata?: Record<string, unknown>): Metadata {
  return MetadataSchema.parse(rawKagan(metadata) ?? {})
}

export function validMode(raw: unknown): IntakeMode | undefined {
  return IntakeModeSchema.parse(raw)
}
