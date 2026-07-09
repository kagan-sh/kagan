export type ColumnType = "backlog" | "in_progress" | "review" | "done"
export const COLUMNS: readonly ColumnType[] = ["backlog", "in_progress", "review", "done"]
export const DEFAULT_IN_PROGRESS_CAP = 2
export type ModelRef = { providerID: string; modelID: string }
export type HelperRole = "intake" | "validator"
