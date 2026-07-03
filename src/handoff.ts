import { getRefinedPrompt, kagan, type Finding } from "./task"

const MAX_REFERENCED_REPORT = 2000

export function parseTaskRefs(text: string): number[] {
  const seen = new Set<number>()
  const refs: number[] = []
  for (const match of text.matchAll(/(?:^|\s)#(\d+)\b/g)) {
    const number = Number(match[1])
    if (seen.has(number)) continue
    seen.add(number)
    refs.push(number)
  }
  return refs
}

export function formatTaskRef(input: {
  number: number
  title?: string
  status?: string
  understanding?: string
  report?: string
}): string {
  if (input.title === undefined) return `(#${input.number} not found)`
  const status = input.status ? ` (${input.status})` : ""
  const sections = [`## Referenced task #${input.number} — ${input.title}${status}`]
  if (input.understanding?.trim()) sections.push(input.understanding.trim())
  if (input.report?.trim()) sections.push(input.report.trim().slice(0, MAX_REFERENCED_REPORT))
  return sections.join("\n\n")
}

function taskBody(title: string, metadata?: Record<string, unknown>): string {
  const refined = getRefinedPrompt(metadata)
  const description = kagan(metadata).description
  if (refined && description) return `${refined}\n\n## Original task description\n${description}`
  return refined ?? description ?? title
}

export function composeStartPrompt(title: string, metadata?: Record<string, unknown>): string {
  const sections = [taskBody(title, metadata)]
  const intake = kagan(metadata).intake
  if (intake) {
    const resolved = intake.decisions.filter((d) => d.resolution === "approved" || d.resolution === "overridden")
    if (resolved.length > 0) {
      const lines = resolved.map((d) =>
        d.resolution === "approved"
          ? `- ${d.question} — assumption holds: ${d.assumption}`
          : `- ${d.question} — user answered: ${d.answer ?? ""}`,
      )
      sections.push(`## Confirmed decisions\n${lines.join("\n")}`)
    }
    if (intake.understanding.trim()) {
      sections.push(`## Intake understanding\n${intake.understanding}`)
    }
  }
  return sections.join("\n\n")
}

function findingLabel(finding: Finding): string {
  return finding.category ? `[${finding.category}] ${finding.summary}` : finding.summary
}

export function composeHandoffPrompt(input: {
  title: string
  metadata?: Record<string, unknown>
  previousReport?: string
  changedFiles: readonly string[]
}): string {
  const sections = [
    taskBody(input.title, input.metadata),
    `## Previous iteration report\n${input.previousReport?.trim() || "(no report)"}`,
    `## Files already changed in this worktree\n${
      input.changedFiles.length > 0 ? input.changedFiles.map((file) => `- ${file}`).join("\n") : "(none)"
    }`,
  ]

  const findings = kagan(input.metadata).findings ?? []
  const toAddress = findings.filter((f) => f.resolution === undefined || f.resolution === "clarified")
  if (toAddress.length > 0) {
    const lines = toAddress.map((f) => {
      const base = `- ${findingLabel(f)}`
      return f.resolution === "clarified" && f.note ? `${base}\n  Clarification: ${f.note}` : base
    })
    sections.push(`## Review findings to address\n${lines.join("\n")}`)
  }

  const intended = [
    ...findings.filter((f) => f.resolution === "intended"),
    ...(kagan(input.metadata).priorTriage ?? []).filter((f) => f.resolution === "intended"),
  ]
  if (intended.length > 0) {
    const lines = intended.map((f) => {
      const base = `- ${findingLabel(f)}`
      return f.note ? `${base}\n  ${f.note}` : base
    })
    sections.push(`## Intended behavior — do not change\n${lines.join("\n")}`)
  }

  sections.push("Continue the work in place in this worktree; do not start over.")
  return sections.join("\n\n")
}
