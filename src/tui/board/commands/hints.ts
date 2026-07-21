import { canRestartHelper } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import type { BoardSession } from "../../types"

type FooterHint = { key: string; label: string }

export function footerHints(
  selected: BoardSession | undefined,
  hasFilter: boolean,
  waitingPermissions = 0,
  updateAvailable = false,
): FooterHint[] {
  const hints: FooterHint[] = [
    { key: "j/k/tab", label: "navigate" },
    { key: "enter", label: "menu" },
    { key: "n", label: "new" },
  ]
  if (waitingPermissions > 0)
    hints.push({ key: "p", label: waitingPermissions > 1 ? `${waitingPermissions} permissions` : "permission" })
  if (selected) {
    if (selected.kaganStatus === "review") {
      hints.push({ key: "a", label: "approve" }, { key: "s", label: "send back" })
    }
    const restartable = canRestartHelper(selected.kaganStatus, selected.metadata)
    if (restartable) {
      hints.push({
        key: "r",
        label: selected.kaganStatus === "backlog" ? "restart intake" : "restart review",
      })
    }
  }
  hints.push({ key: "/", label: "filter" })
  if (hasFilter) hints.push({ key: "esc", label: "clears it" })
  if (updateAvailable) hints.push({ key: "u", label: "update" })
  hints.push({ key: ",", label: "settings" }, { key: "?", label: "help" }, { key: "q", label: "quit" })
  return hints
}

export type MenuAction =
  | "view"
  | "intake"
  | "open"
  | "advance"
  | "send_back"
  | "approve"
  | "retry"
  | "archive"
  | "delete"

type MenuOption = { title: string; value: MenuAction }

// Review leads with approve/send-back; other options mirror o/m/r/d shortcuts where they exist.
export function menuOptions(session: BoardSession): MenuOption[] {
  const status = session.kaganStatus
  const options: MenuOption[] = []
  if (status === "review") {
    options.push({ title: "Approve — a", value: "approve" }, { title: "Send back — s", value: "send_back" })
  }
  options.push({ title: "View details", value: "view" })
  if (kagan(session.metadata).intake) options.push({ title: "View intake notes", value: "intake" })
  options.push({ title: "Open session — o", value: "open" })
  if (status !== "done") options.push({ title: "Advance — m", value: "advance" })
  const restartable = canRestartHelper(status, session.metadata)
  if (restartable) {
    options.push({
      title: status === "backlog" ? "Restart intake — r" : "Restart review — r",
      value: "retry",
    })
  }
  if (status === "done") options.push({ title: "Archive", value: "archive" })
  options.push({ title: "Delete — d", value: "delete" })
  return options
}
