import type { PluginInput } from "@opencode-ai/plugin"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { Finding } from "../../domain/task/findings"
import type { Intake } from "../../domain/task/intake"
import type { ModelRef } from "../../domain/task/types"
import type { CheckResult } from "../../checks/runner"
import { parseOptions } from "../../domain/options"
import { createHelperChild } from "../helpers/child"
import { buildValidatorPrompt } from "./prompt"

function isModelRef(value: unknown): value is ModelRef {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { providerID?: unknown }).providerID === "string" &&
    typeof (value as { modelID?: unknown }).modelID === "string"
  )
}

function resolveValidatorModel(
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

  const childID = await createHelperChild(
    input,
    parentSessionID,
    "validator",
    context.generation > 1 ? `review #${context.generation}` : "review",
  )
  if (!childID) return undefined

  const promptText = buildValidatorPrompt(diffs, context)

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
