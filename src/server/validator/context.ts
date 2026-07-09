import type { Finding } from "../../domain/task/findings"
import type { Intake } from "../../domain/task/intake"
import { isolatedEvidenceBlock } from "../../domain/handoff"

export interface ValidatorContext {
  title: string
  description?: string
  intake?: Intake
  priorTriage?: Finding[]
  generation: number
}

export function formatContext(context: ValidatorContext): string {
  const lines = [`Title: ${context.title}`]
  if (context.description) lines.push(`Description: ${context.description}`)
  const intake = context.intake
  if (!intake) return lines.join("\n")

  if (intake.understanding.trim()) {
    lines.push("", isolatedEvidenceBlock("Intake understanding", intake.understanding))
  }
  const resolved = intake.decisions.filter((d) => d.resolution === "approved" || d.resolution === "overridden")
  if (resolved.length > 0) {
    lines.push("Resolved decisions:")
    for (const d of resolved) {
      const answer = d.resolution === "approved" ? d.assumption : (d.answer ?? "")
      lines.push(`- ${d.question} → ${answer}`)
    }
  }
  if (intake.refinedPrompt?.trim()) {
    lines.push("", isolatedEvidenceBlock("Refined prompt", intake.refinedPrompt))
  }
  return lines.join("\n")
}

export function formatPriorTriage(priorTriage?: Finding[]): string | undefined {
  if (!priorTriage || priorTriage.length === 0) return undefined
  return [
    "The human has already reviewed these issues in earlier iterations and ruled on them.",
    "Do not re-report them or close variations of them:",
    ...priorTriage.map((finding) => {
      const category = finding.category ? `[${finding.category}] ` : ""
      const resolution = finding.resolution ? ` — ruled ${finding.resolution}` : ""
      const note = finding.note ? `: ${finding.note}` : ""
      return `- ${category}${finding.summary}${resolution}${note}`
    }),
  ].join("\n")
}
