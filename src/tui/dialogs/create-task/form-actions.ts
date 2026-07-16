import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { createBoardStore } from "../../board/store"
import type { CreateTaskDependencies, FormState, ModelChoice } from "./types"
import { openScopePicker } from "../create-task-scope"
import { openFilterableSelectPicker } from "./select"

type BoardStore = ReturnType<typeof createBoardStore>

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

export function handleCreateTaskKey(props: {
  key: { name: string; ctrl?: boolean; shift?: boolean }
  focusIndex: () => number
  setFocusIndex: (value: number | ((index: number) => number)) => void
  descriptionRef: DescriptionField | undefined
  submit: () => void
  openPicker: () => void
}): boolean {
  const { key } = props
  if (key.ctrl && key.name === "return") {
    props.submit()
    return true
  }
  if (
    props.focusIndex() === 1 &&
    ((key.ctrl && key.name === "j") || key.name === "linefeed" || (key.shift && key.name === "return"))
  ) {
    props.descriptionRef?.newLine?.()
    return true
  }
  if (key.name === "return") {
    if (props.focusIndex() >= 2) props.openPicker()
    else props.submit()
    return true
  }
  if (key.name === "right" && props.focusIndex() >= 2) {
    props.openPicker()
    return true
  }
  if (key.name === "tab") {
    props.setFocusIndex((index) => (key.shift ? (index + 4) % 5 : (index + 1) % 5))
    return true
  }
  if (props.focusIndex() === 1) return false
  if (key.name === "down") {
    props.setFocusIndex((index) => (index + 1) % 5)
    return true
  }
  if (key.name === "up") {
    props.setFocusIndex((index) => (index + 4) % 5)
    return true
  }
  return false
}
