/** @jsxImportSource @opentui/solid */
import { createSignal } from "solid-js"
import { useKeyIntercept } from "../../../renderer"
import type { EditorContext, ListEditorKeyProps, UseListEditorProps } from "./state"
import { deleteItem } from "./state"

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
  const [items, setItems] = createSignal(props.state.items)
  const [rowIndex, setRowIndex] = createSignal(Math.min(props.state.row, Math.max(props.state.items.length - 1, 0)))
  const [fieldIndex, setFieldIndex] = createSignal(props.state.field)
  const [message, setMessage] = createSignal(props.state.message)
  const fieldCount = props.fields.length
  const selectedRow = () => Math.min(rowIndex(), Math.max(items().length - 1, 0))
  const focusedField = () => props.fields[fieldIndex() % fieldCount] ?? props.fields[0] ?? ""

  const snapshot = () => ({
    items: items(),
    row: selectedRow(),
    field: fieldIndex(),
    message: message(),
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
