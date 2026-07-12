import { canRestartHelper } from "../../../domain/task/policy"
import type { BoardSession } from "../../types"

type FooterHint = { key: string; label: string }

export function footerHints(selected: BoardSession | undefined, hasFilter: boolean): FooterHint[] {
  const hints: FooterHint[] = [
    { key: "j/k/h/l", label: "navigate" },
    { key: "enter", label: "menu" },
    { key: "n", label: "new" },
  ]
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
  hints.push({ key: ",", label: "settings" }, { key: "?", label: "help" }, { key: "q", label: "quit" })
  return hints
}

export type MenuAction = "view" | "open" | "advance" | "send_back" | "approve" | "retry" | "archive" | "delete"

type MenuOption = { title: string; value: MenuAction }

// Mirrors the direct shortcuts (o/m/s/a/r/d) in each title so the menu teaches the fast path;
// options with no dedicated key (view/archive) stay plain.
export function menuOptions(session: BoardSession): MenuOption[] {
  const status = session.kaganStatus
  const options: MenuOption[] = [
    { title: "View details", value: "view" },
    { title: "Open session — o", value: "open" },
  ]
  if (status !== "done") options.push({ title: "Advance — m", value: "advance" })
  if (status === "review") {
    options.push({ title: "Send back — s", value: "send_back" }, { title: "Approve — a", value: "approve" })
  }
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
