import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { CheckResult } from "../../checks/runner"
import { isolatedEvidenceBlock } from "../../domain/handoff"
import { formatContext, formatPriorTriage, type ValidatorContext } from "./context"
import { formatDiffsForPrompt } from "./diffs"

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

function formatCheckGuidance(check?: CheckResult): string | undefined {
  if (!check) return undefined
  return [
    formatCheck(check),
    "Treat the check result as corroborating evidence, not as a source to copy verbatim. If the check failed, consider whether the failure illuminates a real problem with the diff, but do not invent findings that the output does not support.",
  ].join("\n\n")
}

export function buildValidatorPrompt(
  diffs: Array<SnapshotFileDiff>,
  context: ValidatorContext & { check?: CheckResult },
): string {
  const priorTriage = formatPriorTriage(context.priorTriage)
  const check = formatCheckGuidance(context.check)
  return [
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
}
