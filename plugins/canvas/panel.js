// @ts-check

import { h_flex, v_flex } from "gpui-base";
import { div, View } from "gpui-kit";
import { Input, InputState, KgButton, Sketch, SketchScene } from "gpui-component";
import { log, toast, context, onContextChanged } from "kagan/host";
import { clipCenter, copyElements, decodeClip, encodeClip, isBound, pasteOps, translated } from "./clip.js";
import { boxHeightForLabel } from "./layout.js";
import { boundsOf, syncScene } from "./scene.js";
import { arrowPoints, contentBounds, normalizeName } from "./model.js";
import { edit, list, nextChange, open, peek, projectCanvasName, remove, step } from "./store.js";

const COLORS = ["ink", "accent", "muted", "working", "needs", "failed"];

const TOOLS = [
  ["hand", "Hand — H", "icons/hand.svg"],
  ["select", "Selection — V or 1", "icons/mouse-pointer-2.svg"],
  ["rect", "Rectangle — R or 2", "icons/square.svg"],
  ["arrow", "Arrow — A or 5", "icons/arrow-right.svg"],
  ["line", "Line — L or 6", "icons/minus.svg"],
  ["pen", "Draw — P or 7", "icons/pencil.svg"],
  ["text", "Text — T or 8", "icons/type.svg"],
  ["erase", "Eraser — E or 0", "icons/eraser.svg"],
];

const HEADS = [
  ["none", "icons/minus.svg"],
  ["end", "icons/arrow-right.svg"],
  ["start", "icons/arrow-left.svg"],
  ["both", "icons/arrow-left-right.svg"],
];
const HANDLES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const BIND_SLOP = 14;
const MIN_BOX = 16;

export class CanvasPanel extends View {
  /** @param {import("gpui-shell").Props | undefined} _props @param {import("gpui-kit").AsyncContext} cx */
  init(_props, cx) {
    this.scene = SketchScene();
    this.scene.style("hand");
    this.scene.view(32, 32, 1);
    this.painted = new Set();
    this.sent = new Map();
    this.focus = cx.focus_handle();
    this.nameInput = InputState("Canvas name");
    this.noteInput = InputState("Note to the thread");
    this.labelInput = InputState("Label");
    this.tool = "select";
    this.color = "ink";
    this.sticky = false;
    this.viewX = 32;
    this.viewY = 32;
    this.scale = 1;
    /** @type {import("./model.js").Doc | null} */
    this.doc = null;
    this.name = "";
    this.names = /** @type {string[]} */ ([]);
    this.selected = /** @type {string[]} */ ([]);
    this.hidden = new Set();
    /** @type {{ ids: Set<string>, dx: number, dy: number } | null} */
    this.preview = null;
    /** @type {import("./model.js").ElementRecord | null} */
    this.wip = null;
    /** @type {null | { kind: string, x: number, y: number, moved?: boolean, points?: number[][], id?: string, handle?: string, ox?: number, oy?: number, ow?: number, oh?: number }} */
    this.gesture = null;
    this.pointer = { x: 80, y: 80 };
    this.space = false;
    this.pinned = false;
    this.projectId = "";
    this.projectName = "canvas";
    this.threadId = /** @type {string | undefined} */ (undefined);
    /** @type {{ w: number, h: number } | null} */
    this.viewport = null;
    this.menu = false;
    this.naming = false;
    this.editing = false;
    /** @type {{ id: string, x: number, y: number, w: number, h: number } | null} */
    this.resize = null;
    this.handleKeys = /** @type {string[]} */ ([]);
    this.alive = true;
    cx.spawn(async () => {
      await this.follow(context());
      cx.notify();
      let seen = currentGeneration();
      for (;;) {
        seen = await nextChange(seen);
        if (!this.alive || this.name === "") continue;
        const doc = (await peek(this.name)) ?? (await open(this.name));
        this.doc = doc;
        this.names = await list();
        seen = currentGeneration();
        this.sync();
        cx.notify();
      }
    });
    cx.spawn(async () => {
      for (;;) {
        const next = await onContextChanged();
        if (!this.alive) return;
        this.threadId = next.threadId;
        if (!this.pinned && next.projectId !== this.projectId) {
          await this.follow(next);
          cx.notify();
        }
      }
    });
  }

  /** @param {ReturnType<typeof context>} ctx */
  async follow(ctx) {
    this.projectId = ctx.projectId ?? "";
    this.threadId = ctx.threadId;
    this.projectName = await projectCanvasName(ctx.projectId);
    if (this.pinned) return;
    this.name = this.projectName;
    this.nameInput.set_value(this.name);
    this.doc = await open(this.name);
    this.names = await list();
    this.selected = [];
    this.sync();
  }

  sync() {
    if (this.doc === null) return;
    try {
      syncScene(this.scene, this.shown(), { x: this.viewX, y: this.viewY, scale: this.scale }, this.selected, this.wip, this.painted, this.sent);
      this.paintHandles();
    } catch (error) {
      log("error", error instanceof Error ? error.message : String(error));
    }
  }

  shown() {
    if (this.doc === null) return /** @type {import("./model.js").ElementRecord[]} */ ([]);
    let elements = this.doc.elements.filter((element) => {
      if (this.hidden.has(element.id ?? "")) return false;
      if (element.type === "arrow" && ((element.from && this.hidden.has(element.from)) || (element.to && this.hidden.has(element.to)))) return false;
      return true;
    });
    if (this.preview) {
      const preview = this.preview;
      elements = elements.map((element) => (preview.ids.has(element.id ?? "") && !isBound(element) ? translated(element, preview.dx, preview.dy) : element));
    }
    if (this.resize) {
      const resize = this.resize;
      elements = elements.map((element) => (element.id === resize.id && element.type === "box" ? { ...element, x: resize.x, y: resize.y, w: resize.w, h: resize.h } : element));
    }
    if (this.editing && this.selected.length === 1) {
      const value = this.labelInput.value();
      elements = elements.map((element) => {
        if (element.id !== this.selected[0]) return element;
        if (element.type === "box" || element.type === "arrow") return { ...element, label: value };
        if (element.type === "text" && value.trim() !== "") return { ...element, text: value };
        return element;
      });
    }
    return elements;
  }

  /**
   * @param {unknown[]} ops
   * @param {import("gpui-kit").Context} cx
   * @param {boolean} drawn
   */
  apply(ops, cx, drawn) {
    if (this.doc === null || this.name === "" || ops.length === 0) return;
    const name = this.name;
    cx.spawn(async (taskCx) => {
      try {
        const result = await edit(name, ops);
        if (!this.alive || this.name !== name) return;
        this.doc = result.doc;
        if (result.added.length > 0) this.selected = result.added;
        if (this.wantEdit) {
          this.wantEdit = false;
          this.editing = true;
        }
        if (drawn && !this.sticky) this.tool = "select";
        this.preview = null;
        this.wip = null;
        this.hidden = new Set();
        this.fillLabel();
        this.sync();
        taskCx.notify();
      } catch (error) {
        this.preview = null;
        this.wip = null;
        this.hidden = new Set();
        this.sync();
        const message = error instanceof Error ? error.message : String(error);
        log("error", message);
        toast(message);
        taskCx.notify();
      }
    });
  }

  fillLabel() {
    const element = this.only();
    if (element === undefined) {
      this.labelInput.set_value("");
      return;
    }
    if (element.type === "text") this.labelInput.set_value(element.text);
    else if (element.type === "box" || element.type === "arrow") this.labelInput.set_value(element.label ?? "");
    else this.labelInput.set_value("");
  }

  only() {
    if (this.doc === null || this.selected.length !== 1) return undefined;
    return this.doc.elements.find((element) => element.id === this.selected[0]);
  }

  /** @param {import("gpui-kit").Context} cx */
  commitLabel(cx) {
    const element = this.only();
    if (element === undefined || element.id === undefined) return;
    const value = this.labelInput.value();
    if (element.type === "text") {
      const text = value.trim();
      if (text === "" || text === element.text) return;
      this.apply([{ op: "update", id: element.id, patch: { text } }], cx, false);
      return;
    }
    if (element.type === "box" || element.type === "arrow") {
      if ((element.label ?? "") === value) return;
      const grown = element.type === "box" ? Math.max(element.h, boxHeightForLabel(element.w, value)) : element.h;
      const patch = element.type === "box" && grown > element.h ? { label: value, h: grown } : { label: value };
      this.apply([{ op: "update", id: element.id, patch }], cx, false);
    }
  }

  /** @param {{ x: number, y: number }} local */
  toWorld(local) {
    return { x: (local.x - this.viewX) / this.scale, y: (local.y - this.viewY) / this.scale };
  }

  /** @param {{ x: number, y: number }} local @param {number} next */
  zoomAt(local, next) {
    const world = this.toWorld(local);
    this.scale = Math.min(30, Math.max(0.1, next));
    this.viewX = local.x - world.x * this.scale;
    this.viewY = local.y - world.y * this.scale;
  }

  /** @param {import("gpui-kit").MouseMoveEvent | import("gpui-kit").MouseButtonEvent | import("gpui-kit").ScrollWheelEvent} event */
  remember(event) {
    if (event.bounds) this.viewport = { w: event.bounds.width, h: event.bounds.height };
  }

  /** @param {import("gpui-kit").MouseButtonEvent} event @param {import("gpui-kit").Context} cx */
  down(event, cx) {
    const local = event.local_position;
    if (local === undefined || this.doc === null) return;
    this.focus.focus();
    this.remember(event);
    const world = this.toWorld(local);
    this.pointer = world;
    if (event.click_count >= 2) {
      this.double(world, cx);
      return;
    }
    if (this.space || this.tool === "hand") {
      this.gesture = { kind: "pan", x: local.x, y: local.y };
      return;
    }
    if (this.tool === "select") {
      const handle = this.handleAt(world);
      if (handle) {
        this.gesture = { kind: "resize", x: world.x, y: world.y, ...handle };
        return;
      }
      const hit = hitTest(this.doc, world.x, world.y, this.scale);
      if (hit?.id === undefined) {
        if (!event.modifiers.shift) this.selected = [];
        this.gesture = { kind: "marquee", x: world.x, y: world.y };
        this.wip = { type: "box", id: "wip", x: world.x, y: world.y, w: 0, h: 0, color: "muted" };
      } else {
        if (event.modifiers.shift) {
          this.selected = this.selected.includes(hit.id) ? this.selected.filter((id) => id !== hit.id) : [...this.selected, hit.id];
        } else if (!this.selected.includes(hit.id)) this.selected = [hit.id];
        this.gesture = { kind: "move", x: world.x, y: world.y, moved: false };
        this.fillLabel();
      }
      this.sync();
      cx.notify();
      return;
    }
    if (this.tool === "erase") {
      this.gesture = { kind: "erase", x: world.x, y: world.y };
      this.hidden = new Set();
      this.eraseAt(world);
      this.sync();
      cx.notify();
      return;
    }
    if (this.tool === "text") {
      const hit = hitTest(this.doc, world.x, world.y, this.scale);
      if (hit && hit.type !== "path") {
        this.selected = [hit.id ?? ""];
        this.editing = true;
        this.fillLabel();
        this.sync();
        cx.notify();
        return;
      }
      this.wantEdit = true;
      this.apply([{ op: "add", element: { type: "text", x: world.x, y: world.y, text: "text", color: this.color } }], cx, true);
      return;
    }
    this.gesture = { kind: "draw", x: world.x, y: world.y, points: [[world.x, world.y]] };
  }

  /** @param {import("gpui-kit").MouseMoveEvent} event @param {import("gpui-kit").Context} cx */
  move(event, cx) {
    const local = event.local_position;
    if (local === undefined) return;
    this.remember(event);
    const world = this.toWorld(local);
    this.pointer = world;
    const gesture = this.gesture;
    if (gesture === null) return;
    if (gesture.kind === "pan") {
      this.viewX += local.x - gesture.x;
      this.viewY += local.y - gesture.y;
      gesture.x = local.x;
      gesture.y = local.y;
      this.sync();
      cx.notify();
      return;
    }
    if (gesture.kind === "move") {
      const dx = world.x - gesture.x;
      const dy = world.y - gesture.y;
      if (Math.hypot(dx, dy) > 2 / this.scale) gesture.moved = true;
      this.preview = { ids: new Set(this.selected), dx, dy };
      this.sync();
      return;
    }
    if (gesture.kind === "marquee") {
      const rect = boxBetween(gesture.x, gesture.y, world.x, world.y, false);
      this.wip = { type: "box", id: "wip", ...rect, color: "muted" };
      this.sync();
      return;
    }
    if (gesture.kind === "erase") {
      this.eraseAt(world);
      this.sync();
      return;
    }
    if (gesture.kind === "resize") {
      this.resize = resized(gesture, world);
      this.sync();
      return;
    }
    if (gesture.kind === "draw") this.previewDraw(gesture, world, event.modifiers.shift);
  }

  /** @param {import("gpui-kit").MouseButtonEvent} event @param {import("gpui-kit").Context} cx */
  up(event, cx) {
    const gesture = this.gesture;
    this.gesture = null;
    if (gesture === null || this.doc === null) return;
    const local = event.local_position;
    const world = local ? this.toWorld(local) : this.pointer;
    if (gesture.kind === "pan") return;
    if (gesture.kind === "move") {
      this.preview = null;
      if (gesture.moved) {
        const dx = world.x - gesture.x;
        const dy = world.y - gesture.y;
        const ops = this.selected
          .filter((id) => {
            const element = this.doc?.elements.find((item) => item.id === id);
            return element !== undefined && !isBound(element);
          })
          .map((id) => ({ op: "move", id, dx, dy }));
        if (ops.length > 0) this.apply(ops, cx, false);
        else {
          this.sync();
          cx.notify();
        }
      } else {
        this.sync();
        cx.notify();
      }
      return;
    }
    if (gesture.kind === "marquee") {
      const rect = boxBetween(gesture.x, gesture.y, world.x, world.y, false);
      this.selected = this.doc.elements.filter((element) => intersects(rect, boundsOf(element, this.doc?.elements ?? []))).map((element) => element.id ?? "");
      this.wip = null;
      this.fillLabel();
      this.sync();
      cx.notify();
      return;
    }
    if (gesture.kind === "resize") {
      const rect = resized(gesture, world);
      this.resize = null;
      this.apply([{ op: "update", id: gesture.id, patch: { x: rect.x, y: rect.y, w: rect.w, h: rect.h } }], cx, false);
      return;
    }
    if (gesture.kind === "erase") {
      const ops = deleteOps(this.doc.elements, [...this.hidden]);
      this.hidden = new Set();
      if (ops.length > 0) this.apply(ops, cx, false);
      else {
        this.sync();
        cx.notify();
      }
      return;
    }
    if (gesture.kind === "draw") {
      const ops = this.drawOps(gesture, world, event.modifiers.shift);
      this.wip = null;
      if (ops.length > 0) this.apply(ops, cx, true);
      else {
        this.sync();
        cx.notify();
      }
    }
  }

  /** @param {boolean} hovered @param {import("gpui-kit").Context} cx */
  hover(hovered, cx) {
    if (!hovered && this.gesture !== null) {
      this.up({ local_position: undefined, modifiers: { shift: false, control: false, alt: false, platform: false, function: false }, button: "left", click_count: 1, position: { x: 0, y: 0 } }, cx);
    }
  }

  /** @param {{ x: number, y: number }} world @param {import("gpui-kit").Context} cx */
  double(world, cx) {
    if (this.doc === null) return;
    const hit = hitTest(this.doc, world.x, world.y, this.scale);
    if (hit?.id === undefined) {
      this.apply([{ op: "add", element: { type: "text", x: world.x, y: world.y, text: "text", color: this.color } }], cx, true);
      return;
    }
    this.selected = [hit.id];
    this.tool = "select";
    this.editing = hit.type !== "path";
    this.fillLabel();
    this.sync();
    cx.notify();
  }

  /** @param {{ kind: string, x: number, y: number, points?: number[][] }} gesture @param {{ x: number, y: number }} world @param {boolean} shift */
  previewDraw(gesture, world, shift) {
    if (this.tool === "rect") {
      this.wip = { type: "box", id: "wip", ...boxBetween(gesture.x, gesture.y, world.x, world.y, shift), color: this.color };
    } else if (this.tool === "arrow" || this.tool === "line") {
      const end = shift ? snap(gesture.x, gesture.y, world.x, world.y) : world;
      const linked = this.linkedArrow(gesture.x, gesture.y, end.x, end.y);
      this.wip = {
        type: "arrow",
        id: "wip",
        x1: linked.x1,
        y1: linked.y1,
        x2: linked.x2,
        y2: linked.y2,
        ...(linked.from ? { from: linked.from } : {}),
        ...(linked.to ? { to: linked.to } : {}),
        heads: this.tool === "line" ? "none" : "end",
        color: this.color,
      };
    } else if (this.tool === "pen" && gesture.points) {
      const last = gesture.points[gesture.points.length - 1];
      if (gesture.points.length < 2000 && (!last || Math.hypot(world.x - last[0], world.y - last[1]) >= 1.5)) gesture.points.push([world.x, world.y]);
      const points = gesture.points.length < 2 ? [gesture.points[0], gesture.points[0]] : gesture.points;
      this.wip = { type: "path", id: "wip", points, color: this.color };
    }
    this.sync();
  }

  /**
   * @param {{ kind: string, x: number, y: number, points?: number[][] }} gesture
   * @param {{ x: number, y: number }} world
   * @param {boolean} shift
   */
  drawOps(gesture, world, shift) {
    if (this.doc === null) return [];
    if (this.tool === "rect") {
      const rect = boxBetween(gesture.x, gesture.y, world.x, world.y, shift);
      if (rect.w < 4 || rect.h < 4) return [];
      return [{ op: "add", element: { type: "box", ...rect, color: this.color } }];
    }
    if (this.tool === "arrow" || this.tool === "line") {
      const end = shift ? snap(gesture.x, gesture.y, world.x, world.y) : world;
      if (Math.hypot(end.x - gesture.x, end.y - gesture.y) < 4) return [];
      const heads = this.tool === "line" ? "none" : "end";
      const linked = this.linkedArrow(gesture.x, gesture.y, end.x, end.y);
      return [{ op: "add", element: { type: "arrow", x1: linked.x1, y1: linked.y1, x2: linked.x2, y2: linked.y2, ...(linked.from ? { from: linked.from } : {}), ...(linked.to ? { to: linked.to } : {}), heads, color: this.color } }];
    }
    if (this.tool === "pen" && gesture.points && gesture.points.length >= 2) {
      return [{ op: "add", element: { type: "path", points: gesture.points.slice(0, 2000), color: this.color } }];
    }
    return [];
  }

  /** @param {{ x: number, y: number }} world */
  eraseAt(world) {
    if (this.doc === null) return;
    const hit = hitTest(this.doc, world.x, world.y, this.scale);
    if (hit?.id) this.hidden.add(hit.id);
  }

  /** @param {import("gpui-kit").ScrollWheelEvent} event @param {import("gpui-kit").Context} cx */
  wheel(event, cx) {
    const local = event.local_position;
    if (local === undefined) return;
    this.remember(event);
    if (event.modifiers.platform || event.modifiers.control) {
      const dy = event.delta_lines?.y ?? event.delta.y / 40;
      this.zoomAt(local, this.scale * Math.exp(-dy * 0.35));
    } else {
      this.viewX += event.delta.x;
      this.viewY += event.delta.y;
    }
    this.sync();
    cx.notify();
    cx.stop_propagation();
  }

  /** @param {import("gpui-kit").KeyEvent} event @param {import("gpui-kit").Context} cx */
  key(event, cx) {
    if (this.doc === null) return;
    const chord = event.keystroke.replace(/^ctrl-/, "cmd-");
    if (chord === "cmd-z") return this.history("undo", cx);
    if (chord === "cmd-shift-z" || chord === "cmd-y") return this.history("redo", cx);
    if (chord === "cmd-a") {
      this.selected = this.doc.elements.map((element) => element.id ?? "").filter((id) => id !== "");
      this.sync();
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (chord === "cmd-c" || chord === "cmd-x") return this.copy(cx, chord === "cmd-x");
    if (chord === "cmd-v") return this.paste(cx);
    if (chord === "cmd-d") return this.duplicate(cx);
    if (chord === "cmd-0") {
      this.viewX = 32;
      this.viewY = 32;
      this.scale = 1;
      this.sync();
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (chord === "cmd-=" || chord === "cmd-+" || chord === "cmd-equal" || chord === "cmd-minus" || chord === "cmd--") {
      const local = { x: (this.viewport?.w ?? 400) / 2, y: (this.viewport?.h ?? 300) / 2 };
      this.zoomAt(local, this.scale * (chord.startsWith("cmd-minus") || chord === "cmd--" ? 0.9 : 1.1));
      this.sync();
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (event.modifiers.platform || event.modifiers.control || event.modifiers.alt) return;
    if (event.key === "space") {
      this.space = true;
      cx.stop_propagation();
      return;
    }
    if (event.key === "escape") {
      this.selected = [];
      this.wip = null;
      this.preview = null;
      this.resize = null;
      this.editing = false;
      this.gesture = null;
      this.tool = "select";
      this.sync();
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (event.key === "backspace" || event.key === "delete") {
      this.apply(deleteOps(this.doc.elements, this.selected), cx, false);
      this.selected = [];
      cx.stop_propagation();
      return;
    }
    if (event.key === "enter") {
      this.editing = true;
      this.fillLabel();
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (event.key === "1" && event.modifiers.shift) {
      this.fit(cx);
      cx.stop_propagation();
      return;
    }
    const nudge = event.modifiers.shift ? 20 : 4;
    if (event.key === "left" || event.key === "right" || event.key === "up" || event.key === "down") {
      const dx = event.key === "left" ? -nudge : event.key === "right" ? nudge : 0;
      const dy = event.key === "up" ? -nudge : event.key === "down" ? nudge : 0;
      const ops = this.selected
        .filter((id) => {
          const element = this.doc?.elements.find((item) => item.id === id);
          return element !== undefined && !isBound(element);
        })
        .map((id) => ({ op: "move", id, dx, dy }));
      this.apply(ops, cx, false);
      cx.stop_propagation();
      return;
    }
    if (event.is_held) return;
    const tool = toolFor(event.key);
    if (tool) {
      this.tool = tool;
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if (event.key === "q") {
      this.sticky = !this.sticky;
      cx.notify();
      cx.stop_propagation();
    }
  }

  /** @param {import("gpui-kit").KeyEvent} event */
  keyUp(event) {
    if (event.key === "space") this.space = false;
  }

  /** @param {"undo" | "redo"} direction @param {import("gpui-kit").Context} cx */
  history(direction, cx) {
    if (this.name === "") return;
    const name = this.name;
    cx.spawn(async (taskCx) => {
      try {
        const doc = await step(name, direction);
        if (!this.alive || this.name !== name) return;
        this.doc = doc;
        this.selected = [];
        this.sync();
        taskCx.notify();
      } catch (error) {
        toast(error instanceof Error ? error.message : String(error));
      }
    });
    cx.stop_propagation();
  }

  /** @param {import("gpui-kit").Context} cx @param {boolean} cut */
  copy(cx, cut) {
    if (this.doc === null || this.selected.length === 0) return;
    cx.write_to_clipboard(encodeClip(copyElements(this.selected, this.doc.elements)));
    if (cut) {
      this.apply(deleteOps(this.doc.elements, this.selected), cx, false);
      this.selected = [];
    }
    cx.stop_propagation();
  }

  /** @param {import("gpui-kit").Context} cx */
  paste(cx) {
    const text = cx.read_from_clipboard();
    if (text === undefined || this.doc === null) return;
    const clip = decodeClip(text);
    if (clip === null) {
      const body = text.trim().slice(0, 500);
      if (body !== "") this.apply([{ op: "add", element: { type: "text", x: this.pointer.x, y: this.pointer.y, text: body, color: this.color } }], cx, false);
      cx.stop_propagation();
      return;
    }
    const center = clipCenter(clip);
    this.apply(pasteOps(clip, this.pointer.x - center.x, this.pointer.y - center.y), cx, false);
    cx.stop_propagation();
  }

  /** @param {import("gpui-kit").Context} cx */
  duplicate(cx) {
    if (this.doc === null || this.selected.length === 0) return;
    this.apply(pasteOps(copyElements(this.selected, this.doc.elements), 24, 24), cx, false);
    cx.stop_propagation();
  }

  /** @param {import("gpui-kit").Context} cx */
  fit(cx) {
    if (this.doc === null || this.viewport === null) return;
    const bounds = contentBounds(this.doc);
    if (bounds === null) {
      this.viewX = 32;
      this.viewY = 32;
      this.scale = 1;
    } else {
      const pad = 32;
      const w = Math.max(bounds.w, 1);
      const h = Math.max(bounds.h, 1);
      this.scale = Math.min(30, Math.max(0.1, Math.min((this.viewport.w - pad * 2) / w, (this.viewport.h - pad * 2) / h)));
      this.viewX = pad - bounds.x * this.scale;
      this.viewY = pad - bounds.y * this.scale;
    }
    this.sync();
    cx.notify();
  }

  /** @param {import("gpui-kit").Context} cx */
  useName(cx) {
    const name = normalizeName(this.nameInput.value());
    if (name === "") return;
    this.pinned = true;
    this.name = name;
    this.selected = [];
    cx.spawn(async (taskCx) => {
      try {
        this.doc = await open(name);
        this.names = await list();
        this.sync();
        taskCx.notify();
      } catch (error) {
        log("error", error instanceof Error ? error.message : String(error));
      }
    });
  }

  /** @param {import("gpui-kit").Context} cx */
  makeCanvas(cx) {
    const known = new Set(this.names);
    let name = this.projectName || "canvas";
    let n = 2;
    while (known.has(name)) {
      name = `${this.projectName || "canvas"}-${n}`;
      n += 1;
    }
    this.nameInput.set_value(name);
    this.pinned = true;
    this.name = name;
    this.selected = [];
    open(name).catch((error) => log("error", String(error)));
    cx.notify();
  }

  /** @param {import("gpui-kit").Context} cx */
  deleteCanvas(cx) {
    const name = this.name;
    if (name === "") return;
    this.pinned = false;
    this.name = this.projectName || "canvas";
    this.nameInput.set_value(this.name);
    const next = this.name;
    cx.spawn(async (taskCx) => {
      try {
        await remove(name);
        this.doc = await open(next);
        this.names = await list();
        this.selected = [];
        this.sync();
        taskCx.notify();
      } catch (error) {
        log("error", error instanceof Error ? error.message : String(error));
      }
    });
  }

  /** @param {import("gpui-kit").Context} cx */
  sendNote(_cx) {
    toast("The drawing stays on the canvas.");
  }

  /**
   * @param {import("gpui-kit").Context} cx
   * @param {string} color
   */
  recolor(cx, color) {
    this.color = color;
    if (this.doc === null || this.selected.length === 0) {
      cx.notify();
      return;
    }
    this.apply(this.selected.map((id) => ({ op: "update", id, patch: { color } })), cx, false);
  }

  /**
   * @param {import("gpui-kit").Context} cx
   * @param {string} heads
   */
  setHeads(cx, heads) {
    if (this.doc === null) return;
    const ops = this.doc.elements
      .filter((element) => element.type === "arrow" && element.id !== undefined && this.selected.includes(element.id))
      .map((element) => ({ op: "update", id: element.id, patch: { heads } }));
    this.apply(ops, cx, false);
  }

  strokeColor() {
    if (this.doc === null || this.selected.length === 0) return this.color;
    const first = this.doc.elements.find((element) => element.id === this.selected[0]);
    return first?.color ?? "ink";
  }

  /** @param {import("gpui-kit").Context} cx */
  render(cx) {
    const colors = cx.theme().colors;
    const root = div().id("canvas-root").relative().size_full().overflow_hidden().bg(colors.background).text_color(colors.foreground)
      .on_key_down((event, ctx) => {
        if (event.key !== "escape") return;
        this.menu = false;
        this.naming = false;
        this.editing = false;
        this.focus.focus();
        ctx.notify();
      }).child(this.surface(colors));
    root.child(this.topBar(colors));
    if (!this.menu && this.showStyle()) root.child(this.stylePanel(colors));
    root.child(this.bottomBar(colors));
    if (this.menu) root.child(this.dismiss()).child(this.menuPanel(colors));
    return root;
  }

  showStyle() {
    return this.selected.length > 0 || this.tool === "rect" || this.tool === "arrow" || this.tool === "line" || this.tool === "pen" || this.tool === "text";
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  surface(colors) {
    const empty = this.doc !== null && this.doc.elements.length === 0 && this.wip === null && this.gesture === null;
    const layer = div()
      .id("canvas-surface")
      .absolute()
      .inset_0()
      .track_focus(this.focus)
      .key_context("Canvas")
      .on_mouse_down("left", (event, ctx) => this.down(event, ctx))
      .on_mouse_down("middle", (event, ctx) => {
        const local = event.local_position;
        if (local === undefined) return;
        this.gesture = { kind: "pan", x: local.x, y: local.y };
      })
      .on_mouse_up("left", (event, ctx) => this.up(event, ctx))
      .on_mouse_up("middle", (event, ctx) => this.up(event, ctx))
      .on_mouse_down_out((_event, ctx) => this.hover(false, ctx))
      .on_mouse_move((event, ctx) => this.move(event, ctx))
      .on_hover((hovered, ctx) => this.hover(hovered, ctx))
      .on_scroll_wheel((event, ctx) => this.wheel(event, ctx))
      .on_key_down((event, ctx) => this.key(event, ctx))
      .on_key_up((event) => this.keyUp(event));
    if (this.gesture?.kind === "pan" || this.space) layer.cursor_grabbing();
    else if (this.tool === "hand") layer.cursor_grab();
    else if (this.tool === "select") layer.cursor_default();
    else layer.cursor_crosshair();
    // The input sibling paints last so Sketch and the empty hint cannot mask its hitbox.
    const frame = div().absolute().inset_0().bg(colors.background).child(new Sketch("surface", this.scene).absolute().inset_0());
    if (empty) {
      frame.child(
        v_flex()
          .absolute()
          .inset_0()
          .items_center()
          .justify_center()
          .gap(8)
          .p(24)
          .text_color(colors.muted_foreground)
          .child(div().text_sm().child("Pick a tool and start drawing"))
          .child(div().text_xs().child("Agents draw here too. Send the canvas from below.")),
      );
    }
    return frame.child(layer);
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  topBar(colors) {
    return v_flex()
      .absolute()
      .top(12)
      .left(12)
      .right(12)
      .gap(8)
      .child(
        h_flex()
          .items_center()
          .gap(8)
          .child(this.square("canvas-menu", "icons/menu.svg", "Main menu", this.menu, colors, (ctx) => {
            this.menu = !this.menu;
            this.naming = false;
            ctx.notify();
          }))
          .child(div().flex_1().min_w(0).truncate().text_xs().text_color(colors.muted_foreground).child(this.name)),
      )
      .child(h_flex().w_full().justify_center().child(this.toolIsland(colors)));
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  toolIsland(colors) {
    return island(colors)
      .max_w_full()
      .flex_wrap()
      .justify_center()
      .p(4)
      .gap(2)
      .child(this.square("tool-keep", this.sticky ? "icons/lock.svg" : "icons/lock-open.svg", "Keep the tool after drawing — Q", this.sticky, colors, (ctx) => {
        this.sticky = !this.sticky;
        ctx.notify();
      }))
      .child(div().w(1).h(24).mx(4).bg(colors.border))
      .children(TOOLS.map(([tool, tip, icon]) => this.square(`tool-${tool}`, icon, tip, this.tool === tool, colors, (ctx) => {
        this.tool = tool;
        this.editing = false;
        this.menu = false;
        ctx.notify();
      })));
  }

  /**
   * @param {string} id
   * @param {string} mark
   * @param {string} tip
   * @param {boolean} active
   * @param {import("gpui-base").ColorTokens} colors
   * @param {(cx: import("gpui-kit").Context) => void} onClick
   */
  square(id, mark, tip, active, colors, onClick) {
    return new KgButton(id)
      .icon(mark)
      .label(tip)
      .size("small")
      .ghost()
      .selected(active)
      .on_click((_event, ctx) => onClick(ctx));
  }

  dismiss() {
    return div()
      .absolute()
      .inset_0()
      .on_mouse_down("left", (_event, ctx) => {
        this.menu = false;
        this.naming = false;
        ctx.notify();
        ctx.stop_propagation();
      });
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  menuPanel(colors) {
    const names = this.doc !== null && !this.names.includes(this.name) ? [this.name, ...this.names] : this.names;
    const panel = island(colors)
      .absolute()
      .top(104)
      .left(12)
      .w(240)
      .p(8)
      .flex_col()
      .gap(2)
      .child(div().px(8).py(4).text_xs().text_color(colors.muted_foreground).child("Canvases"));
    for (const name of names) {
      panel.child(this.menuRow(`canvas-pick-${name}`, name, name === this.name, (ctx) => {
        this.menu = false;
        this.nameInput.set_value(name);
        this.useName(ctx);
      }));
    }
    if (this.naming) {
      panel.child(
        h_flex()
          .gap(4)
          .p(4)
          .child(div().flex_1().child(new Input(this.nameInput)))
          .child(this.square("canvas-create", "icons/plus.svg", "Create this canvas", false, colors, (ctx) => this.useName(ctx))),
      );
    } else {
      panel.child(this.menuRow("canvas-new", "New canvas…", false, (ctx) => {
        this.naming = true;
        this.nameInput.set_value("");
        ctx.notify();
      }));
    }
    panel.child(div().h(1).my(4).bg(colors.border));
    panel.child(this.menuRow("canvas-fit", "Zoom to fit", false, (ctx) => {
      this.menu = false;
      this.fit(ctx);
    }));
    panel.child(this.menuRow("canvas-clear", "Reset the canvas", false, (ctx) => {
      this.menu = false;
      this.selected = [];
      this.apply([{ op: "clear" }], ctx, false);
    }));
    panel.child(this.menuRow("canvas-delete", "Delete this canvas", false, (ctx) => {
      this.menu = false;
      this.deleteCanvas(ctx);
    }));
    return panel;
  }

  /**
   * @param {string} id
   * @param {string} label
   * @param {boolean} current
   * @param {import("gpui-base").ColorTokens} colors
   * @param {(cx: import("gpui-kit").Context) => void} onClick
   */
  menuRow(id, label, current, onClick) {
    return v_flex().w_full().child(new KgButton(id)
      .label(label)
      .ghost()
      .selected(current)
      .size("small")
      .on_click((_event, ctx) => onClick(ctx)));
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  stylePanel(colors) {
    const current = this.strokeColor();
    const arrows = this.doc?.elements.filter((element) => element.type === "arrow" && element.id !== undefined && this.selected.includes(element.id)) ?? [];
    const panel = v_flex()
      .absolute()
      .top(104)
      .left(12)
      .w(202)
      .p(12)
      .gap(12)
      .bg(colors.background)
      .border(1)
      .border_color(colors.border)
      .rounded_lg()
      .shadow_md()
      .child(div().text_xs().child("Stroke"))
      .child(
        h_flex()
          .gap(4)
          .items_center()
          .flex_wrap()
          .children(COLORS.map((color) => new KgButton(`color-${color}`).label(color).ghost().selected(current === color).size("small").on_click((_event, ctx) => this.recolor(ctx, color)))),
      );
    if (arrows.length > 0) {
      panel.child(div().text_xs().child("Arrowheads")).child(
        h_flex().gap(4).children(HEADS.map(([heads, mark]) => this.square(`heads-${heads}`, mark, heads, arrows.every((arrow) => (arrow.heads ?? "end") === heads), colors, (ctx) => this.setHeads(ctx, heads)))),
      );
    }
    if (this.selected.length === 1) {
      const element = this.only();
      if (element && element.type !== "path") {
        panel.child(div().text_xs().child("Label")).child(new Input(this.labelInput).on_change(() => this.sync())).child(this.square("canvas-label", "icons/check.svg", "Set the label", false, colors, (ctx) => this.commitLabel(ctx)));
      }
      panel.child(div().text_xs().child("Actions")).child(
        h_flex()
          .gap(4)
          .child(this.square("canvas-duplicate", "icons/copy.svg", "Duplicate — Cmd+D", false, colors, (ctx) => this.duplicate(ctx)))
          .child(this.square("canvas-remove", "icons/trash.svg", "Delete", false, colors, (ctx) => {
            if (this.doc === null) return;
            this.apply(deleteOps(this.doc.elements, this.selected), ctx, false);
            this.selected = [];
          })),
      );
    }
    return panel;
  }

  /** @param {import("gpui-base").ColorTokens} colors */
  bottomBar(colors) {
    return h_flex()
      .absolute()
      .bottom(12)
      .left(12)
      .right(12)
      .items_end()
      .gap(8)
      .child(
        island(colors).child(this.square("canvas-zoom-out", "icons/zoom-out.svg", "Zoom out", false, colors, (ctx) => this.zoomBy(0.9, ctx))).child(
          new KgButton("canvas-zoom")
            .label(`${Math.round(this.scale * 100)}%`)
            .tooltip("Reset zoom")
            .size("small")
            .ghost()
            .on_click((_event, ctx) => {
              const local = { x: (this.viewport?.w ?? 400) / 2, y: (this.viewport?.h ?? 300) / 2 };
              this.zoomAt(local, 1);
              this.sync();
              ctx.notify();
            }),
        ).child(this.square("canvas-zoom-in", "icons/zoom-in.svg", "Zoom in", false, colors, (ctx) => this.zoomBy(1.1, ctx))),
      )
      .child(
        island(colors)
          .child(this.square("canvas-undo", "icons/undo-2.svg", "Undo", false, colors, (ctx) => this.history("undo", ctx)))
          .child(this.square("canvas-redo", "icons/redo-2.svg", "Redo", false, colors, (ctx) => this.history("redo", ctx))),
      )
      .child(
        island(colors)
          .flex_1()
          .min_w(0)
          .p(4)
          .gap(4)
          .child(div().flex_1().min_w(0).child(new Input(this.noteInput)))
          .child(this.square("canvas-send", "icons/send.svg", "Send to the thread", false, colors, (ctx) => this.sendNote(ctx))),
      );
  }

  paintHandles() {
    for (const key of this.handleKeys) {
      try {
        this.scene.remove(key, true);
      } catch {
        // The handle was never accepted.
      }
      this.painted.delete(`overlay:${key}`);
    }
    this.handleKeys = [];
    if (this.tool !== "select" || this.selected.length !== 1) return;
    const element = this.shown().find((item) => item.id === this.selected[0]);
    if (element === undefined || element.type !== "box") return;
    const side = 8 / this.scale;
    for (const handle of HANDLES) {
      const [hx, hy] = handlePoint(element, handle);
      const key = `handle:${handle}`;
      try {
        this.scene.rect(key, hx - side / 2, hy - side / 2, side, side, "accent", 1.2, 1, true);
        this.handleKeys.push(key);
        this.painted.add(`overlay:${key}`);
      } catch {
        // A handle past the world edge is skipped.
      }
    }
  }

  /** @param {{ x: number, y: number }} world */
  handleAt(world) {
    if (this.doc === null || this.selected.length !== 1) return null;
    const element = this.doc.elements.find((item) => item.id === this.selected[0]);
    if (element === undefined || element.type !== "box" || element.id === undefined) return null;
    const slop = 10 / this.scale;
    for (const handle of HANDLES) {
      const [hx, hy] = handlePoint(element, handle);
      if (Math.abs(world.x - hx) <= slop && Math.abs(world.y - hy) <= slop) {
        return { id: element.id, handle, ox: element.x, oy: element.y, ow: element.w, oh: element.h };
      }
    }
    return null;
  }

  /**
   * @param {number} x1
   * @param {number} y1
   * @param {number} x2
   * @param {number} y2
   */
  linkedArrow(x1, y1, x2, y2) {
    if (this.doc === null) return { x1, y1, x2, y2 };
    const start = hitBox(this.doc, x1, y1, BIND_SLOP);
    const finish = hitBox(this.doc, x2, y2, BIND_SLOP);
    if (start?.id && finish?.id && start.id !== finish.id) return { x1: 0, y1: 0, x2: 0, y2: 0, from: start.id, to: finish.id };
    return { x1, y1, x2, y2 };
  }

  zoomBy(factor, cx) {
    const local = { x: (this.viewport?.w ?? 400) / 2, y: (this.viewport?.h ?? 300) / 2 };
    this.zoomAt(local, this.scale * factor);
    this.sync();
    cx.notify();
  }
}

/** @param {import("gpui-base").ColorTokens} colors */
function island(colors) {
  return h_flex().items_center().bg(colors.background).border(1).border_color(colors.border).rounded_lg().shadow_md();
}

/** @param {string} key */
function toolFor(key) {
  if (key === "v" || key === "1") return "select";
  if (key === "h") return "hand";
  if (key === "r" || key === "2") return "rect";
  if (key === "a" || key === "5") return "arrow";
  if (key === "l" || key === "6") return "line";
  if (key === "p" || key === "7") return "pen";
  if (key === "t" || key === "8") return "text";
  if (key === "e" || key === "0") return "erase";
  return "";
}

/** @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2 @param {boolean} square */
function boxBetween(x1, y1, x2, y2, square) {
  let right = x2;
  let bottom = y2;
  if (square) {
    const side = Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1));
    right = x1 + Math.sign(x2 - x1 || 1) * side;
    bottom = y1 + Math.sign(y2 - y1 || 1) * side;
  }
  return { x: Math.min(x1, right), y: Math.min(y1, bottom), w: Math.abs(right - x1), h: Math.abs(bottom - y1) };
}

/** @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2 */
function snap(x1, y1, x2, y2) {
  const angle = Math.atan2(y2 - y1, x2 - x1);
  const step = Math.PI / 12;
  const snapped = Math.round(angle / step) * step;
  const length = Math.hypot(x2 - x1, y2 - y1);
  return { x: x1 + Math.cos(snapped) * length, y: y1 + Math.sin(snapped) * length };
}

/** @param {import("./model.js").Doc} doc @param {number} x @param {number} y @param {number} scale */
function hitTest(doc, x, y, scale) {
  const slop = 8 / scale;
  for (let index = doc.elements.length - 1; index >= 0; index -= 1) {
    const element = doc.elements[index];
    if (hits(element, doc.elements, x, y, slop)) return element;
  }
  return undefined;
}

/** @param {import("./model.js").Doc} doc @param {number} x @param {number} y @param {number} [slop] */
function hitBox(doc, x, y, slop = 0) {
  for (let index = doc.elements.length - 1; index >= 0; index -= 1) {
    const element = doc.elements[index];
    if (element.type === "box" && x >= element.x - slop && y >= element.y - slop && x <= element.x + element.w + slop && y <= element.y + element.h + slop) return element;
  }
  return undefined;
}

/**
 * @param {{ x: number, y: number, handle?: string, ox?: number, oy?: number, ow?: number, oh?: number, id?: string }} gesture
 * @param {{ x: number, y: number }} world
 */
function resized(gesture, world) {
  const dx = world.x - gesture.x;
  const dy = world.y - gesture.y;
  let x = gesture.ox ?? 0;
  let y = gesture.oy ?? 0;
  let w = gesture.ow ?? MIN_BOX;
  let h = gesture.oh ?? MIN_BOX;
  const handle = gesture.handle ?? "";
  if (handle.includes("w")) {
    x += dx;
    w -= dx;
  }
  if (handle.includes("e")) w += dx;
  if (handle.includes("n")) {
    y += dy;
    h -= dy;
  }
  if (handle.includes("s")) h += dy;
  if (w < 0) {
    x += w;
    w = -w;
  }
  if (h < 0) {
    y += h;
    h = -h;
  }
  return { id: gesture.id ?? "", x, y, w: Math.max(MIN_BOX, w), h: Math.max(MIN_BOX, h) };
}

/** @param {{ x: number, y: number, w: number, h: number }} box @param {string} handle */
function handlePoint(box, handle) {
  const x = handle.includes("w") ? box.x : handle.includes("e") ? box.x + box.w : box.x + box.w / 2;
  const y = handle.includes("n") ? box.y : handle.includes("s") ? box.y + box.h : box.y + box.h / 2;
  return [x, y];
}

/**
 * @param {import("./model.js").ElementRecord} element
 * @param {import("./model.js").ElementRecord[]} elements
 * @param {number} x
 * @param {number} y
 * @param {number} slop
 */
function hits(element, elements, x, y, slop) {
  const bounds = boundsOf(element, elements);
  if (element.type === "box" || element.type === "text") {
    return x >= bounds.x - slop && y >= bounds.y - slop && x <= bounds.x + bounds.w + slop && y <= bounds.y + bounds.h + slop;
  }
  const points = element.type === "path" ? element.points : arrowPoints(element, elements).map(({ x, y }) => [x, y]);
  return points.slice(1).some((point, ix) => nearSegment(x, y, points[ix][0], points[ix][1], point[0], point[1], slop));
}

/** @param {number} px @param {number} py @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2 @param {number} slop */
function nearSegment(px, py, x1, y1, x2, y2, slop) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const length = dx * dx + dy * dy;
  const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / length));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy)) <= slop;
}

/** @param {{ x: number, y: number, w: number, h: number }} a @param {{ x: number, y: number, w: number, h: number }} b */
function intersects(a, b) {
  return a.w > 0 && a.h > 0 && a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

/** @param {import("./model.js").ElementRecord[]} elements @param {string[]} ids */
function deleteOps(elements, ids) {
  const chosen = new Set(ids);
  return ids
    .filter((id) => {
      const element = elements.find((item) => item.id === id);
      if (element === undefined) return false;
      if (element.type === "arrow" && ((element.from && chosen.has(element.from)) || (element.to && chosen.has(element.to)))) return false;
      return true;
    })
    .map((id) => ({ op: "delete", id }));
}

/** @param {import("./model.js").Doc} doc @param {string} note */
function shareText(doc, note) {
  const body = note.trim();
  const word = /^[\w.][\w.-]*$/.test(doc.name) ? doc.name : `'${doc.name.split("'").join("'\\''")}'`;
  return [
    ...(body === "" ? [] : [body, ""]),
    `The canvas beside you is "${doc.name}". Run this to see what is on it:`,
    "",
    `kg canvas show ${word}`,
    "",
    "Draw on it with the canvas_edit tool; the canvas skill says how.",
    body === "" ? "In one or two sentences, tell me what you see. Ask whether I want to draw or change anything." : "Then do what I asked.",
  ].join("\n");
}
