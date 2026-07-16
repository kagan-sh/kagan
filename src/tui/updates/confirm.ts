import type { TuiPluginApi } from "@opencode-ai/plugin/tui"

export function confirmUpdate(api: TuiPluginApi, current: string, target: string): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    api.ui.dialog.replace(() =>
      api.ui.DialogConfirm({
        title: "Update Kagan",
        message: `Update Kagan from v${current} to v${target}? Restart OpenCode after installation.`,
        onConfirm: () => {
          api.ui.dialog.clear()
          resolve(true)
        },
        onCancel: () => {
          api.ui.dialog.clear()
          resolve(false)
        },
      }),
    )
  })
}
