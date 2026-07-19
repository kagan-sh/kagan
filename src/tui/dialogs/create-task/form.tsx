/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { TextareaRenderable } from "@opentui/core"
import { createSignal, onMount, Show } from "solid-js"
import type { BoardStore } from "../../board/store"
import { useKeyIntercept } from "../../renderer"
import { DialogFrame } from "../chrome"
import {
  handleCreateTaskKey,
  openCreateTaskPicker,
  submitCreateTask,
  type CreateTaskDependencies,
  type FormState,
  type ModelChoice,
} from "./form-actions"

function PickerRow(props: { api: TuiPluginApi; label: string; value: string; focused: boolean }) {
  const theme = () => props.api.theme.current
  return (
    <box flexDirection="row" justifyContent="space-between">
      <box flexDirection="row" gap={1}>
        <text fg={props.focused ? theme().primary : theme().textMuted}>{props.label}</text>
        <text fg={theme().text}>{props.value}</text>
      </box>
      <text fg={theme().textMuted}>›</text>
    </box>
  )
}

function scopeLabel(scope: FormState["scope"]): string {
  const parts = [...scope.values]
  if (scope.custom) parts.push(scope.custom)
  return parts.length > 0 ? parts.join(", ") : "Not set"
}

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
    <DialogFrame api={props.api} title="New task">
      <box flexDirection="column">
        <text fg={labelColor(0)}>Title</text>
        <input
          focused={focusIndex() === 0}
          value={state.title}
          placeholder="What should the agent do?"
          onInput={(value) => {
            state.title = value
          }}
        />
      </box>
      <box flexDirection="column">
        <text fg={labelColor(1)}>Description</text>
        <textarea
          height={3}
          focused={focusIndex() === 1}
          initialValue={state.description}
          placeholder="Optional context or constraints"
          ref={(el) => {
            descriptionRef = el
          }}
        />
      </box>
      <PickerRow api={props.api} label="Scope" value={scopeLabel(state.scope)} focused={focusIndex() === 2} />
      <PickerRow
        api={props.api}
        label="Model"
        value={props.models[state.modelIndex]?.label ?? "Auto (session default)"}
        focused={focusIndex() === 3}
      />
      <PickerRow
        api={props.api}
        label="Base branch"
        value={props.branches[state.branchIndex] ?? "HEAD"}
        focused={focusIndex() === 4}
      />
      <box flexDirection="row" gap={2}>
        <text fg={theme().text}>
          tab <span style={{ fg: theme().textMuted }}>move</span>
        </text>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>create</span>
        </text>
        <Show when={focusIndex() === 1}>
          <text fg={theme().text}>
            ctrl+j <span style={{ fg: theme().textMuted }}>newline</span>
          </text>
        </Show>
        <text fg={theme().text}>
          esc <span style={{ fg: theme().textMuted }}>cancel</span>
        </text>
      </box>
    </DialogFrame>
  )
}
