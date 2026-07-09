import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { ColumnType } from "../../domain/task/types"

const orderKey = (column: ColumnType) => `kagan:order:${column}`

export function getOrder(api: TuiPluginApi, column: ColumnType): string[] {
  return api.kv.get(orderKey(column), [])
}

export function setOrder(api: TuiPluginApi, column: ColumnType, order: readonly string[]): void {
  api.kv.set(orderKey(column), [...order])
}

const filterKey = "kagan:filter"

export function getFilter(api: TuiPluginApi): string {
  return api.kv.get(filterKey, "")
}

export function setFilter(api: TuiPluginApi, value: string): void {
  api.kv.set(filterKey, value)
}
