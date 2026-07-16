import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Accessor, Setter } from "solid-js"
import { useKeyIntercept } from "../../renderer"
import { SETTINGS_ROUTE, ROUTE } from "../../types"
import type { Draft } from "./draft"
import { SECTIONS, saveOptions } from "./draft"
import type { Row } from "./rows"

export function useSettingsKeys(props: {
  api: TuiPluginApi
  draft: Accessor<Draft>
  rows: Accessor<Row[]>
  sectionIndex: Accessor<number>
  setSectionIndex: Setter<number>
  rowIndex: Accessor<number>
  setRowIndex: Setter<number>
  setMessage: Setter<string | undefined>
}) {
  const selectedRow = () => props.rows()[props.rowIndex()]

  useKeyIntercept(props.api, (key) => {
    if (props.api.route.current.name !== SETTINGS_ROUTE || props.api.ui.dialog.open) return false
    if (key.name === "escape" || key.name === "q") {
      props.api.route.navigate(ROUTE)
      return true
    }
    if (key.name === "tab" || key.name === "right") {
      props.setSectionIndex((index) => (index + 1) % SECTIONS.length)
      props.setRowIndex(0)
      return true
    }
    if (key.name === "left") {
      props.setSectionIndex((index) => (index + SECTIONS.length - 1) % SECTIONS.length)
      props.setRowIndex(0)
      return true
    }
    if (key.name === "down" || key.name === "j") {
      props.setRowIndex((index) => Math.min(index + 1, props.rows().length - 1))
      return true
    }
    if (key.name === "up" || key.name === "k") {
      props.setRowIndex((index) => Math.max(index - 1, 0))
      return true
    }
    if (key.name === "return" || key.name === "e") {
      try {
        selectedRow()?.edit?.()
      } catch (error) {
        props.setMessage(error instanceof Error ? error.message : String(error))
      }
      return true
    }
    if (key.name === "s") {
      void saveOptions(props.api.state.path.worktree, props.draft()).then(props.setMessage, (error) =>
        props.setMessage(error instanceof Error ? error.message : String(error)),
      )
      return true
    }
    return false
  })
}
