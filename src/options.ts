import { z } from "zod"
import { DEFAULT_IN_PROGRESS_CAP } from "./types"

const DEFAULT_HELPER_RETRIES = 1
const DEFAULT_SEND_BACK_STOP_THRESHOLD = 3

export const OptionsSchema = z.object({
  inProgressLimit: z.number().int().min(1).catch(DEFAULT_IN_PROGRESS_CAP),
  helperRetries: z.number().int().min(0).catch(DEFAULT_HELPER_RETRIES),
  sendBackStopThreshold: z.number().int().min(1).catch(DEFAULT_SEND_BACK_STOP_THRESHOLD),
  squashMerge: z.boolean().catch(true),
  intakeAgent: z.string().optional().catch(undefined),
  validatorAgent: z.string().optional().catch(undefined),
})

export type Options = z.infer<typeof OptionsSchema>

export function parseOptions(raw?: unknown): Options {
  return OptionsSchema.parse(typeof raw === "object" && raw !== null ? raw : {})
}
