import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { useKeyIntercept } from "../renderer"

export function useOnboardingKeys(
  api: TuiPluginApi,
  step: () => number,
  setStep: (value: number | ((current: number) => number)) => void,
  last: number,
  close: () => void,
  dismissForever: () => void,
): void {
  useKeyIntercept(api, (key) => {
    if (key.name === "left" || key.name === "h") {
      setStep((current) => Math.max(0, current - 1))
      return true
    }
    if (key.name === "right" || key.name === "l") {
      setStep((current) => Math.min(last, current + 1))
      return true
    }
    if (key.name === "return") {
      if (step() === last) close()
      else setStep((current) => current + 1)
      return true
    }
    if (key.name === "x") {
      dismissForever()
      return true
    }
    return false
  })
}
