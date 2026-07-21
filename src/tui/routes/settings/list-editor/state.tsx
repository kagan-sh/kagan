import type { TuiPluginApi, TuiThemeCurrent } from "@opencode-ai/plugin/tui"

type SignalRead<T> = () => T
type SignalWrite<T> = (value: T | ((previous: T) => T)) => void

export type ListEditorState<T> = {
  items: T[]
  row: number
  field: number
  message?: string
}

export type ListEditorColumn<T> = {
  field: string
  value: (item: T) => string
  width?: number
  flexGrow?: number
}

export type EditorContext<T> = {
  items: SignalRead<T[]>
  setItems: SignalWrite<T[]>
  selectedRow: SignalRead<number>
  setRowIndex: SignalWrite<number>
  setFieldIndex: SignalWrite<number>
  focusedField: SignalRead<string>
  setMessage: SignalWrite<string | undefined>
  reopenWithSnapshot: () => void
  prompt: (title: string, value: string, onConfirm: (value: string) => void) => void
}

export type ListEditorKeyProps = {
  api: TuiPluginApi
  itemCount: SignalRead<number>
  fieldCount: number
  setRowIndex: SignalWrite<number>
  setFieldIndex: SignalWrite<number>
  close: () => void
  add: () => void
  remove: () => void
  edit: () => void
  move?: (delta: number) => void
}

export type ListEditorContentsProps<T> = {
  api: TuiPluginApi
  theme: SignalRead<TuiThemeCurrent>
  items: SignalRead<T[]>
  selectedRow: SignalRead<number>
  focusedField: SignalRead<string>
  columns: ListEditorColumn<T>[]
  empty: string
  message: SignalRead<string | undefined>
  reorder?: boolean
}

export type UseListEditorProps<T> = {
  api: TuiPluginApi
  state: ListEditorState<T>
  fields: readonly string[]
  reopen: () => void
  onChange: (items: T[]) => void
  add: (ctx: EditorContext<T>) => void
  edit: (ctx: EditorContext<T>) => void
  move?: (ctx: EditorContext<T>, delta: number) => void
}

export function deleteItem<T>(ctx: EditorContext<T>) {
  const index = ctx.selectedRow()
  const current = ctx.items()
  if (index < 0 || index >= current.length) return
  const next = [...current]
  next.splice(index, 1)
  ctx.setItems(next)
  ctx.setRowIndex(Math.min(index, Math.max(next.length - 1, 0)))
}

export function appendItem<T>(ctx: EditorContext<T>, item: T) {
  const next = [...ctx.items(), item]
  ctx.setItems(next)
  ctx.setRowIndex(next.length - 1)
  ctx.setFieldIndex(0)
  ctx.setMessage(undefined)
  ctx.reopenWithSnapshot()
}

export function moveItem<T>(ctx: EditorContext<T>, delta: number) {
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
