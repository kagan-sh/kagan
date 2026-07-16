import type { EditorContext, UseListEditorProps } from "./types"
import { useListEditorKeys } from "./keys"
import { buildEditorContext, deleteItem, listEditorDialogControls, listEditorSignals } from "./state"

export function useListEditor<T>(props: UseListEditorProps<T>) {
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

  const ctx: EditorContext<T> = buildEditorContext({
    items,
    setItems,
    selectedRow,
    setRowIndex,
    setFieldIndex,
    focusedField,
    setMessage,
    reopenWithSnapshot,
    prompt,
  })

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
