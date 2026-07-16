/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { CommandSpec } from "../../../../domain/task/types"
import { DialogFrame } from "../../../dialogs/chrome"
import { addCommand, COMMAND_FIELDS, editCommandField } from "./command-edit"
import { ListEditorContents } from "./contents"
import { useListEditor } from "./hook"
import { moveItem } from "./state"
import type { ListEditorColumn, ListEditorState } from "./types"

const COMMAND_COLUMNS: ListEditorColumn<CommandSpec>[] = [
  { field: "name", width: 16, value: (command) => command.name },
  { field: "cwd", width: 14, value: (command) => command.cwd },
  { field: "command", flexGrow: 1, value: (command) => command.command },
  { field: "scope", width: 20, value: (command) => (command.scope ?? []).join(", ") },
]

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
