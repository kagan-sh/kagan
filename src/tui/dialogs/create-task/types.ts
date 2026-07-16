import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { ModelRef } from "../../../domain/task/types"
import type { TaskScope } from "../../../domain/task/commands"
import type { createTask } from "../../tasks"
import type { getOrder, setOrder } from "../../session/preferences"

export type ModelChoice = { label: string; model?: ModelRef }

export type CreateTaskDependencies = {
  createTask: typeof createTask
  getOrder: typeof getOrder
  setOrder: typeof setOrder
  listBranches: (api: TuiPluginApi) => Promise<string[]>
}

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
