// @ts-check

import { arrowPoints, contentBounds, emptyDoc, parseDoc } from "./model.js";

/** @param {import("./model.js").ElementRecord} element */
export function isBound(element) {
  return element.type === "arrow" && (element.from !== undefined || element.to !== undefined);
}

/** @param {import("./model.js").ElementRecord} element @param {number} dx @param {number} dy */
export function translated(element, dx, dy) {
  if (element.type === "box" || element.type === "text") return { ...element, x: element.x + dx, y: element.y + dy };
  if (element.type === "arrow") {
    return isBound(element) ? element : { ...element, x1: element.x1 + dx, y1: element.y1 + dy, x2: element.x2 + dx, y2: element.y2 + dy };
  }
  return { ...element, points: element.points.map(([x, y]) => [x + dx, y + dy]) };
}

/** @param {string[]} ids @param {import("./model.js").ElementRecord[]} elements */
export function copyElements(ids, elements) {
  const chosen = elements.filter((element) => ids.includes(element.id ?? ""));
  const boxes = new Set(chosen.filter((element) => element.type === "box").map((element) => element.id));
  return chosen.map((element) => {
    if (element.type !== "arrow" || !isBound(element)) return element;
    if (element.from !== undefined && element.to !== undefined && boxes.has(element.from) && boxes.has(element.to)) return element;
    const points = arrowPoints(element, elements);
    const { from: _from, to: _to, ...free } = element;
    return { ...free, x1: points[0].x, y1: points[0].y, x2: points[points.length - 1].x, y2: points[points.length - 1].y };
  });
}

/** @param {import("./model.js").ElementRecord[]} clip @param {number} dx @param {number} dy */
export function pasteOps(clip, dx, dy) {
  const alias = (/** @type {string} */ id) => `copy_${id}`;
  const boxes = new Set(clip.filter((element) => element.type === "box").map((element) => element.id));
  const shapes = clip.filter((element) => element.type !== "arrow");
  const arrows = clip.filter((element) => element.type === "arrow");
  return [...shapes, ...arrows].map((element) => {
    if (element.type === "arrow") {
      const { id: _id, from, to, ...free } = element;
      if (from !== undefined && to !== undefined && boxes.has(from) && boxes.has(to)) {
        return { op: "add", element: { ...free, from: alias(from), to: alias(to) } };
      }
      const moved = translated(free, dx, dy);
      const { id: _freeId, ...rest } = moved;
      return { op: "add", element: rest };
    }
    const moved = translated(element, dx, dy);
    const { id, ...rest } = moved;
    return { op: "add", as: alias(id ?? "item"), element: rest };
  });
}

/** @param {import("./model.js").ElementRecord[]} clip */
export function clipCenter(clip) {
  const bounds = contentBounds({ ...emptyDoc(""), elements: clip });
  return bounds === null ? { x: 0, y: 0 } : { x: bounds.x + bounds.w / 2, y: bounds.y + bounds.h / 2 };
}

/** @param {import("./model.js").ElementRecord[]} clip */
export function encodeClip(clip) {
  return JSON.stringify({ kaganCanvas: 1, elements: clip });
}

/** @param {string} text */
export function decodeClip(text) {
  try {
    const parsed = JSON.parse(text);
    if (parsed === null || typeof parsed !== "object") return null;
    if (parsed.kaganCanvas !== 1) return null;
    if (!Array.isArray(parsed.elements) || parsed.elements.length === 0 || parsed.elements.length > 500) return null;
    return parseDoc({ ...emptyDoc(""), elements: parsed.elements }, "").elements;
  } catch {
    return null;
  }
}
