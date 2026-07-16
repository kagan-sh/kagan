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

/** Shared handle over an editor's live signals, passed to the module-level add/edit/delete/move handlers. */
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

export type ListEditorDialogProps<T> = {
  api: TuiPluginApi
  state: ListEditorState<T>
  items: SignalRead<T[]>
  selectedRow: SignalRead<number>
  fieldIndex: SignalRead<number>
  message: SignalRead<string | undefined>
  reopen: () => void
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
