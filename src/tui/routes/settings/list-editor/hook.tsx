import { useKeyIntercept } from "../../../renderer"
import type { EditorContext, ListEditorKeyProps, UseListEditorProps } from "./state"
import { buildEditorContext, deleteItem, listEditorDialogControls, listEditorSignals } from "./state"

function handleListEditorActionKey(props: ListEditorKeyProps, key: { name: string; shift?: boolean }) {
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
  return false
}

function handleListEditorNavigationKey(props: ListEditorKeyProps, key: { name: string }) {
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
}

function useListEditorKeys(props: ListEditorKeyProps) {
  useKeyIntercept(props.api, (key) => {
    if (handleListEditorActionKey(props, key)) return true
    if (handleListEditorNavigationKey(props, key)) return true
    return false
  })
}

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
