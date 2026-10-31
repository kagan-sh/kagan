// @ts-check

import { arrowRoute, layoutNotes, textBounds } from "./layout.js";

export const WORLD_LIMIT = 1_000_000;
export const COLORS = ["ink", "accent", "muted", "working", "needs", "failed"];
export const HEADS = ["none", "start", "end", "both"];
const PREFIX = { box: "b", arrow: "a", text: "t", path: "p" };
const ID = /^[batp]\d+$/;
const REF = /^[A-Za-z][A-Za-z0-9_-]*$/;
const PATCHABLE = {
  box: ["x", "y", "w", "h", "label", "color"],
  arrow: ["x1", "y1", "x2", "y2", "label", "color", "heads"],
  text: ["x", "y", "text", "color"],
  path: ["color"],
};

/** @param {string} name */
export function emptyDoc(name) {
  return { name, rev: 0, next: 1, elements: /** @type {ElementRecord[]} */ ([]) };
}

/** @param {string} name */
export function normalizeName(name) {
  return name.trim().toLowerCase().replace(/\s+/g, "-");
}

/** @param {string} name */
export function slug(name) {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "canvas";
}

/**
 * @typedef {{ type: "box", id?: string, x: number, y: number, w: number, h: number, label?: string, color?: string }} BoxRecord
 * @typedef {{ type: "arrow", id?: string, x1: number, y1: number, x2: number, y2: number, heads?: string, from?: string, to?: string, label?: string, color?: string }} ArrowRecord
 * @typedef {{ type: "text", id?: string, x: number, y: number, text: string, color?: string }} TextRecord
 * @typedef {{ type: "path", id?: string, points: number[][], color?: string }} PathRecord
 * @typedef {BoxRecord | ArrowRecord | TextRecord | PathRecord} ElementRecord
 * @typedef {{ name: string, rev: number, next: number, elements: ElementRecord[] }} Doc
 */

/** @param {unknown} value @param {string} label */
function coord(value, label) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < -WORLD_LIMIT || value > WORLD_LIMIT) {
    throw new Error(`${label} is out of range`);
  }
  return value;
}

/** @param {unknown} value @param {string} label */
function positive(value, label) {
  const number = coord(value, label);
  if (number <= 0) throw new Error(`${label} must be positive`);
  return number;
}

/** @param {unknown} value @param {string} label */
function ref(value, label) {
  if (typeof value !== "string" || value.length > 60 || !REF.test(value)) throw new Error(`${label} is not a name`);
  return value;
}

/** @param {unknown} value */
function colorOf(value) {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !COLORS.includes(value)) throw new Error(`unknown color ${String(value)}`);
  return value;
}

/** @param {unknown} value */
function headsOf(value) {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !HEADS.includes(value)) throw new Error(`unknown heads ${String(value)}`);
  return value;
}

/** @param {Record<string, unknown>} raw @param {string[]} allowed */
function only(raw, allowed) {
  for (const key of Object.keys(raw)) {
    if (!allowed.includes(key)) throw new Error(`unexpected field ${key}`);
  }
}

/** @param {unknown} raw */
function parseElement(raw) {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw new Error("element must be an object");
  const element = /** @type {Record<string, unknown>} */ (raw);
  const type = element.type;
  if (type === "box") {
    only(element, ["type", "id", "x", "y", "w", "h", "label", "color"]);
    /** @type {BoxRecord} */
    const box = {
      type: "box",
      x: coord(element.x, "x"),
      y: coord(element.y, "y"),
      w: positive(element.w, "w"),
      h: positive(element.h, "h"),
    };
    if (element.label !== undefined) {
      if (typeof element.label !== "string" || element.label.length > 200) throw new Error("label is too long");
      box.label = element.label;
    }
    const color = colorOf(element.color);
    if (color !== undefined) box.color = color;
    if (box.x + box.w > WORLD_LIMIT || box.y + box.h > WORLD_LIMIT) throw new Error("box must stay within ±1000000");
    return box;
  }
  if (type === "arrow") {
    only(element, ["type", "id", "x1", "y1", "x2", "y2", "heads", "from", "to", "label", "color"]);
    /** @type {ArrowRecord} */
    const arrow = {
      type: "arrow",
      x1: coord(element.x1, "x1"),
      y1: coord(element.y1, "y1"),
      x2: coord(element.x2, "x2"),
      y2: coord(element.y2, "y2"),
    };
    const heads = headsOf(element.heads);
    if (heads !== undefined) arrow.heads = heads;
    if (element.from !== undefined) arrow.from = ref(element.from, "from");
    if (element.to !== undefined) arrow.to = ref(element.to, "to");
    if (element.label !== undefined) {
      if (typeof element.label !== "string" || element.label.length > 200) throw new Error("label is too long");
      arrow.label = element.label;
    }
    const color = colorOf(element.color);
    if (color !== undefined) arrow.color = color;
    return arrow;
  }
  if (type === "text") {
    only(element, ["type", "id", "x", "y", "text", "color"]);
    if (typeof element.text !== "string" || element.text.length < 1 || element.text.length > 500) {
      throw new Error("text must be 1 to 500 characters");
    }
    /** @type {TextRecord} */
    const text = { type: "text", x: coord(element.x, "x"), y: coord(element.y, "y"), text: element.text };
    const color = colorOf(element.color);
    if (color !== undefined) text.color = color;
    return text;
  }
  if (type === "path") {
    only(element, ["type", "id", "points", "color"]);
    if (!Array.isArray(element.points) || element.points.length < 2 || element.points.length > 2000) {
      throw new Error("a freehand path needs 2 to 2000 points");
    }
    /** @type {number[][]} */
    const points = element.points.map((point, index) => {
      if (!Array.isArray(point) || point.length !== 2) throw new Error(`point ${index} must be a pair`);
      return [coord(point[0], "x"), coord(point[1], "y")];
    });
    /** @type {PathRecord} */
    const path = { type: "path", points };
    const color = colorOf(element.color);
    if (color !== undefined) path.color = color;
    return path;
  }
  throw new Error(`unknown element type ${String(type)}`);
}

/**
 * @param {unknown} raw
 * @returns {{ op: "add", element: ElementRecord, as?: string } | { op: "update", id: string, patch: Record<string, unknown> } | { op: "move", id: string, dx: number, dy: number } | { op: "delete", id: string } | { op: "clear" }}
 */
function parseOp(raw) {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw new Error("op must be an object");
  const op = /** @type {Record<string, unknown>} */ (raw);
  if (op.op === "add") {
    only(op, ["op", "element", "as"]);
    /** @type {{ op: "add", element: ElementRecord, as?: string }} */
    const add = { op: "add", element: parseElement(op.element) };
    if (op.as !== undefined) {
      const alias = ref(op.as, "as");
      if (ID.test(alias)) throw new Error("An alias must not look like an id");
      add.as = alias;
    }
    return add;
  }
  if (op.op === "update") {
    only(op, ["op", "id", "patch"]);
    if (op.patch === null || typeof op.patch !== "object" || Array.isArray(op.patch)) throw new Error("patch must be an object");
    return { op: "update", id: ref(op.id, "id"), patch: /** @type {Record<string, unknown>} */ (op.patch) };
  }
  if (op.op === "move") {
    only(op, ["op", "id", "dx", "dy"]);
    return { op: "move", id: ref(op.id, "id"), dx: coord(op.dx, "dx"), dy: coord(op.dy, "dy") };
  }
  if (op.op === "delete") {
    only(op, ["op", "id"]);
    return { op: "delete", id: ref(op.id, "id") };
  }
  if (op.op === "clear") {
    only(op, ["op"]);
    return { op: "clear" };
  }
  throw new Error(`unknown op ${String(op.op)}`);
}

/** @param {Record<string, unknown>} patch @param {ElementRecord} element */
function applyPatch(patch, element) {
  for (const [key, value] of Object.entries(patch)) {
    if (key === "from" || key === "to") continue;
    if (!PATCHABLE[element.type].includes(key)) throw new Error(`${element.id} is a ${element.type}; it has no "${key}"`);
    if (key === "x" || key === "y" || key === "x1" || key === "y1" || key === "x2" || key === "y2") {
      element[key] = coord(value, key);
    } else if (key === "w" || key === "h") {
      element[key] = positive(value, key);
    } else if (key === "label") {
      if (typeof value !== "string" || value.length > 200) throw new Error("label is too long");
      if (element.type === "box" || element.type === "arrow") element.label = value;
    } else if (key === "text") {
      if (typeof value !== "string" || value.length < 1 || value.length > 500) throw new Error("text must be 1 to 500 characters");
      if (element.type === "text") element.text = value;
    } else if (key === "color") {
      const color = colorOf(value);
      if (color === undefined) throw new Error("color is required");
      element.color = color;
    } else if (key === "heads") {
      const heads = headsOf(value);
      if (heads === undefined) throw new Error("heads is required");
      if (element.type === "arrow") element.heads = heads;
    }
  }
  if (element.type === "box" && (element.x + element.w > WORLD_LIMIT || element.y + element.h > WORLD_LIMIT)) {
    throw new Error("box must stay within ±1000000");
  }
}

/**
 * Applies every op or none. The input doc is left unchanged.
 * @param {Doc} doc
 * @param {unknown[]} ops
 */
export function applyOps(doc, ops) {
  if (!Array.isArray(ops) || ops.length < 1 || ops.length > 500) throw new Error("ops must be 1 to 500");
  const parsed = ops.map(parseOp);
  /** @type {ElementRecord[]} */
  let elements = doc.elements.map((element) =>
    element.type === "path" ? { ...element, points: element.points.map((point) => [point[0], point[1]]) } : { ...element },
  );
  let next = doc.next;
  /** @type {string[]} */
  const added = [];
  /** @type {Map<string, string>} */
  const aliases = new Map();
  const resolve = (/** @type {string} */ reference) => aliases.get(reference) ?? reference;
  const find = (/** @type {string} */ elementId) => {
    const element = elements.find((candidate) => candidate.id === elementId);
    if (element === undefined) throw new Error(`no element ${elementId}`);
    return element;
  };
  const assertBox = (/** @type {string | null | undefined} */ elementId) => {
    if (elementId == null) return;
    if (find(elementId).type !== "box") throw new Error(`${elementId} is not a box`);
  };
  for (const op of parsed) {
    if (op.op === "add") {
      /** @type {ElementRecord} */
      const element = op.element.type === "arrow"
        ? {
            ...op.element,
            ...(op.element.from === undefined ? {} : { from: resolve(op.element.from) }),
            ...(op.element.to === undefined ? {} : { to: resolve(op.element.to) }),
          }
        : op.element;
      if (element.type === "arrow") {
        assertBox(element.from);
        assertBox(element.to);
        if (element.from !== undefined && element.from === element.to) throw new Error("arrow needs two different boxes");
      }
      const elementId = `${PREFIX[element.type]}${next++}`;
      if (op.as !== undefined) {
        if (aliases.has(op.as)) throw new Error(`alias "${op.as}" is used twice`);
        aliases.set(op.as, elementId);
      }
      elements.push({ ...element, id: elementId });
      added.push(elementId);
    } else if (op.op === "update") {
      const element = find(resolve(op.id));
      const from = op.patch.from === undefined ? undefined : op.patch.from === null ? null : resolve(ref(op.patch.from, "from"));
      const to = op.patch.to === undefined ? undefined : op.patch.to === null ? null : resolve(ref(op.patch.to, "to"));
      if (element.type === "arrow") {
        assertBox(from);
        assertBox(to);
        if (from === null) delete element.from;
        else if (from !== undefined) element.from = from;
        if (to === null) delete element.to;
        else if (to !== undefined) element.to = to;
        if (element.from !== undefined && element.from === element.to) throw new Error("arrow needs two different boxes");
      } else if (op.patch.from !== undefined || op.patch.to !== undefined) {
        throw new Error(`${op.id} is a ${element.type}; it has no "from"`);
      }
      applyPatch(op.patch, element);
    } else if (op.op === "move") {
      const element = find(resolve(op.id));
      if (element.type === "arrow" && (element.from !== undefined || element.to !== undefined)) {
        throw new Error(`${op.id} is attached to a box; move the box instead`);
      }
      if (element.type === "box" || element.type === "text") {
        element.x += op.dx;
        element.y += op.dy;
        coord(element.x, "x");
        coord(element.y, "y");
        if (element.type === "box" && (element.x + element.w > WORLD_LIMIT || element.y + element.h > WORLD_LIMIT)) throw new Error("box must stay within ±1000000");
      } else if (element.type === "arrow") {
        element.x1 += op.dx;
        element.y1 += op.dy;
        element.x2 += op.dx;
        element.y2 += op.dy;
        coord(element.x1, "x1");
        coord(element.y1, "y1");
        coord(element.x2, "x2");
        coord(element.y2, "y2");
      } else {
        element.points = element.points.map(([x, y]) => [coord(x + op.dx, "x"), coord(y + op.dy, "y")]);
      }
    } else if (op.op === "delete") {
      const target = resolve(op.id);
      find(target);
      elements = elements.filter(
        (element) => element.id !== target && !(element.type === "arrow" && (element.from === target || element.to === target)),
      );
    } else {
      elements = [];
    }
  }
  return { doc: { ...doc, rev: doc.rev + 1, next, elements }, added };
}

/** @param {ElementRecord[]} elements */
function boxMap(elements) {
  /** @type {Map<string, { x: number, y: number, w: number, h: number }>} */
  const boxes = new Map();
  for (const element of elements) {
    if (element.type === "box") boxes.set(element.id ?? "", element);
  }
  return boxes;
}

/** @param {ArrowRecord} arrow @param {ElementRecord[]} elements */
export function arrowPoints(arrow, elements) {
  return arrowRoute(arrow, boxMap(elements));
}

/** @param {ArrowRecord} arrow @param {ElementRecord[]} elements */
export function arrowEnds(arrow, elements) {
  const points = arrowPoints(arrow, elements);
  const start = points[0];
  const end = points[points.length - 1];
  return { x1: start.x, y1: start.y, x2: end.x, y2: end.y };
}

/** @param {Doc} doc */
export function contentBounds(doc) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const add = (/** @type {number} */ x, /** @type {number} */ y) => {
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
  };
  for (const element of doc.elements) {
    if (element.type === "box") {
      add(element.x, element.y);
      add(element.x + element.w, element.y + element.h);
    } else if (element.type === "text") {
      const bounds = textBounds(element.x, element.y, element.text);
      add(bounds.x, bounds.y);
      add(bounds.x + bounds.w, bounds.y + bounds.h);
    } else if (element.type === "arrow") {
      for (const point of arrowPoints(element, doc.elements)) add(point.x, point.y);
    } else {
      for (const [x, y] of element.points) add(x, y);
    }
  }
  return minX === Infinity ? null : { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}

const round = (/** @type {number} */ n) => Math.round(n);

/** @param {Doc} doc */
export function exportText(doc) {
  if (doc.elements.length === 0) return `(empty canvas "${doc.name}")`;
  return doc.elements.map((element) => {
    const tint = element.color && element.color !== "ink" ? ` ${element.color}` : "";
    if (element.type === "box") {
      return `${element.id} box ${round(element.x)},${round(element.y)} ${round(element.w)}x${round(element.h)}${tint}${element.label ? ` ${JSON.stringify(element.label)}` : ""}`;
    }
    if (element.type === "arrow") {
      const ends = arrowEnds(element, doc.elements);
      const link = element.from || element.to ? ` ${element.from ?? "?"}->${element.to ?? "?"}` : "";
      const kind = element.heads === "none" ? "line" : "arrow";
      const heads = element.heads === undefined || element.heads === "end" || element.heads === "none" ? "" : ` heads ${element.heads}`;
      return `${element.id} ${kind} ${round(ends.x1)},${round(ends.y1)} -> ${round(ends.x2)},${round(ends.y2)}${link}${heads}${tint}${element.label ? ` ${JSON.stringify(element.label)}` : ""}`;
    }
    if (element.type === "text") return `${element.id} text ${round(element.x)},${round(element.y)}${tint} ${JSON.stringify(element.text)}`;
    const xs = element.points.map((point) => point[0]);
    const ys = element.points.map((point) => point[1]);
    return `${element.id} freehand ${element.points.length} points, bounds ${round(Math.min(...xs))},${round(Math.min(...ys))} to ${round(Math.max(...xs))},${round(Math.max(...ys))}${tint}`;
  }).join("\n");
}

/** @param {Doc} doc */
export function layoutReport(doc) {
  const notes = layoutNotes(
    doc.elements.filter((element) => element.type === "box").map((element) => ({ ...element, id: element.id ?? "" })),
  );
  return notes.length === 0 ? "" : `layout: ${notes.join("; ")}`;
}

/** @param {Doc} doc */
export function summary(doc) {
  const bounds = contentBounds(doc);
  const count = doc.elements.length;
  if (bounds === null) return `canvas "${doc.name}" is empty; draw near 0,0`;
  return `canvas "${doc.name}": ${count} element${count === 1 ? "" : "s"} using ${round(bounds.x)},${round(bounds.y)} to ${round(bounds.x + bounds.w)},${round(bounds.y + bounds.h)}`;
}

/** @param {Doc} doc */
export function dump(doc) {
  const notes = layoutReport(doc);
  return notes === "" ? `${summary(doc)}\n\n${exportText(doc)}` : `${summary(doc)}\n\n${exportText(doc)}\n\n${notes}`;
}

/** @param {Doc} before @param {Doc} after @param {string[]} added */
export function changes(before, after, added) {
  const old = new Set(exportText(before).split("\n"));
  const changed = after.elements.length === 0 ? [] : exportText(after).split("\n").filter((line) => !old.has(line));
  const kept = new Set(after.elements.map((element) => element.id));
  const deleted = before.elements.filter((element) => !kept.has(element.id)).map((element) => element.id);
  return [summary(after), added.length === 0 ? "" : `added: ${added.join(" ")}`, deleted.length === 0 ? "" : `deleted: ${deleted.join(" ")}`, changed.join("\n"), layoutReport(after)]
    .filter((part) => part !== "")
    .join("\n\n");
}

/** @param {unknown} value @param {string} name */
export function parseDoc(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error(`canvas "${name}" is unreadable`);
  const raw = /** @type {Record<string, unknown>} */ (value);
  if (!Array.isArray(raw.elements) || typeof raw.rev !== "number" || typeof raw.next !== "number") {
    throw new Error(`canvas "${name}" is unreadable`);
  }
  /** @type {ElementRecord[]} */
  const elements = raw.elements.map((element) => {
    const parsed = parseElement(element);
    if (element === null || typeof element !== "object") throw new Error("element must be an object");
    const id = /** @type {Record<string, unknown>} */ (element).id;
    if (typeof id !== "string" || !ID.test(id)) throw new Error(`bad id ${String(id)}`);
    return { ...parsed, id };
  });
  return { name: typeof raw.name === "string" ? raw.name : name, rev: raw.rev, next: raw.next, elements };
}
