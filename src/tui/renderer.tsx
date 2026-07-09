import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { KeyInputContext, KeymapEvent } from "@opentui/keymap"
import { createSignal, onCleanup, onMount } from "solid-js"

type Dimensions = { width: number; height: number }

export function useRendererDimensions(api: TuiPluginApi) {
  const [dimensions, setDimensions] = createSignal<Dimensions>({
    width: api.renderer.width,
    height: api.renderer.height,
  })
  const onResize = (width: number, height: number) => setDimensions({ width, height })

  onMount(() => api.renderer.on("resize", onResize))
  onCleanup(() => api.renderer.off("resize", onResize))

  return dimensions
}

export function useKeyIntercept(api: TuiPluginApi, handler: (key: KeymapEvent) => boolean) {
  let dispose: (() => void) | undefined
  onMount(() => {
    dispose = api.keymap.intercept("key", (ctx: KeyInputContext) => {
      if (handler(ctx.event)) {
        ctx.consume({ preventDefault: true, stopPropagation: true })
      }
    })
  })
  onCleanup(() => dispose?.())
}
