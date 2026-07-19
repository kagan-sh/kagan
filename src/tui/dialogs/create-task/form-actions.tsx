/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { BoardStore } from "../../board/store"
import { openFilterableSelectPicker, openScopePicker } from "./pickers"
import type { CreateTaskDependencies, FormState, ModelChoice } from "./types"

export type { CreateTaskDependencies, FormState, ModelChoice } from "./types"
type DescriptionField = { plainText?: string; newLine?: () => void }

function hasScope(scope: FormState["scope"]): boolean {
  return scope.values.length > 0 || !!scope.custom
}

export async function submitCreateTask(props: {
  api: TuiPluginApi
  store: BoardStore
  state: FormState
  models: ModelChoice[]
  branches: string[]
  descriptionRef: DescriptionField | undefined
  dependencies: CreateTaskDependencies
}): Promise<void> {
  const trimmed = props.state.title.trim()
  if (!trimmed) {
    props.store.notify({ variant: "warning", title: "Kagan", message: "Title is required" })
    return
  }
  const description = props.descriptionRef?.plainText ?? props.state.description
  if (props.store.configuredScopes.length > 1 && !hasScope(props.state.scope)) {
    props.store.notify({ variant: "warning", title: "Kagan", message: "Scope is required" })
    return
  }
  props.api.ui.dialog.clear()
  try {
    const scope = hasScope(props.state.scope) ? props.state.scope : undefined
    const input = {
      title: trimmed,
      description,
      model: props.models[props.state.modelIndex]?.model,
      baseBranch: props.branches[props.state.branchIndex] ?? "HEAD",
      setupCommands: props.store.setupCommands,
      ...(scope ? { scope } : {}),
    }
    const session = await props.dependencies.createTask(props.api, input)
    props.dependencies.setOrder(props.api, "backlog", [
      ...props.dependencies.getOrder(props.api, "backlog"),
      session.id,
    ])
    await props.store.refresh()
    props.store.notify({ variant: "success", title: "Kagan", message: `Created "${trimmed}"` })
  } catch (error) {
    props.store.notify({
      variant: "error",
      title: "Kagan",
      message: error instanceof Error ? error.message : String(error),
    })
  }
}

export function openCreateTaskPicker(props: {
  api: TuiPluginApi
  store: BoardStore
  state: FormState
  models: ModelChoice[]
  branches: string[]
  focusIndex: number
  descriptionRef: DescriptionField | undefined
  reopen: () => void
}): void {
  props.state.focusIndex = props.focusIndex
  props.state.description = props.descriptionRef?.plainText ?? props.state.description
  if (props.focusIndex === 2) {
    openScopePicker(props.api, props.store.configuredScopes, props.state, props.reopen)
    return
  }
  if (props.focusIndex === 3) {
    openFilterableSelectPicker(props.api, {
      title: "Model",
      filterPlaceholder: "Filter models",
      labels: props.models.map((choice) => choice.label),
      selectedIndex: props.state.modelIndex,
      filter: props.state.modelFilter,
      onFilter: (value) => {
        props.state.modelFilter = value
      },
      onSelect: (index) => {
        props.state.modelIndex = index
      },
      reopen: props.reopen,
    })
    return
  }
  openFilterableSelectPicker(props.api, {
    title: "Base branch",
    filterPlaceholder: "Filter branches",
    labels: props.branches,
    selectedIndex: props.state.branchIndex,
    filter: props.state.branchFilter,
    onFilter: (value) => {
      props.state.branchFilter = value
    },
    onSelect: (index) => {
      props.state.branchIndex = index
    },
    reopen: props.reopen,
  })
}

export { handleCreateTaskKey } from "./form-keys"
