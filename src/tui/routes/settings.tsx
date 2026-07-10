/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi, TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { type Accessor, type JSX, type ParentProps, type Setter, For, Show, createMemo, createSignal } from "solid-js"
import { readFile, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { OptionBoundsSchema } from "../../domain/options"
import { commandPlan, commandSpec } from "../../domain/task/commands"
import { helperRetries, inProgressCap, sendBackStopThreshold, squashMerge } from "../../domain/task/policy"
import type { ModelRef } from "../../domain/task/types"
import type { CommandSpec } from "../../domain/task/types"
import { SETTINGS_ROUTE, ROUTE } from "../types"
import { useKeyIntercept } from "../renderer"
import { DialogFrame } from "../dialogs/chrome"

type Section = "General" | "Agents" | "Commands" | "Validator models" | "JSON preview"

type Draft = {
  inProgressLimit: number
  helperRetries: number
  sendBackStopThreshold: number
  squashMerge: boolean
  intakeAgent: string
  validatorAgent: string
  validatorModels: ModelRef[]
  commands: {
    setup: CommandSpec[]
    check: CommandSpec[]
  }
}

const SECTIONS: Section[] = ["General", "Agents", "Commands", "Validator models", "JSON preview"]

function modelRef(value: unknown): ModelRef | undefined {
  if (typeof value !== "object" || value === null) return undefined
  const raw = value as Record<string, unknown>
  const providerID = typeof raw.providerID === "string" ? raw.providerID.trim() : ""
  const modelID = typeof raw.modelID === "string" ? raw.modelID.trim() : ""
  if (!providerID || !modelID) return undefined
  return { providerID, modelID }
}

function draftFromOptions(options?: Record<string, unknown>): Draft {
  return {
    inProgressLimit: inProgressCap(options),
    helperRetries: helperRetries(options),
    sendBackStopThreshold: sendBackStopThreshold(options),
    squashMerge: squashMerge(options),
    intakeAgent: typeof options?.intakeAgent === "string" ? options.intakeAgent : "",
    validatorAgent: typeof options?.validatorAgent === "string" ? options.validatorAgent : "",
    validatorModels: Array.isArray(options?.validatorModels)
      ? options.validatorModels.map(modelRef).filter((model): model is ModelRef => model !== undefined)
      : [],
    commands: {
      setup: commandPlan(options, "setup"),
      check: commandPlan(options, "check"),
    },
  }
}

function optionsFromDraft(draft: Draft): Record<string, unknown> {
  const options: Record<string, unknown> = {
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
    squashMerge: draft.squashMerge,
  }
  if (draft.intakeAgent.trim()) options.intakeAgent = draft.intakeAgent.trim()
  if (draft.validatorAgent.trim()) options.validatorAgent = draft.validatorAgent.trim()
  if (draft.validatorModels.length > 0) options.validatorModels = draft.validatorModels
  if (draft.commands.setup.length > 0 || draft.commands.check.length > 0) {
    options.commands = draft.commands
  }
  return options
}

function pluginOptionsJson(draft: Draft): string {
  return JSON.stringify(optionsFromDraft(draft), null, 2)
}

function validateValidatorModels(value: ModelRef[]): string | undefined {
  for (let i = 0; i < value.length; i++) {
    const item = value[i]
    if (item === undefined) continue
    if (!item.providerID.trim() || !item.modelID.trim()) {
      return `validatorModels[${i}] must be { providerID: string, modelID: string }`
    }
  }
  return undefined
}

function validateDraft(draft: Draft): string | undefined {
  const modelError = validateValidatorModels(draft.validatorModels)
  if (modelError) return modelError
  const bounds = OptionBoundsSchema.safeParse({
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
  })
  if (bounds.success) return undefined
  const field = bounds.error.issues[0]?.path[0]
  if (field === "inProgressLimit") return "inProgressLimit must be at least 1"
  if (field === "helperRetries") return "helperRetries must be at least 0"
  if (field === "sendBackStopThreshold") return "sendBackStopThreshold must be at least 1"
  return bounds.error.issues[0]?.message ?? "Invalid settings"
}

async function saveOptions(worktree: string, draft: Draft): Promise<string> {
  const error = validateDraft(draft)
  if (error) throw new Error(error)
  const path = join(worktree, "opencode.json")
  let raw: string
  try {
    raw = await readFile(path, "utf8")
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && (error as { code: string }).code === "ENOENT") {
      throw new Error("opencode.json not found in project root")
    }
    throw error
  }
  const config = JSON.parse(raw) as { plugin?: unknown }
  if (!Array.isArray(config.plugin)) throw new Error("opencode.json has no plugin array")
  const index = config.plugin.findIndex((entry) => {
    const path = Array.isArray(entry) ? entry[0] : entry
    return typeof path === "string" && path.includes("kagan")
  })
  if (index === -1) throw new Error("opencode.json has no Kagan plugin entry")
  const entry = config.plugin[index]
  const pathValue = Array.isArray(entry) ? entry[0] : entry
  config.plugin[index] = [pathValue, optionsFromDraft(draft)]
  await writeFile(path, `${JSON.stringify(config, null, 2)}\n`)
  return "Saved opencode.json. Restart OpenCode or reopen the project to apply changes."
}

type Row = { label: string; value: string; edit?: () => void }

const COMMAND_FIELDS = ["name", "cwd", "command", "scope"] as const

type CommandField = (typeof COMMAND_FIELDS)[number]

const COMMAND_COLUMNS: ListEditorColumn<CommandSpec>[] = [
  { field: "name", width: 16, value: (command) => command.name },
  { field: "cwd", width: 14, value: (command) => command.cwd },
  { field: "command", flexGrow: 1, value: (command) => command.command },
  { field: "scope", width: 20, value: (command) => (command.scope ?? []).join(", ") },
]

type ListEditorState<T> = {
  items: T[]
  row: number
  field: number
  message?: string
}

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

function listEditorFieldColor(theme: TuiThemeCurrent, selected: boolean, focused: boolean) {
  return selected && focused ? theme.text : selected ? theme.selectedListItemText : theme.text
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

function ListEditorHints(props: { api: TuiPluginApi; reorder?: boolean }) {
  const theme = () => props.api.theme.current
  return (
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
  )
}

function ListEditorCell(props: ParentProps<{ fg: TuiThemeCurrent["text"]; width?: number; flexGrow?: number }>) {
  return (
    <text width={props.width} flexGrow={props.flexGrow} wrapMode="none" fg={props.fg}>
      {props.children}
    </text>
  )
}

function ListEditorRow(props: ParentProps<{ theme: TuiThemeCurrent; selected: Accessor<boolean> }>) {
  return (
    <box flexDirection="row" gap={1} backgroundColor={props.selected() ? props.theme.primary : undefined}>
      {props.children}
    </box>
  )
}

type ListEditorColumn<T> = {
  field: string
  value: (item: T) => string
  width?: number
  flexGrow?: number
}

function ListEditorRows<T>(props: {
  items: Accessor<T[]>
  selectedRow: Accessor<number>
  focusedField: Accessor<string>
  theme: Accessor<TuiThemeCurrent>
  columns: ListEditorColumn<T>[]
}) {
  return (
    <For each={props.items()}>
      {(item, i) => {
        const selected = () => i() === props.selectedRow()
        const fieldFg = (field: string) =>
          listEditorFieldColor(props.theme(), selected(), props.focusedField() === field)
        return (
          <ListEditorRow theme={props.theme()} selected={selected}>
            <For each={props.columns}>
              {(column) => (
                <ListEditorCell width={column.width} flexGrow={column.flexGrow} fg={fieldFg(column.field)}>
                  {column.value(item)}
                </ListEditorCell>
              )}
            </For>
          </ListEditorRow>
        )
      }}
    </For>
  )
}

function renderListEditorRows<T>(
  items: Accessor<T[]>,
  selectedRow: Accessor<number>,
  focusedField: Accessor<string>,
  theme: Accessor<TuiThemeCurrent>,
  columns: ListEditorColumn<T>[],
): JSX.Element {
  return (
    <ListEditorRows
      items={items}
      selectedRow={selectedRow}
      focusedField={focusedField}
      theme={theme}
      columns={columns}
    />
  )
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
  return (
    <>
      <box flexDirection="column" gap={1}>
        <Show when={props.items().length > 0} fallback={<text fg={props.theme().textMuted}>{props.empty}</text>}>
          {renderListEditorRows(props.items, props.selectedRow, props.focusedField, props.theme, props.columns)}
        </Show>
      </box>
      <Show when={props.message()}>
        <text fg={props.theme().error}>{props.message()}</text>
      </Show>
      <ListEditorHints api={props.api} reorder={props.reorder} />
    </>
  )
}

function openCommandListEditor(
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
  const {
    items: commands,
    setItems: setCommands,
    rowIndex,
    setRowIndex,
    fieldIndex,
    setFieldIndex,
    message,
    setMessage,
  } = listEditorSignals(props.state)

  const fieldCount = COMMAND_FIELDS.length
  const selectedRow = () => Math.min(rowIndex(), Math.max(commands().length - 1, 0))
  const focusedField = () => COMMAND_FIELDS[fieldIndex() % fieldCount] ?? "name"

  const { reopenWithSnapshot, prompt } = listEditorDialogControls({
    api: props.api,
    state: props.state,
    items: commands,
    selectedRow,
    fieldIndex,
    message,
    reopen: props.reopen,
  })

  const addCommand = () => {
    const values: Partial<Record<CommandField, string>> = {}

    const ask = (fields: CommandField[]) => {
      if (fields.length === 0) {
        const name = values.name?.trim()
        const cwd = values.cwd?.trim()
        const command = values.command?.trim()
        const scopeText = values.scope?.trim()
        if (!name || !command || cwd === undefined) {
          setMessage("Name, cwd, and command are required")
          reopenWithSnapshot()
          return
        }
        if (!cwd) {
          setMessage("cwd cannot be empty")
          reopenWithSnapshot()
          return
        }
        const parsed = commandSpec(
          {
            name,
            cwd,
            command,
            scope: scopeText
              ? scopeText
                  .split(",")
                  .map((pattern) => pattern.trim())
                  .filter(Boolean)
              : undefined,
          },
          `${props.kind} ${commands().length + 1}`,
        )
        if (!parsed) {
          setMessage("Invalid command: unsafe cwd or invalid scope regex")
          reopenWithSnapshot()
          return
        }
        const next = [...commands(), parsed]
        setCommands(next)
        setRowIndex(next.length - 1)
        setFieldIndex(0)
        setMessage(undefined)
        reopenWithSnapshot()
        return
      }

      const field = fields[0]!
      const rest = fields.slice(1)
      const title = field === "scope" ? "scope (comma-separated regexes)" : field
      prompt(title, values[field] ?? "", (next) => {
        values[field] = next
        ask(rest)
      })
    }

    ask(["name", "cwd", "command", "scope"])
  }

  const editField = () => {
    const command = commands()[selectedRow()]
    if (!command) return
    const field = focusedField()
    const value = field === "scope" ? (command.scope ?? []).join(", ") : command[field]
    const title = field === "scope" ? "scope (comma-separated regexes)" : field
    prompt(title, value, (next) => {
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
      const name = updated.name
      const cwd = updated.cwd
      const commandText = updated.command
      if (!name || !commandText || !cwd) {
        setMessage("Name, cwd, and command are required")
        reopenWithSnapshot()
        return
      }
      const parsed = commandSpec(updated, command.name)
      if (!parsed) {
        setMessage("Invalid command: unsafe cwd or invalid scope regex")
        reopenWithSnapshot()
        return
      }
      const nextCommands = [...commands()]
      nextCommands[selectedRow()] = parsed
      setCommands(nextCommands)
      setMessage(undefined)
      reopenWithSnapshot()
    })
  }

  const deleteCommand = () => {
    const index = selectedRow()
    const current = commands()
    if (index < 0 || index >= current.length) return
    const next = [...current]
    next.splice(index, 1)
    setCommands(next)
    setRowIndex(Math.min(index, Math.max(next.length - 1, 0)))
  }

  const moveCommand = (delta: number) => {
    const index = selectedRow()
    const current = commands()
    const target = index + delta
    if (target < 0 || target >= current.length) return
    const next = [...current]
    const temp = next[index]
    next[index] = next[target]!
    next[target] = temp!
    setCommands(next)
    setRowIndex(target)
  }

  useListEditorKeys({
    api: props.api,
    itemCount: () => commands().length,
    fieldCount,
    setRowIndex,
    setFieldIndex,
    close: () => {
      props.api.ui.dialog.clear()
      props.onChange(commands())
    },
    add: addCommand,
    remove: deleteCommand,
    edit: editField,
    move: moveCommand,
  })

  return (
    <DialogFrame api={props.api} title={`${props.kind} commands`} closeLabel="esc close">
      <ListEditorContents
        api={props.api}
        theme={theme}
        items={commands}
        selectedRow={selectedRow}
        focusedField={focusedField}
        columns={COMMAND_COLUMNS}
        empty="No commands. Press a to add one."
        message={message}
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

function openValidatorModelListEditor(api: TuiPluginApi, initial: ModelRef[], onChange: (models: ModelRef[]) => void) {
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
  const {
    items: models,
    setItems: setModels,
    rowIndex,
    setRowIndex,
    fieldIndex,
    setFieldIndex,
    message,
    setMessage,
  } = listEditorSignals(props.state)

  const fieldCount = VALIDATOR_MODEL_FIELDS.length
  const selectedRow = () => Math.min(rowIndex(), Math.max(models().length - 1, 0))
  const focusedField = () => VALIDATOR_MODEL_FIELDS[fieldIndex() % fieldCount] ?? "providerID"

  const { reopenWithSnapshot, prompt } = listEditorDialogControls({
    api: props.api,
    state: props.state,
    items: models,
    selectedRow,
    fieldIndex,
    message,
    reopen: props.reopen,
  })

  const addModel = () => {
    const values: Partial<Record<ValidatorModelField, string>> = {}

    const ask = (fields: ValidatorModelField[]) => {
      if (fields.length === 0) {
        const providerID = values.providerID?.trim()
        const modelID = values.modelID?.trim()
        if (!providerID || !modelID) {
          setMessage("providerID and modelID are required")
          reopenWithSnapshot()
          return
        }
        const next = [...models(), { providerID, modelID }]
        setModels(next)
        setRowIndex(next.length - 1)
        setFieldIndex(0)
        setMessage(undefined)
        reopenWithSnapshot()
        return
      }

      const field = fields[0]!
      const rest = fields.slice(1)
      prompt(field, values[field] ?? "", (next) => {
        values[field] = next
        ask(rest)
      })
    }

    ask(["providerID", "modelID"])
  }

  const editField = () => {
    const model = models()[selectedRow()]
    if (!model) return
    const field = focusedField()
    prompt(field, model[field], (next) => {
      const updated = { ...model, [field]: next.trim() }
      const providerID = updated.providerID.trim()
      const modelID = updated.modelID.trim()
      if (!providerID || !modelID) {
        setMessage("providerID and modelID are required")
        reopenWithSnapshot()
        return
      }
      const nextModels = [...models()]
      nextModels[selectedRow()] = { providerID, modelID }
      setModels(nextModels)
      setMessage(undefined)
      reopenWithSnapshot()
    })
  }

  const deleteModel = () => {
    const index = selectedRow()
    const current = models()
    if (index < 0 || index >= current.length) return
    const next = [...current]
    next.splice(index, 1)
    setModels(next)
    setRowIndex(Math.min(index, Math.max(next.length - 1, 0)))
  }

  useListEditorKeys({
    api: props.api,
    itemCount: () => models().length,
    fieldCount,
    setRowIndex,
    setFieldIndex,
    close: () => {
      props.api.ui.dialog.clear()
      props.onChange(models())
    },
    add: addModel,
    remove: deleteModel,
    edit: editField,
  })

  return (
    <DialogFrame api={props.api} title="validator models" closeLabel="esc close">
      <ListEditorContents
        api={props.api}
        theme={theme}
        items={models}
        selectedRow={selectedRow}
        focusedField={focusedField}
        columns={VALIDATOR_MODEL_COLUMNS}
        empty="No models. Press a to add one."
        message={message}
      />
    </DialogFrame>
  )
}

function rowsFor(
  section: Section,
  draft: Draft,
  setDraft: (draft: Draft) => void,
  setMessage: (message: string | undefined) => void,
  api: TuiPluginApi,
): Row[] {
  const prompt = (title: string, value: string, onConfirm: (value: string) => void) => {
    api.ui.dialog.replace(() => (
      <api.ui.DialogPrompt
        title={title}
        value={value}
        placeholder={title}
        onConfirm={(next) => {
          api.ui.dialog.clear()
          onConfirm(next)
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  if (section === "General") {
    const number = (
      key: "inProgressLimit" | "helperRetries" | "sendBackStopThreshold",
      label: string,
      min: number,
    ) => ({
      label,
      value: String(draft[key]),
      edit: () =>
        prompt(label, String(draft[key]), (value) => {
          const parsed = Number(value)
          if (Number.isInteger(parsed) && parsed >= min) setDraft({ ...draft, [key]: parsed })
        }),
    })
    return [
      number("inProgressLimit", "inProgressLimit", 1),
      number("helperRetries", "helperRetries", 0),
      number("sendBackStopThreshold", "sendBackStopThreshold", 1),
      {
        label: "squashMerge",
        value: draft.squashMerge ? "yes" : "no",
        edit: () => setDraft({ ...draft, squashMerge: !draft.squashMerge }),
      },
    ]
  }
  if (section === "Agents") {
    return [
      {
        label: "intakeAgent",
        value: draft.intakeAgent || "session default",
        edit: () =>
          prompt("intakeAgent", draft.intakeAgent, (value) => setDraft({ ...draft, intakeAgent: value.trim() })),
      },
      {
        label: "validatorAgent",
        value: draft.validatorAgent || "session default",
        edit: () =>
          prompt("validatorAgent", draft.validatorAgent, (value) =>
            setDraft({ ...draft, validatorAgent: value.trim() }),
          ),
      },
    ]
  }
  if (section === "Commands") {
    return [
      {
        label: "setup",
        value: `${draft.commands.setup.length} command(s)`,
        edit: () =>
          openCommandListEditor(api, "setup", draft.commands.setup, (setup) =>
            setDraft({ ...draft, commands: { ...draft.commands, setup } }),
          ),
      },
      {
        label: "check",
        value: `${draft.commands.check.length} command(s)`,
        edit: () =>
          openCommandListEditor(api, "check", draft.commands.check, (check) =>
            setDraft({ ...draft, commands: { ...draft.commands, check } }),
          ),
      },
    ]
  }
  if (section === "Validator models") {
    return [
      {
        label: "validatorModels",
        value: `${draft.validatorModels.length} model(s)`,
        edit: () =>
          openValidatorModelListEditor(api, draft.validatorModels, (models) =>
            setDraft({ ...draft, validatorModels: models }),
          ),
      },
    ]
  }
  return [{ label: "plugin options", value: pluginOptionsJson(draft) }]
}

export function Settings(props: { api: TuiPluginApi; options?: Record<string, unknown> }) {
  const theme = () => props.api.theme.current
  const [draft, setDraft] = createSignal(draftFromOptions(props.options))
  const [sectionIndex, setSectionIndex] = createSignal(0)
  const [rowIndex, setRowIndex] = createSignal(0)
  const [message, setMessage] = createSignal<string>()
  const section = () => SECTIONS[sectionIndex()] ?? "General"
  const rows = createMemo(() => rowsFor(section(), draft(), setDraft, setMessage, props.api))
  const selectedRow = () => rows()[rowIndex()]

  useKeyIntercept(props.api, (key) => {
    if (props.api.route.current.name !== SETTINGS_ROUTE || props.api.ui.dialog.open) return false
    if (key.name === "escape" || key.name === "q") {
      props.api.route.navigate(ROUTE)
      return true
    }
    if (key.name === "tab" || key.name === "right") {
      setSectionIndex((index) => (index + 1) % SECTIONS.length)
      setRowIndex(0)
      return true
    }
    if (key.name === "left") {
      setSectionIndex((index) => (index + SECTIONS.length - 1) % SECTIONS.length)
      setRowIndex(0)
      return true
    }
    if (key.name === "down" || key.name === "j") {
      setRowIndex((index) => Math.min(index + 1, rows().length - 1))
      return true
    }
    if (key.name === "up" || key.name === "k") {
      setRowIndex((index) => Math.max(index - 1, 0))
      return true
    }
    if (key.name === "return" || key.name === "e") {
      try {
        selectedRow()?.edit?.()
      } catch (error) {
        setMessage(error instanceof Error ? error.message : String(error))
      }
      return true
    }
    if (key.name === "s") {
      void saveOptions(props.api.state.path.worktree, draft()).then(setMessage, (error) =>
        setMessage(error instanceof Error ? error.message : String(error)),
      )
      return true
    }
    return false
  })

  return (
    <box position="absolute" left={0} top={0} width="100%" height="100%" padding={1}>
      <box flexDirection="column" width="100%" height="100%" borderColor={theme().border}>
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().text} attributes={TextAttributes.BOLD}>
            Kagan settings
          </text>
          <text fg={theme().textMuted}>q/esc back</text>
        </box>
        <box flexDirection="row" flexGrow={1} minHeight={0} gap={2}>
          <box width={22} flexDirection="column" border={["right"]} borderColor={theme().border}>
            <For each={SECTIONS}>
              {(item, index) => <text fg={index() === sectionIndex() ? theme().primary : theme().text}>{item}</text>}
            </For>
          </box>
          <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
            <box flexDirection="column" gap={1}>
              <Show
                when={section() !== "JSON preview"}
                fallback={<text wrapMode="word">{pluginOptionsJson(draft())}</text>}
              >
                <For each={rows()}>
                  {(row, index) => (
                    <box
                      flexDirection="row"
                      gap={2}
                      backgroundColor={index() === rowIndex() ? theme().primary : undefined}
                    >
                      <text width={24} fg={index() === rowIndex() ? theme().selectedListItemText : theme().textMuted}>
                        {row.label}
                      </text>
                      <text fg={index() === rowIndex() ? theme().selectedListItemText : theme().text}>{row.value}</text>
                    </box>
                  )}
                </For>
              </Show>
            </box>
          </scrollbox>
        </box>
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().textMuted}>{message() ?? "enter/e edit   s save   tab switch section"}</text>
          <text fg={theme().textMuted}>opencode.json only</text>
        </box>
      </box>
    </box>
  )
}
