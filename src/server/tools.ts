import type { PluginInput } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin/tool"
import { worktreeDiffs } from "../git/diffs"
import { shellGitRunner } from "../git/runner"
import { sanitizeIntakeDecisions } from "../domain/task/intake"
import { verifyFindingCitations, type Finding } from "../domain/task/findings"
import { helper } from "../domain/task/policy"
import { kagan, validMode } from "../domain/task/metadata"
import { getSessionData } from "./data"
import { patchKagan } from "./session/patch"
import { createTasksTool } from "./create-tasks-tool"

const intakeArgs = {
  understanding: tool.schema.string().describe("A short summary of what this task means and how you would approach it"),
  decisions: tool.schema
    .array(
      tool.schema.object({
        id: tool.schema.string().describe("Stable decision id"),
        question: tool.schema.string().describe("The question the human should confirm"),
        assumption: tool.schema.string().describe("The default assumption being made"),
        required: tool.schema.boolean().optional().describe("False only when work can proceed safely either way"),
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
      rationale: tool.schema.string().describe("One-line rationale for the recommendation, based on the five factors"),
    })
    .optional()
    .describe("Advisory mode recommendation for the human — not a gate"),
}

const findingsArgs = {
  findings: tool.schema
    .array(
      tool.schema.object({
        id: tool.schema.string().describe("Stable finding id"),
        summary: tool.schema.string().describe("What the validator found"),
        detail: tool.schema
          .string()
          .optional()
          .describe("Full problem statement, quoting the offending lines if useful"),
        location: tool.schema.string().optional().describe("Pointer to the offending code, as `file:line`"),
        severity: tool.schema.enum(["low", "medium", "high"]).optional(),
        confidence: tool.schema.number().min(0).max(10).optional().describe("Confidence this finding is real, 0 to 10"),
        category: tool.schema
          .enum(["misalignment", "bug", "uncertainty"])
          .describe("misalignment = agreed task diverges; bug = defect; uncertainty = cannot verify"),
      }),
    )
    .describe("Findings from the diff review"),
}

export function createServerTools(input: PluginInput, options?: Record<string, unknown>) {
  return {
    kagan_intake: tool({
      description: "Record the read-only intake assessment for the parent kagan task",
      args: intakeArgs,
      async execute(args, ctx) {
        const parentID = helper((await getSessionData(input, ctx.sessionID))?.metadata, "intake").parent
        if (!parentID) throw new Error("kagan_intake is only available in intake sessions")
        const decisions = sanitizeIntakeDecisions(args.decisions)
        const mode = validMode(args.mode)
        const intake: Record<string, unknown> = {
          understanding: args.understanding,
          decisions,
          refinedPrompt: args.refinedPrompt,
        }
        if (mode) intake.mode = mode
        if (helper((await getSessionData(input, parentID))?.metadata, "intake").sessionID !== ctx.sessionID) {
          return { output: "Intake not recorded — this prep session was superseded by a newer one." }
        }
        await patchKagan(input.client, parentID, { intake, intakeOutcome: "ran", helperError: undefined })
        return { output: `Recorded intake with ${decisions.length} decision(s).` }
      },
    }),
    kagan_findings: tool({
      description: "Record validator findings for the parent kagan task",
      args: findingsArgs,
      async execute(args, ctx) {
        const parentID = helper((await getSessionData(input, ctx.sessionID))?.metadata, "validator").parent
        if (!parentID) throw new Error("kagan_findings is only available in validator sessions")
        const parent = await getSessionData(input, parentID)
        const view = kagan(parent?.metadata)
        let findings: Finding[] = args.findings
        if (view.worktree) {
          try {
            findings = verifyFindingCitations(
              findings,
              await worktreeDiffs(shellGitRunner(input.$), view.worktree, view.baseBranch ?? "HEAD"),
            )
          } catch {}
        }
        if (helper((await getSessionData(input, parentID))?.metadata, "validator").sessionID !== ctx.sessionID) {
          return { output: "Findings not recorded — this review was superseded by a newer iteration." }
        }
        await patchKagan(input.client, parentID, { findings, validatorOutcome: "ran", helperError: undefined })
        return { output: `Recorded ${args.findings.length} finding(s).` }
      },
    }),
    kagan_create_tasks: createTasksTool(input, options),
  }
}
