import type { PluginInput } from "@opencode-ai/plugin"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { Finding, Intake, ModelRef } from "./task"
import type { CheckResult } from "./check"
import { isolatedEvidenceBlock } from "./handoff"
import { parseOptions } from "./options"
import { patchKagan } from "./session-api"
import { orderDiffsByRisk } from "./git"

const PATCH_CHAR_LIMIT = 8000
const DIFF_CHAR_BUDGET = 60000
const LOCKFILE_BASENAMES = new Set([
  "bun.lock",
  "bun.lockb",
  "package-lock.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "Cargo.lock",
  "poetry.lock",
  "uv.lock",
])

function statLine(file: string, reason: string, diff: SnapshotFileDiff): string {
  return `--- ${file} (${reason}, +${diff.additions ?? 0}/-${diff.deletions ?? 0})`
}

function formatDiffsForPrompt(diffs: Array<SnapshotFileDiff>): string {
  let budgetRemaining = DIFF_CHAR_BUDGET
  let budgetExhausted = false
  return orderDiffsByRisk(diffs)
    .map((diff) => {
      const file = diff.file ?? "unknown"
      const basename = file.split("/").pop() ?? file
      if (LOCKFILE_BASENAMES.has(basename)) {
        return statLine(file, "lockfile — patch omitted", diff)
      }

      let patch = diff.patch ?? ""
      if (patch.length > PATCH_CHAR_LIMIT) {
        patch = `${patch.slice(0, PATCH_CHAR_LIMIT)}\n[patch truncated — showing ${PATCH_CHAR_LIMIT} of ${patch.length} chars; read the file in the worktree for full context]`
      }

      if (budgetExhausted || patch.length > budgetRemaining) {
        budgetExhausted = true
        return statLine(file, "patch omitted — diff budget exhausted", diff)
      }
      budgetRemaining -= patch.length

      return `--- ${file}\n${patch}`
    })
    .join("\n\n")
}

function formatContext(context: { title: string; description?: string; intake?: Intake }): string {
  const lines = [`Title: ${context.title}`]
  if (context.description) lines.push(`Description: ${context.description}`)
  const intake = context.intake
  if (intake) {
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
  }
  return lines.join("\n")
}

function formatPriorTriage(priorTriage?: Finding[]): string | undefined {
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

function isModelRef(value: unknown): value is ModelRef {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { providerID?: unknown }).providerID === "string" &&
    typeof (value as { modelID?: unknown }).modelID === "string"
  )
}

export function resolveValidatorModel(
  options?: Record<string, unknown>,
  generation?: number,
  builderModel?: ModelRef,
): ModelRef | undefined {
  // A different model family reduces self-preference bias, but it is still an LLM, not an oracle.
  // Deterministic checks remain the verified half of the judgment.
  const modelsOption = options?.validatorModels
  if (!Array.isArray(modelsOption) || modelsOption.length === 0) return undefined
  const models = modelsOption.filter(isModelRef)
  if (models.length === 0) return undefined
  const preferred = builderModel?.providerID
    ? models.filter((model) => model.providerID !== builderModel.providerID)
    : []
  const pool = preferred.length > 0 ? preferred : models
  const index = Math.max(1, generation ?? 1) - 1
  return pool[index % pool.length]
}

function formatCheck(check: CheckResult): string {
  let body: string
  if (check.steps && check.steps.length > 0) {
    const ran = check.steps.filter((step) => step.status === "ran")
    const skipped = check.steps.filter((step) => step.status === "skipped")
    const sections = ["Deterministic check evidence:"]
    if (ran.length > 0) {
      sections.push(
        [
          "Ran checks:",
          ...ran.map((step) => {
            const exit = step.exitCode === null ? "?" : step.exitCode
            return `- ${step.name} (${step.cwd}) — \`${step.command}\` exited ${exit}:\n${step.output}`
          }),
        ].join("\n\n"),
      )
    }
    if (skipped.length > 0) {
      sections.push(
        [
          "Skipped checks:",
          ...skipped.map((step) => `- ${step.name} (${step.cwd}): ${step.reason ?? "not in scope"}`),
        ].join("\n"),
      )
    }
    body = sections.join("\n\n")
  } else if (check.exitCode === null) {
    body = `Deterministic check \`${check.command}\` did not complete: ${check.output}`
  } else {
    body = `Deterministic check evidence — \`${check.command}\` exited ${check.exitCode}:\n${check.output}`
  }
  return isolatedEvidenceBlock("Check output", body)
}

export async function spawnValidator(
  input: PluginInput,
  parentSessionID: string,
  diffs: Array<SnapshotFileDiff>,
  context: {
    title: string
    description?: string
    intake?: Intake
    priorTriage?: Finding[]
    generation: number
    check?: CheckResult
    builderModel?: ModelRef
  },
  options?: Record<string, unknown>,
): Promise<string | undefined> {
  const model = resolveValidatorModel(options, context.generation, context.builderModel)

  const child = await input.client.session.create({
    body: {
      parentID: parentSessionID,
      title: context.generation > 1 ? `review #${context.generation}` : "review",
      metadata: {
        kagan: {
          validatorParent: parentSessionID,
          role: "validator",
        },
      },
    },
    throwOnError: true,
  } as Parameters<typeof input.client.session.create>[0])

  const childID = child.data?.id
  if (!childID) return undefined

  await patchKagan(input.client, parentSessionID, { validatorSessionID: childID })

  const priorTriage = formatPriorTriage(context.priorTriage)
  const check = context.check
    ? [
        formatCheck(context.check),
        "Treat the check result as corroborating evidence, not as a source to copy verbatim. If the check failed, consider whether the failure illuminates a real problem with the diff, but do not invent findings that the output does not support.",
      ].join("\n\n")
    : undefined
  const promptText = [
    "Review the following diff against the task it was meant to implement.",
    "",
    formatContext(context),
    "",
    ...(priorTriage ? [priorTriage, ""] : []),
    ...(check ? [check, ""] : []),
    "For every issue you find, classify it with a category:",
    "- misalignment: the change diverges from the agreed task.",
    "- bug: a defect in the code.",
    "- uncertainty: you cannot verify the change is correct.",
    "Scope audit: enumerate every change the refined prompt and resolved decisions did not ask for — including changed constants/values and added/changed dependencies with no stated reason — and report each as a misalignment finding, noting whether it looks beneficial, neutral, or drift.",
    "Test integrity: for every added or changed test, check it can actually fail when the logic it covers breaks; report a test that passes regardless as a bug finding.",
    'Evidence bar: do not report findings that rest on speculation about code you have not read — verify against the worktree first. A claim with no concrete failure mode ("could", "might", "consider") warrants confidence 2 or lower.',
    "Human verification list: report anything the diff alone cannot prove — runtime behavior, visuals, device-specific behavior — as an uncertainty finding so the human gets a directed checklist of what to verify by hand.",
    "Score each finding's confidence from 0 (likely false positive) to 10 (certain it's real).",
    "For each finding also give `detail` (your full reasoning, quoting the offending lines) and `location` (`file:line`) so the human can judge it without re-reading the whole diff. `location` must point at a changed line in the diff; put supporting locations in `detail`.",
    "Call the kagan_findings tool exactly once with all findings; if the diff is clean, call it with an empty array.",
    "",
    formatDiffsForPrompt(diffs) || "(no diff)",
  ].join("\n")

  const body: Record<string, unknown> = {
    tools: { read: true, edit: false, write: false, bash: false, kagan_findings: true },
    parts: [{ type: "text", text: promptText }],
  }
  if (model) body.model = { providerID: model.providerID, modelID: model.modelID }
  const validatorAgent = parseOptions(options).validatorAgent
  if (validatorAgent !== undefined) body.agent = validatorAgent

  await input.client.session.promptAsync({
    path: { id: childID },
    body,
    throwOnError: true,
  } as Parameters<typeof input.client.session.promptAsync>[0])

  return childID
}
