import { readFile, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { OptionBoundsSchema } from "../../../domain/options"
import { commandPlan } from "../../../domain/task/commands"
import { helperRetries, inProgressCap, sendBackStopThreshold, squashMerge } from "../../../domain/task/policy"
import type { CommandSpec, ModelRef } from "../../../domain/task/types"

export type Section = "General" | "Agents" | "Commands" | "Validator models" | "JSON preview"

export type Draft = {
  inProgressLimit: number
  helperRetries: number
  sendBackStopThreshold: number
  squashMerge: boolean
  intakeAgent: string
  validatorAgent: string
  validatorModels: ModelRef[]
  commands: {
    setup: CommandSpec[]
    check: CommandSpec[]
  }
}

export const SECTIONS: Section[] = ["General", "Agents", "Commands", "Validator models", "JSON preview"]

function modelRef(value: unknown): ModelRef | undefined {
  if (typeof value !== "object" || value === null) return undefined
  const raw = value as Record<string, unknown>
  const providerID = typeof raw.providerID === "string" ? raw.providerID.trim() : ""
  const modelID = typeof raw.modelID === "string" ? raw.modelID.trim() : ""
  if (!providerID || !modelID) return undefined
  return { providerID, modelID }
}

export function draftFromOptions(options?: Record<string, unknown>): Draft {
  return {
    inProgressLimit: inProgressCap(options),
    helperRetries: helperRetries(options),
    sendBackStopThreshold: sendBackStopThreshold(options),
    squashMerge: squashMerge(options),
    intakeAgent: typeof options?.intakeAgent === "string" ? options.intakeAgent : "",
    validatorAgent: typeof options?.validatorAgent === "string" ? options.validatorAgent : "",
    validatorModels: Array.isArray(options?.validatorModels)
      ? options.validatorModels.map(modelRef).filter((model): model is ModelRef => model !== undefined)
      : [],
    commands: {
      setup: commandPlan(options, "setup"),
      check: commandPlan(options, "check"),
    },
  }
}

function optionsFromDraft(draft: Draft): Record<string, unknown> {
  const options: Record<string, unknown> = {
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
    squashMerge: draft.squashMerge,
  }
  if (draft.intakeAgent.trim()) options.intakeAgent = draft.intakeAgent.trim()
  if (draft.validatorAgent.trim()) options.validatorAgent = draft.validatorAgent.trim()
  if (draft.validatorModels.length > 0) options.validatorModels = draft.validatorModels
  if (draft.commands.setup.length > 0 || draft.commands.check.length > 0) {
    options.commands = draft.commands
  }
  return options
}

export function pluginOptionsJson(draft: Draft): string {
  return JSON.stringify(optionsFromDraft(draft), null, 2)
}

function validateValidatorModels(value: ModelRef[]): string | undefined {
  for (let i = 0; i < value.length; i++) {
    const item = value[i]
    if (item === undefined) continue
    if (!item.providerID.trim() || !item.modelID.trim()) {
      return `validatorModels[${i}] must be { providerID: string, modelID: string }`
    }
  }
  return undefined
}

function validateDraft(draft: Draft): string | undefined {
  const modelError = validateValidatorModels(draft.validatorModels)
  if (modelError) return modelError
  const bounds = OptionBoundsSchema.safeParse({
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
  })
  if (bounds.success) return undefined
  const field = bounds.error.issues[0]?.path[0]
  if (field === "inProgressLimit") return "inProgressLimit must be at least 1"
  if (field === "helperRetries") return "helperRetries must be at least 0"
  if (field === "sendBackStopThreshold") return "sendBackStopThreshold must be at least 1"
  return bounds.error.issues[0]?.message ?? "Invalid settings"
}

export async function saveOptions(worktree: string, draft: Draft): Promise<string> {
  const error = validateDraft(draft)
  if (error) throw new Error(error)
  const path = join(worktree, "opencode.json")
  let raw: string
  try {
    raw = await readFile(path, "utf8")
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && (error as { code: string }).code === "ENOENT") {
      throw new Error("opencode.json not found in project root")
    }
    throw error
  }
  const config = JSON.parse(raw) as { plugin?: unknown }
  if (!Array.isArray(config.plugin)) throw new Error("opencode.json has no plugin array")
  const index = config.plugin.findIndex((entry) => {
    const path = Array.isArray(entry) ? entry[0] : entry
    return typeof path === "string" && path.includes("kagan")
  })
  if (index === -1) throw new Error("opencode.json has no Kagan plugin entry")
  const entry = config.plugin[index]
  const pathValue = Array.isArray(entry) ? entry[0] : entry
  config.plugin[index] = [pathValue, optionsFromDraft(draft)]
  await writeFile(path, `${JSON.stringify(config, null, 2)}\n`)
  return "Saved opencode.json. Restart OpenCode or reopen the project to apply changes."
}
