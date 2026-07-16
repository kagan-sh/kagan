/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { commandSpec } from "../../../../domain/task/commands"
import type { CommandSpec } from "../../../../domain/task/types"
import { DialogFrame } from "../../../dialogs/chrome"
import { ListEditorContents } from "./contents"
import { useListEditor } from "./hook"
import { appendItem, moveItem } from "./state"
import type { EditorContext, ListEditorColumn, ListEditorState } from "./types"

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

function finalizeCommand(
  ctx: EditorContext<CommandSpec>,
  values: Partial<Record<CommandField, string>>,
  fallbackName: string,
) {
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
  const parsed = commandSpec({ name, cwd, command, scope: parseScope(values.scope) }, fallbackName)
  if (!parsed) {
    ctx.setMessage("Invalid command: unsafe cwd or invalid scope regex")
    ctx.reopenWithSnapshot()
    return
  }
  appendItem(ctx, parsed)
}

function addCommand(ctx: EditorContext<CommandSpec>, kind: "setup" | "check") {
  const values: Partial<Record<CommandField, string>> = {}

  const ask = (fields: CommandField[]) => {
    if (fields.length === 0) {
      finalizeCommand(ctx, values, `${kind} ${ctx.items().length + 1}`)
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
