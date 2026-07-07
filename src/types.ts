import type { Session } from "@opencode-ai/sdk/v2"

export const ROUTE = "kagan"
export const SETTINGS_ROUTE = "kagan-settings"

export type ColumnType = "backlog" | "in_progress" | "review" | "done"

export const COLUMNS: readonly ColumnType[] = ["backlog", "in_progress", "review", "done"]

export type BoardSession = Session & {
  kaganStatus: ColumnType
}

export type BoardCard = {
  session: BoardSession
  children: BoardSession[]
}

export const DEFAULT_IN_PROGRESS_CAP = 2
