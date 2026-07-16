import type { TuiToast } from "@opencode-ai/plugin/tui"
import { createSignal } from "solid-js"

type BoardNotice = TuiToast & { key: string }

function noticeDuration(toast: TuiToast): number {
  return toast.duration ?? (toast.variant === "error" ? 10000 : 5000)
}

const NOTICE_CAP = 3

// context: OpenCode mounts <Toast/> only on home/session routes, so board feedback renders through this Notice overlay state rather than api.ui.toast.
export function createNotices() {
  const [notices, setNotices] = createSignal<BoardNotice[]>([])
  const noticeTimers = new Map<string, ReturnType<typeof setTimeout>>()
  let noticeSeq = 0

  const clearNoticeTimer = (key: string) => {
    const timer = noticeTimers.get(key)
    if (timer) clearTimeout(timer)
    noticeTimers.delete(key)
  }

  const dismissNotice = (key: string) => {
    clearNoticeTimer(key)
    setNotices((current) => current.filter((notice) => notice.key !== key))
  }

  const notify = (toast: TuiToast) => {
    const key = `notice-${++noticeSeq}`
    setNotices((current) => {
      const next = [...current, { ...toast, key }]
      while (next.length > NOTICE_CAP) {
        const expired = next.shift()
        if (expired) clearNoticeTimer(expired.key)
      }
      return next
    })
    noticeTimers.set(
      key,
      setTimeout(() => dismissNotice(key), noticeDuration(toast)),
    )
  }

  const toastError = (message: string) => {
    notify({ variant: "error", title: "Kagan", message })
  }

  const runWithToast = async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await fn()
    } catch (error) {
      toastError(error instanceof Error ? error.message : String(error))
      return undefined
    }
  }

  return { notices, notify, toastError, runWithToast }
}
