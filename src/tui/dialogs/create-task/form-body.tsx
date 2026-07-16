/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { RGBA, TextareaRenderable } from "@opentui/core"
import { Show } from "solid-js"
import { DialogFrame } from "../chrome"
import type { FormState, ModelChoice } from "./types"

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

export function CreateTaskFormBody(props: {
  api: TuiPluginApi
  state: FormState
  models: ModelChoice[]
  branches: string[]
  focusIndex: number
  labelColor: (index: number) => RGBA
  descriptionRef: (el: TextareaRenderable) => void
}) {
  const theme = () => props.api.theme.current
  return (
    <DialogFrame api={props.api} title="New task">
      <box flexDirection="column">
        <text fg={props.labelColor(0)}>Title</text>
        <input
          focused={props.focusIndex === 0}
          value={props.state.title}
          placeholder="What should the agent do?"
          onInput={(value) => {
            props.state.title = value
          }}
        />
      </box>
      <box flexDirection="column">
        <text fg={props.labelColor(1)}>Description</text>
        <textarea
          height={3}
          focused={props.focusIndex === 1}
          initialValue={props.state.description}
          placeholder="Optional context or constraints"
          ref={props.descriptionRef}
        />
      </box>
      <PickerRow api={props.api} label="Scope" value={scopeLabel(props.state.scope)} focused={props.focusIndex === 2} />
      <PickerRow
        api={props.api}
        label="Model"
        value={props.models[props.state.modelIndex]?.label ?? "Auto (session default)"}
        focused={props.focusIndex === 3}
      />
      <PickerRow
        api={props.api}
        label="Base branch"
        value={props.branches[props.state.branchIndex] ?? "HEAD"}
        focused={props.focusIndex === 4}
      />
      <box paddingBottom={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          tab <span style={{ fg: theme().textMuted }}>move</span>
        </text>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>create</span>
        </text>
        <Show when={props.focusIndex === 1}>
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
