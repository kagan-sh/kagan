import type { ModelRef } from "../../../domain/task/types"
import type { TaskScope } from "../../../domain/task/commands"

export type ModelChoice = { label: string; model?: ModelRef }

export type FormState = {
  title: string
  description: string
  scope: TaskScope
  scopeFilter: string
  modelIndex: number
  modelFilter: string
  branchIndex: number
  branchFilter: string
  focusIndex: number
}
