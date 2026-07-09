import type { Session } from "@opencode-ai/sdk/v2"
import type { ColumnType } from "../domain/task/types"

export const ROUTE = "kagan"
export const SETTINGS_ROUTE = "kagan-settings"

export type BoardSession = Session & {
  kaganStatus: ColumnType
}

export type BoardCard = {
  session: BoardSession
  children: BoardSession[]
}
