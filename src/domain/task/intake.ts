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
  if (!trimmed || PLACEHOLDERS.has(trimmed)) return false
  const words = trimmed.split(/\s+/)
  if (words.length < MIN_WORDS) return false
  return words.filter((word) => /[a-z]{3,}/i.test(word)).length >= MIN_REAL_WORDS
}

const MAX_INTAKE_DECISIONS = 6

function decision(raw: unknown): IntakeDecision | undefined {
  if (typeof raw !== "object" || raw === null) return undefined
  const candidate = raw as Record<string, unknown>
  const id = typeof candidate.id === "string" ? candidate.id.trim() : ""
  const question = typeof candidate.question === "string" ? candidate.question.trim() : ""
  const assumption = typeof candidate.assumption === "string" ? candidate.assumption.trim() : ""
  if (!id || !question || !assumption) return undefined
  return { id, question, assumption, required: candidate.required !== false }
}

export function sanitizeIntakeDecisions(decisions: readonly unknown[]): IntakeDecision[] {
  const seen = new Set<string>()
  const sanitized: IntakeDecision[] = []
  for (const raw of decisions) {
    const candidate = decision(raw)
    if (!candidate || seen.has(candidate.id)) continue
    seen.add(candidate.id)
    sanitized.push(candidate)
    if (sanitized.length >= MAX_INTAKE_DECISIONS) break
  }
  return sanitized
}

function isResolved(decision: IntakeDecision): boolean {
  if (decision.resolution === "approved") return true
  return decision.resolution === "overridden" && decision.answer !== undefined && isSubstantive(decision.answer)
}

export function pendingRequiredIntakeDecisions(intake: Intake | undefined): IntakeDecision[] {
  return intake?.decisions.filter((decision) => decision.required && !isResolved(decision)) ?? []
}

export function intakeReady(outcome: "pending" | "failed" | "ran" | undefined, intake: Intake | undefined): boolean {
  if (outcome === "failed") return true
  return outcome === "ran" && pendingRequiredIntakeDecisions(intake).length === 0
}

export function resolveIntakeDecision(
  decisions: readonly IntakeDecision[],
  decisionID: string,
  resolution: "approved" | "overridden",
  answer?: string,
): IntakeDecision[] {
  return decisions.map((decision) => {
    if (decision.id !== decisionID) return decision
    return { ...decision, resolution, ...(answer === undefined ? {} : { answer }) }
  })
}

export function refinedPrompt(intake: Intake | undefined): string | undefined {
  const prompt = intake?.refinedPrompt
  return typeof prompt === "string" && isSubstantive(prompt) ? prompt : undefined
}
