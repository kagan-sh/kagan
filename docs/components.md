# Native components

`kg_ui::register_components(&mut ComponentRegistry)` registers Sketch,
ThreadTitle, ThreadStatus and Markdown.
The application builds one registry, registers the gpui-component catalog and
these components into it, and freezes it. `kg_plugins::init` accepts that
frozen catalog and retains it for each plugin runtime. The registry generates
their script declarations.

## Sketch

```js
import { Sketch, SketchScene } from "gpui-component";

// During View.init:
this.scene = SketchScene();
this.scene.rect("box", 0, 0, 100, 80, "ink", 2, 17, false);
this.scene.line("arrow", [[100, 80], [200, 120]], "end", "accent", 2, 3, false);
this.scene.text("label", 50, 20, ["Sketch"], 18, "middle", "ink", false);
this.scene.ink("pen", [[10, 100, 0.2], [30, 110, 0.8]], "ink", 8, false);

// During View.render:
return new Sketch("canvas", this.scene);
```

SketchScene is a retained GPUI entity. Edits replace a single shape key and
notify observers; rejected edits leave the entity unchanged. Keys preserve
paint order. The final boolean chooses a separate overlay layer, painted after
the scene with clean strokes. `remove(key, overlay)` removes a shape, and
`clear()` releases both layers. `view(x, y, scale)` pans and zooms;
`style("hand" | "clean")` changes scene strokes. Colors resolve from the active
Workbench palette: ink, accent, muted, working, needs and failed.

Coordinates must be finite and within ±1,000,000, and a rectangle's far
corner (`x + width`, `y + height`) must stay inside that range. Dimensions
cannot be negative. Width is 0.1–100, text size 1–1000, scale 0.01–100, pressure 0–1,
and seed an unsigned 32-bit integer. Each layer and path accepts at most
20,000 entries. Text accepts separate lines and start/middle/end anchors.
Pointer and keyboard handling belongs to the caller. Children can host a native
text editor.

Geometry is cached by component, layer and shape key. Panning reuses paths;
shape edits, zoom and stroke style changes rebuild affected paths. Removed
keys leave the cache. Hand strokes use seeded roughr; ink outlines use
perfect_freehand. Both versions are pinned in Cargo.lock.

## Thread components

ThreadTitle and ThreadStatus take a core thread id and observe that thread
entity. Missing ids fail explicitly. Status uses the shared Workbench colors,
a 3 px bar and a visible status word with an accessible name.

Markdown takes a stable id and text, and uses GPUI Kit's native Markdown view.
All components receive the active Workbench theme. Runtime creation is tested
to preserve its projected background, foreground and primary colors.

## Drag measurement

Run the real pointer-dispatch test with the native UI optimized:

```sh
cargo t --locked --release -p kg-ui sketch_drag_2000 --no-capture
```

The test mounts a script with 2,000 visible hand rectangles, dispatches 140
pressed-left mouse moves and draws each frame. Mount's draw, before the loop,
builds the 2,000 paths. Each loop frame rebuilds only the dragged rectangle
(2,000 initial path builds plus 140 edits). The first 20 frames are warm-up
and are excluded from the sample; the reported figures are the remaining 120
frames, and the log names the frame index of the maximum. Timing includes
pointer dispatch, typed entity mutation, cache preparation and GPUI
layout/prepaint/paint in the headless CPU test platform. It excludes GPU
presentation. Timing is reported, rather than enforced on differently loaded
CI machines.

The earlier release maximum of 17.673 ms was one of those 120 frames, not a
warm-up frame. Per-frame reruns do not pin it to frame 20 or to any other
fixed index, and each frame still rebuilds one path. On a quiet macOS arm64
release run the maximum was 11.871 ms at frame 50 (p50 8.000 ms, p95 8.938
ms). On a busy machine the same frames' wall clock jumps far above that draw
while the path-build count stays at one per frame: the thread was preempted.
The 17.673 ms sample is that kind of spike. The isolated unoptimized native
UI (dev profile, kg-ui opt-level 3 via CLI override) measured p50 24.442 ms
and p95 25.508 ms. Live GPU frame timing remains unmeasured.
