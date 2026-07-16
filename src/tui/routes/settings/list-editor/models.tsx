/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { ModelRef } from "../../../../domain/task/types"
import { DialogFrame } from "../../../dialogs/chrome"
import { ListEditorContents } from "./contents"
import { useListEditor } from "./hook"
import { appendItem } from "./state"
import type { EditorContext, ListEditorColumn, ListEditorState } from "./types"

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
