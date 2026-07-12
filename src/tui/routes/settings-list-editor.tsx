/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi, TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { type Accessor, type Setter, For, Show, createSignal } from "solid-js"
import { commandSpec } from "../../domain/task/commands"
import type { CommandSpec, ModelRef } from "../../domain/task/types"
import { useKeyIntercept } from "../renderer"
import { DialogFrame } from "../dialogs/chrome"

type ListEditorState<T> = {
  items: T[]
  row: number
  field: number
  message?: string
}

type ListEditorColumn<T> = {
  field: string
  value: (item: T) => string
  width?: number
  flexGrow?: number
}

/** Shared handle over an editor's live signals, passed to the module-level add/edit/delete/move handlers. */
type EditorContext<T> = {
  items: Accessor<T[]>
  setItems: Setter<T[]>
  selectedRow: Accessor<number>
  setRowIndex: Setter<number>
  setFieldIndex: Setter<number>
  focusedField: Accessor<string>
  setMessage: Setter<string | undefined>
  reopenWithSnapshot: () => void
  prompt: (title: string, value: string, onConfirm: (value: string) => void) => void
}

/**
 * The host dialog stack destroys and recreates a dialog whenever a sub-prompt opens, so every edit
 * path snapshots the live signal state back into `state` before opening a prompt and restores it on
 * reopen. Dropping this makes in-flight edits lose their row/field/message on the round trip.
 */
function listEditorDialogControls<T>(props: {
  api: TuiPluginApi
  state: ListEditorState<T>
  items: Accessor<T[]>
  selectedRow: Accessor<number>
  fieldIndex: Accessor<number>
  message: Accessor<string | undefined>
  reopen: () => void
}) {
  const snapshot = () => ({
    items: props.items(),
    row: props.selectedRow(),
    field: props.fieldIndex(),
    message: props.message(),
  })
  const reopenWithSnapshot = () => {
    Object.assign(props.state, snapshot())
    props.reopen()
  }
  const prompt = (title: string, value: string, onConfirm: (value: string) => void) => {
    Object.assign(props.state, snapshot())
    props.api.ui.dialog.replace(() => (
      <props.api.ui.DialogPrompt
        title={title}
        value={value}
        placeholder={title}
        onConfirm={onConfirm}
        onCancel={props.reopen}
      />
    ))
  }
  return { reopenWithSnapshot, prompt }
}

function listEditorSignals<T>(state: ListEditorState<T>) {
  const [items, setItems] = createSignal(state.items)
  const [rowIndex, setRowIndex] = createSignal(Math.min(state.row, Math.max(state.items.length - 1, 0)))
  const [fieldIndex, setFieldIndex] = createSignal(state.field)
  const [message, setMessage] = createSignal(state.message)
  return { items, setItems, rowIndex, setRowIndex, fieldIndex, setFieldIndex, message, setMessage }
}

function useListEditorKeys(props: {
  api: TuiPluginApi
  itemCount: Accessor<number>
  fieldCount: number
  setRowIndex: Setter<number>
  setFieldIndex: Setter<number>
  close: () => void
  add: () => void
  remove: () => void
  edit: () => void
  move?: (delta: number) => void
}) {
  useKeyIntercept(props.api, (key) => {
    if (key.name === "escape") {
      props.close()
      return true
    }
    if (key.name === "a") {
      props.add()
      return true
    }
    if (key.name === "d") {
      props.remove()
      return true
    }
    if (props.move && key.shift && (key.name === "up" || key.name === "k")) {
      props.move(-1)
      return true
    }
    if (props.move && key.shift && (key.name === "down" || key.name === "j")) {
      props.move(1)
      return true
    }
    if (key.name === "up" || key.name === "k") {
      props.setRowIndex((i) => Math.max(i - 1, 0))
      return true
    }
    if (key.name === "down" || key.name === "j") {
      props.setRowIndex((i) => Math.min(i + 1, props.itemCount() - 1))
      return true
    }
    if (key.name === "left" || key.name === "h") {
      props.setFieldIndex((i) => (i - 1 + props.fieldCount) % props.fieldCount)
      return true
    }
    if (key.name === "right" || key.name === "l") {
      props.setFieldIndex((i) => (i + 1) % props.fieldCount)
      return true
    }
    if (key.name === "return") {
      props.edit()
      return true
    }
    return false
  })
}

function deleteItem<T>(ctx: EditorContext<T>) {
  const index = ctx.selectedRow()
  const current = ctx.items()
  if (index < 0 || index >= current.length) return
  const next = [...current]
  next.splice(index, 1)
  ctx.setItems(next)
  ctx.setRowIndex(Math.min(index, Math.max(next.length - 1, 0)))
}

function appendItem<T>(ctx: EditorContext<T>, item: T) {
  const next = [...ctx.items(), item]
  ctx.setItems(next)
  ctx.setRowIndex(next.length - 1)
  ctx.setFieldIndex(0)
  ctx.setMessage(undefined)
  ctx.reopenWithSnapshot()
}

function moveItem<T>(ctx: EditorContext<T>, delta: number) {
  const index = ctx.selectedRow()
  const current = ctx.items()
  const target = index + delta
  if (target < 0 || target >= current.length) return
  const next = [...current]
  const temp = next[index]
  const replacement = next[target]
  if (!temp || !replacement) return
  next[index] = replacement
  next[target] = temp
  ctx.setItems(next)
  ctx.setRowIndex(target)
}

function useListEditor<T>(props: {
  api: TuiPluginApi
  state: ListEditorState<T>
  fields: readonly string[]
  reopen: () => void
  onChange: (items: T[]) => void
  add: (ctx: EditorContext<T>) => void
  edit: (ctx: EditorContext<T>) => void
  move?: (ctx: EditorContext<T>, delta: number) => void
}) {
  const { items, setItems, rowIndex, setRowIndex, fieldIndex, setFieldIndex, message, setMessage } = listEditorSignals(
    props.state,
  )
  const fieldCount = props.fields.length
  const selectedRow = () => Math.min(rowIndex(), Math.max(items().length - 1, 0))
  const focusedField = () => props.fields[fieldIndex() % fieldCount] ?? props.fields[0] ?? ""

  const { reopenWithSnapshot, prompt } = listEditorDialogControls({
    api: props.api,
    state: props.state,
    items,
    selectedRow,
    fieldIndex,
    message,
    reopen: props.reopen,
  })

  const ctx: EditorContext<T> = {
    items,
    setItems,
    selectedRow,
    setRowIndex,
    setFieldIndex,
    focusedField,
    setMessage,
    reopenWithSnapshot,
    prompt,
  }

  const move = props.move
  useListEditorKeys({
    api: props.api,
    itemCount: () => items().length,
    fieldCount,
    setRowIndex,
    setFieldIndex,
    close: () => {
      props.api.ui.dialog.clear()
      props.onChange(items())
    },
    add: () => props.add(ctx),
    remove: () => deleteItem(ctx),
    edit: () => props.edit(ctx),
    move: move ? (delta) => move(ctx, delta) : undefined,
  })

  return { items, message, selectedRow, focusedField }
}

function fieldColor(theme: TuiThemeCurrent, selected: boolean, focused: boolean) {
  return selected && focused ? theme.text : selected ? theme.selectedListItemText : theme.text
}

function ListEditorContents<T>(props: {
  api: TuiPluginApi
  theme: Accessor<TuiThemeCurrent>
  items: Accessor<T[]>
  selectedRow: Accessor<number>
  focusedField: Accessor<string>
  columns: ListEditorColumn<T>[]
  empty: string
  message: Accessor<string | undefined>
  reorder?: boolean
}) {
  const theme = props.theme
  return (
    <>
      <box flexDirection="column" gap={1}>
        <Show when={props.items().length > 0} fallback={<text fg={theme().textMuted}>{props.empty}</text>}>
          <For each={props.items()}>
            {(item, i) => {
              const selected = () => i() === props.selectedRow()
              return (
                <box flexDirection="row" gap={1} backgroundColor={selected() ? theme().primary : undefined}>
                  <For each={props.columns}>
                    {(column) => (
                      <text
                        width={column.width}
                        flexGrow={column.flexGrow}
                        wrapMode="none"
                        fg={fieldColor(theme(), selected(), props.focusedField() === column.field)}
                      >
                        {column.value(item)}
                      </text>
                    )}
                  </For>
                </box>
              )
            }}
          </For>
        </Show>
      </box>
      <Show when={props.message()}>
        <text fg={theme().error}>{props.message()}</text>
      </Show>
      <box paddingTop={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>edit</span>
        </text>
        <text fg={theme().text}>
          a <span style={{ fg: theme().textMuted }}>add</span>
        </text>
        <text fg={theme().text}>
          d <span style={{ fg: theme().textMuted }}>delete</span>
        </text>
        <Show when={props.reorder}>
          <text fg={theme().text}>
            shift+↑↓ <span style={{ fg: theme().textMuted }}>reorder</span>
          </text>
        </Show>
      </box>
    </>
  )
}

const COMMAND_FIELDS = ["name", "cwd", "command", "scope"] as const

type CommandField = (typeof COMMAND_FIELDS)[number]

const COMMAND_COLUMNS: ListEditorColumn<CommandSpec>[] = [
  { field: "name", width: 16, value: (command) => command.name },
  { field: "cwd", width: 14, value: (command) => command.cwd },
  { field: "command", flexGrow: 1, value: (command) => command.command },
  { field: "scope", width: 20, value: (command) => (command.scope ?? []).join(", ") },
]

function parseScope(text?: string): string[] | undefined {
  const trimmed = text?.trim()
  return trimmed
    ? trimmed
        .split(",")
        .map((pattern) => pattern.trim())
        .filter(Boolean)
    : undefined
}

function addCommand(ctx: EditorContext<CommandSpec>, kind: "setup" | "check") {
  const values: Partial<Record<CommandField, string>> = {}

  const ask = (fields: CommandField[]) => {
    if (fields.length === 0) {
      const name = values.name?.trim()
      const cwd = values.cwd?.trim()
      const command = values.command?.trim()
      if (!name || !command || cwd === undefined) {
        ctx.setMessage("Name, cwd, and command are required")
        ctx.reopenWithSnapshot()
        return
      }
      if (!cwd) {
        ctx.setMessage("cwd cannot be empty")
        ctx.reopenWithSnapshot()
        return
      }
      const parsed = commandSpec(
        { name, cwd, command, scope: parseScope(values.scope) },
        `${kind} ${ctx.items().length + 1}`,
      )
      if (!parsed) {
        ctx.setMessage("Invalid command: unsafe cwd or invalid scope regex")
        ctx.reopenWithSnapshot()
        return
      }
      appendItem(ctx, parsed)
      return
    }

    const [field, ...rest] = fields
    if (!field) return
    const title = field === "scope" ? "scope (comma-separated regexes)" : field
    ctx.prompt(title, values[field] ?? "", (next) => {
      values[field] = next
      ask(rest)
    })
  }

  ask(["name", "cwd", "command", "scope"])
}

function editCommandField(ctx: EditorContext<CommandSpec>) {
  const command = ctx.items()[ctx.selectedRow()]
  if (!command) return
  const field = ctx.focusedField() as CommandField
  const value = field === "scope" ? (command.scope ?? []).join(", ") : command[field]
  const title = field === "scope" ? "scope (comma-separated regexes)" : field
  ctx.prompt(title, value, (next) => {
    const updated = { ...command }
    if (field === "scope") {
      updated.scope = next
        ? next
            .split(",")
            .map((pattern) => pattern.trim())
            .filter(Boolean)
        : undefined
    } else {
      updated[field] = next.trim()
    }
    if (!updated.name || !updated.command || !updated.cwd) {
      ctx.setMessage("Name, cwd, and command are required")
      ctx.reopenWithSnapshot()
      return
    }
    const parsed = commandSpec(updated, command.name)
    if (!parsed) {
      ctx.setMessage("Invalid command: unsafe cwd or invalid scope regex")
      ctx.reopenWithSnapshot()
      return
    }
    const nextCommands = [...ctx.items()]
    nextCommands[ctx.selectedRow()] = parsed
    ctx.setItems(nextCommands)
    ctx.setMessage(undefined)
    ctx.reopenWithSnapshot()
  })
}

export function openCommandListEditor(
  api: TuiPluginApi,
  kind: "setup" | "check",
  initial: CommandSpec[],
  onChange: (commands: CommandSpec[]) => void,
) {
  const state: ListEditorState<CommandSpec> = { items: initial, row: 0, field: 0 }
  const reopen = () => {
    api.ui.dialog.replace(() => (
      <CommandListEditor api={api} kind={kind} state={state} onChange={onChange} reopen={reopen} />
    ))
  }
  reopen()
}

function CommandListEditor(props: {
  api: TuiPluginApi
  kind: "setup" | "check"
  state: ListEditorState<CommandSpec>
  onChange: (commands: CommandSpec[]) => void
  reopen: () => void
}) {
  const theme = () => props.api.theme.current
  const editor = useListEditor<CommandSpec>({
    api: props.api,
    state: props.state,
    fields: COMMAND_FIELDS,
    reopen: props.reopen,
    onChange: props.onChange,
    add: (ctx) => addCommand(ctx, props.kind),
    edit: editCommandField,
    move: moveItem,
  })

  return (
    <DialogFrame api={props.api} title={`${props.kind} commands`} closeLabel="esc close">
      <ListEditorContents
        api={props.api}
        theme={theme}
        items={editor.items}
        selectedRow={editor.selectedRow}
        focusedField={editor.focusedField}
        columns={COMMAND_COLUMNS}
        empty="No commands. Press a to add one."
        message={editor.message}
        reorder={true}
      />
    </DialogFrame>
  )
}

const VALIDATOR_MODEL_FIELDS = ["providerID", "modelID"] as const

type ValidatorModelField = (typeof VALIDATOR_MODEL_FIELDS)[number]

const VALIDATOR_MODEL_COLUMNS: ListEditorColumn<ModelRef>[] = [
  { field: "providerID", width: 24, value: (model) => model.providerID },
  { field: "modelID", flexGrow: 1, value: (model) => model.modelID },
]

function addModel(ctx: EditorContext<ModelRef>) {
  const values: Partial<Record<ValidatorModelField, string>> = {}

  const ask = (fields: ValidatorModelField[]) => {
    if (fields.length === 0) {
      const providerID = values.providerID?.trim()
      const modelID = values.modelID?.trim()
      if (!providerID || !modelID) {
        ctx.setMessage("providerID and modelID are required")
        ctx.reopenWithSnapshot()
        return
      }
      appendItem(ctx, { providerID, modelID })
      return
    }

    const [field, ...rest] = fields
    if (!field) return
    ctx.prompt(field, values[field] ?? "", (next) => {
      values[field] = next
      ask(rest)
    })
  }

  ask(["providerID", "modelID"])
}

function editModelField(ctx: EditorContext<ModelRef>) {
  const model = ctx.items()[ctx.selectedRow()]
  if (!model) return
  const field = ctx.focusedField() as ValidatorModelField
  ctx.prompt(field, model[field], (next) => {
    const updated = { ...model, [field]: next.trim() }
    const providerID = updated.providerID.trim()
    const modelID = updated.modelID.trim()
    if (!providerID || !modelID) {
      ctx.setMessage("providerID and modelID are required")
      ctx.reopenWithSnapshot()
      return
    }
    const nextModels = [...ctx.items()]
    nextModels[ctx.selectedRow()] = { providerID, modelID }
    ctx.setItems(nextModels)
    ctx.setMessage(undefined)
    ctx.reopenWithSnapshot()
  })
}

export function openValidatorModelListEditor(
  api: TuiPluginApi,
  initial: ModelRef[],
  onChange: (models: ModelRef[]) => void,
) {
  const state: ListEditorState<ModelRef> = { items: initial, row: 0, field: 0 }
  const reopen = () => {
    api.ui.dialog.replace(() => (
      <ValidatorModelListEditor api={api} state={state} onChange={onChange} reopen={reopen} />
    ))
  }
  reopen()
}

function ValidatorModelListEditor(props: {
  api: TuiPluginApi
  state: ListEditorState<ModelRef>
  onChange: (models: ModelRef[]) => void
  reopen: () => void
}) {
  const theme = () => props.api.theme.current
  const editor = useListEditor<ModelRef>({
    api: props.api,
    state: props.state,
    fields: VALIDATOR_MODEL_FIELDS,
    reopen: props.reopen,
    onChange: props.onChange,
    add: addModel,
    edit: editModelField,
  })

  return (
    <DialogFrame api={props.api} title="validator models" closeLabel="esc close">
      <ListEditorContents
        api={props.api}
        theme={theme}
        items={editor.items}
        selectedRow={editor.selectedRow}
        focusedField={editor.focusedField}
        columns={VALIDATOR_MODEL_COLUMNS}
        empty="No models. Press a to add one."
        message={editor.message}
      />
    </DialogFrame>
  )
}
