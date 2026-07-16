import type { ListEditorKeyProps } from "./types"
import { useKeyIntercept } from "../../../renderer"

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

export function useListEditorKeys(props: ListEditorKeyProps) {
  useKeyIntercept(props.api, (key) => {
    if (handleListEditorActionKey(props, key)) return true
    if (handleListEditorNavigationKey(props, key)) return true
    return false
  })
}
