/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { TextareaRenderable } from "@opentui/core"
import { createSignal, onMount } from "solid-js"
import type { createBoardStore } from "../../board/store"
import { useKeyIntercept } from "../../renderer"
import type { CreateTaskDependencies, FormState, ModelChoice } from "./types"
import { CreateTaskFormBody } from "./form-body"
import { handleCreateTaskKey, openCreateTaskPicker, submitCreateTask } from "./form-actions"

type BoardStore = ReturnType<typeof createBoardStore>

export function CreateTaskForm(props: {
  api: TuiPluginApi
  store: BoardStore
  branches: string[]
  models: ModelChoice[]
  state: FormState
  reopen: () => void
  dependencies: CreateTaskDependencies
}) {
  const theme = () => props.api.theme.current
  const state = props.state
  const [focusIndex, setFocusIndex] = createSignal(state.focusIndex)
  let descriptionRef: TextareaRenderable | undefined

  onMount(() => props.api.ui.dialog.setSize("medium"))

  const submit = () =>
    void submitCreateTask({
      api: props.api,
      store: props.store,
      state,
      models: props.models,
      branches: props.branches,
      descriptionRef,
      dependencies: props.dependencies,
    })

  const openPicker = () =>
    openCreateTaskPicker({
      api: props.api,
      store: props.store,
      state,
      models: props.models,
      branches: props.branches,
      focusIndex: focusIndex(),
      descriptionRef,
      reopen: props.reopen,
    })

  useKeyIntercept(props.api, (key) =>
    handleCreateTaskKey({ key, focusIndex, setFocusIndex, descriptionRef, submit, openPicker }),
  )

  const labelColor = (index: number) => (focusIndex() === index ? theme().primary : theme().textMuted)

  return (
    <CreateTaskFormBody
      api={props.api}
      state={state}
      models={props.models}
      branches={props.branches}
      focusIndex={focusIndex()}
      labelColor={labelColor}
      descriptionRef={(el) => {
        descriptionRef = el
      }}
    />
  )
}
