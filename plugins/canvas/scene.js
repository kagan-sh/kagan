// @ts-check

import { arrowLabelAt, arrowRoute, textBounds, textLines, visibleBoxLines, BOX_PAD, LABEL_FONT, ARROW_FONT } from "./layout.js";
import { arrowPoints } from "./model.js";

const LIMIT = 1_000_000;

/** @param {string | undefined} color */
function ink(color) {
  return color ?? "ink";
}

/** @param {string} id */
function seedFor(id) {
  let hash = 2166136261;
  for (let i = 0; i < id.length; i++) {
    hash ^= id.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/** @param {number} value */
function finite(value) {
  return Number.isFinite(value) && Math.abs(value) <= LIMIT;
}

/** @param {number} x @param {number} y @param {number} w @param {number} h */
function legalRect(x, y, w, h) {
  return finite(x) && finite(y) && finite(w) && finite(h) && w >= 0 && h >= 0 && x + w <= LIMIT && y + h <= LIMIT;
}

/**
 * @param {import("gpui-component").SketchScene} scene
 * @param {import("./model.js").ElementRecord[]} elements
 * @param {{ x: number, y: number, scale: number }} view
 * @param {string[]} selected
 * @param {import("./model.js").ElementRecord | null} wip
 * @param {Set<string>} painted
 * @param {Map<string, string>} sent
 */
export function syncScene(scene, elements, view, selected, wip, painted, sent) {
  scene.view(view.x, view.y, view.scale);
  /** @type {Set<string>} */
  const next = new Set();
  /** @type {Map<string, { x: number, y: number, w: number, h: number }>} */
  const boxes = new Map();
  for (const element of elements) {
    if (element.type === "box" && element.id) boxes.set(element.id, element);
  }
  for (const element of elements) paintElement(scene, element, elements, boxes, false, next, sent);
  if (wip) paintElement(scene, wip, elements, boxes, true, next, sent);
  for (const id of selected) {
    const element = elements.find((item) => item.id === id);
    if (element) paintHalo(scene, element, elements, next);
  }
  for (const key of painted) {
    if (next.has(key)) continue;
    const overlay = key.startsWith("overlay:");
    try {
      scene.remove(overlay ? key.slice("overlay:".length) : key, overlay);
      sent.delete(key);
    } catch {
      // A rejected shape never landed, so the next sync has nothing to drop.
    }
  }
  painted.clear();
  for (const key of next) painted.add(key);
}

/**
 * @param {import("gpui-component").SketchScene} scene
 * @param {import("./model.js").ElementRecord} element
 * @param {import("./model.js").ElementRecord[]} elements
 * @param {Map<string, { x: number, y: number, w: number, h: number }>} boxes
 * @param {boolean} overlay
 * @param {Set<string>} next
 * @param {Map<string, string>} sent
 */
function paintElement(scene, element, elements, boxes, overlay, next, sent) {
  const id = element.id ?? "wip";
  const signature = JSON.stringify([element, element.type === "arrow" ? [boxes.get(element.from ?? ""), boxes.get(element.to ?? "")] : null]);
  const stored = overlay ? `overlay:${id}` : id;
  const label = overlay ? `overlay:${id}:label` : `${id}:label`;
  if (sent.get(stored) === signature) {
    next.add(stored);
    if (sent.has(label)) next.add(label);
    return;
  }
  const color = ink(element.color);
  const key = (/** @type {string} */ name) => {
    const stored = overlay ? `overlay:${name}` : name;
    next.add(stored);
    sent.set(stored, signature);
    return name;
  };
  if (element.type === "box") {
    if (!legalRect(element.x, element.y, element.w, element.h)) return;
    scene.rect(key(id), element.x, element.y, element.w, element.h, color, 2, seedFor(id), overlay);
    if (element.label) {
      const lines = visibleBoxLines(element.label, element.w, element.h).filter((line) => !line.includes("\n"));
      if (lines.length > 0) {
        scene.text(key(`${id}:label`), element.x + element.w / 2, element.y + BOX_PAD, lines, LABEL_FONT, "middle", color, overlay);
      }
    }
    return;
  }
  if (element.type === "arrow") {
    const points = element.from || element.to ? arrowRoute(element, boxes).map((point) => [point.x, point.y]) : arrowPoints(element, elements).map((point) => [point.x, point.y]);
    if (points.length < 2 || points.some((point) => !finite(point[0]) || !finite(point[1]))) return;
    const heads = element.heads === "none" || element.heads === "start" || element.heads === "both" ? element.heads : "end";
    scene.line(key(id), points, heads, color, 2, seedFor(id), overlay);
    if (element.label) {
      const at = arrowLabelAt(element.label, points.map(([x, y]) => ({ x, y })));
      if (at && finite(at.x) && finite(at.y)) {
        scene.text(key(`${id}:label`), at.x, at.y - ARROW_FONT / 2, [element.label.replace(/[\r\n]/g, " ")], ARROW_FONT, "middle", color, overlay);
      }
    }
    return;
  }
  if (element.type === "text") {
    const bounds = textBounds(element.x, element.y, element.text);
    const lines = textLines(element.text).map((line) => line.replace(/[\r\n]/g, ""));
    if (!legalRect(bounds.x, bounds.y, Math.max(bounds.w, 1), Math.max(bounds.h, 1))) return;
    scene.text(key(id), bounds.x, bounds.y, lines, LABEL_FONT, "start", color, overlay);
    return;
  }
  if (element.points.length < 2) return;
  const samples = element.points.map(([x, y]) => [x, y]);
  if (samples.some((point) => !finite(point[0]) || !finite(point[1]))) return;
  scene.ink(key(id), samples, color, 4, overlay);
}

/**
 * @param {import("gpui-component").SketchScene} scene
 * @param {import("./model.js").ElementRecord} element
 * @param {import("./model.js").ElementRecord[]} elements
 * @param {Set<string>} next
 */
function paintHalo(scene, element, elements, next) {
  const bounds = boundsOf(element, elements);
  if (bounds === null) return;
  const x = bounds.x - 4;
  const y = bounds.y - 4;
  const w = bounds.w + 8;
  const h = bounds.h + 8;
  if (!legalRect(x, y, w, h)) return;
  const name = `sel:${element.id}`;
  next.add(`overlay:${name}`);
  scene.rect(name, x, y, w, h, "accent", 1.5, 1, true);
}

/** @param {import("./model.js").ElementRecord} element @param {import("./model.js").ElementRecord[]} elements */
export function boundsOf(element, elements) {
  if (element.type === "box") return { x: element.x, y: element.y, w: element.w, h: element.h };
  if (element.type === "text") return textBounds(element.x, element.y, element.text);
  if (element.type === "arrow") {
    const points = arrowPoints(element, elements);
    const xs = points.map((point) => point.x);
    const ys = points.map((point) => point.y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
  }
  const xs = element.points.map((point) => point[0]);
  const ys = element.points.map((point) => point[1]);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(Math.max(...xs) - x, 1), h: Math.max(Math.max(...ys) - y, 1) };
}
