// @ts-check

export const LABEL_FONT = 18;
export const ARROW_FONT = 16;
export const CHAR_W = 10;
export const LINE_H = 20;
export const BOX_PAD = 12;
export const TEXT_MAX_W = 360;

/** @param {string} text @param {number} maxChars */
export function wrapLines(text, maxChars) {
  const width = Math.max(1, maxChars);
  /** @type {string[]} */
  const lines = [];
  for (const paragraph of text.split(/\r?\n/)) {
    if (paragraph === "") {
      lines.push("");
      continue;
    }
    let current = "";
    for (const word of paragraph.split(/\s+/)) {
      if (word.length > width) {
        if (current !== "") {
          lines.push(current);
          current = "";
        }
        for (let i = 0; i < word.length; i += width) lines.push(word.slice(i, i + width));
        continue;
      }
      const next = current === "" ? word : `${current} ${word}`;
      if (next.length > width) {
        lines.push(current);
        current = word;
      } else current = next;
    }
    if (current !== "") lines.push(current);
  }
  return lines.length === 0 ? [""] : lines;
}

/** @param {string} label @param {number} w */
export function boxLabelLines(label, w) {
  const inner = Math.max(CHAR_W, w - BOX_PAD * 2);
  return wrapLines(label, Math.max(1, Math.floor(inner / CHAR_W)));
}

/** @param {string} label @param {number} w @param {number} h */
export function visibleBoxLines(label, w, h) {
  const lines = boxLabelLines(label, w);
  const maxLines = Math.max(1, Math.floor((h - BOX_PAD * 2) / LINE_H));
  return lines.slice(0, maxLines);
}

/** @param {number} w @param {string | undefined} label */
export function boxHeightForLabel(w, label) {
  if (label === undefined || label === "") return 0;
  return BOX_PAD * 2 + boxLabelLines(label, w).length * LINE_H;
}

/** @param {string} text */
export function textLines(text) {
  return wrapLines(text, Math.max(1, Math.floor(TEXT_MAX_W / CHAR_W)));
}

/** @param {number} x @param {number} y @param {string} text */
export function textBounds(x, y, text) {
  const lines = textLines(text);
  const width = Math.max(...lines.map((line) => line.length), 1) * CHAR_W;
  return { x, y: y - LABEL_FONT, w: width, h: LABEL_FONT + (lines.length - 1) * LINE_H + 4 };
}

/** @param {number} a1 @param {number} a2 @param {number} b1 @param {number} b2 */
function gap(a1, a2, b1, b2) {
  if (b1 > a2) return b1 - a2;
  if (a1 > b2) return a1 - b2;
  return 0;
}

/** @param {number} lo @param {number} hi */
function centre(lo, hi) {
  return (lo + hi) / 2;
}

/** @param {number} ca @param {number} cb @param {number} ea @param {number} eb */
function before(ca, cb, ea, eb) {
  if (ca !== cb) return ca < cb;
  return ea <= eb;
}

/** @param {{ x: number, y: number, w: number, h: number }} a @param {{ x: number, y: number, w: number, h: number }} b */
function facingRows(a, b) {
  if (before(centre(a.y, a.y + a.h), centre(b.y, b.y + b.h), a.y, b.y)) return { ay: a.y + a.h, by: b.y };
  return { ay: a.y, by: b.y + b.h };
}

/** @param {{ x: number, y: number, w: number, h: number }} a @param {{ x: number, y: number, w: number, h: number }} b */
function facingCols(a, b) {
  if (before(centre(a.x, a.x + a.w), centre(b.x, b.x + b.w), a.x, b.x)) return { ax: a.x + a.w, bx: b.x };
  return { ax: a.x, bx: b.x + b.w };
}

/** @param {{ x: number, y: number, w: number, h: number }} a @param {{ x: number, y: number, w: number, h: number }} b */
export function edgeEnds(a, b) {
  const rowGap = gap(a.y, a.y + a.h, b.y, b.y + b.h);
  const colGap = gap(a.x, a.x + a.w, b.x, b.x + b.w);
  const { ay, by } = facingRows(a, b);
  const { ax, bx } = facingCols(a, b);
  if (rowGap > colGap) {
    if (colGap === 0) {
      const x = centre(Math.max(a.x, b.x), Math.min(a.x + a.w, b.x + b.w));
      return { x1: x, y1: ay, x2: x, y2: by, vertical: true };
    }
    return { x1: centre(a.x, a.x + a.w), y1: ay, x2: centre(b.x, b.x + b.w), y2: by, vertical: true };
  }
  if (rowGap === 0) {
    const y = centre(Math.max(a.y, b.y), Math.min(a.y + a.h, b.y + b.h));
    return { x1: ax, y1: y, x2: bx, y2: y, vertical: false };
  }
  if (colGap >= 16) return { x1: ax, y1: centre(a.y, a.y + a.h), x2: bx, y2: centre(b.y, b.y + b.h), vertical: false };
  return { x1: ax, y1: ay, x2: bx, y2: by, vertical: true };
}

/** @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2 @param {boolean} vertical */
export function edgePath(x1, y1, x2, y2, vertical) {
  if (vertical) {
    if (x1 === x2) return [{ x: x1, y: y1 }, { x: x2, y: y2 }];
    const mid = (y1 + y2) / 2;
    return [{ x: x1, y: y1 }, { x: x1, y: mid }, { x: x2, y: mid }, { x: x2, y: y2 }];
  }
  if (y1 === y2) return [{ x: x1, y: y1 }, { x: x2, y: y2 }];
  const mid = (x1 + x2) / 2;
  return [{ x: x1, y: y1 }, { x: mid, y: y1 }, { x: mid, y: y2 }, { x: x2, y: y2 }];
}

/** @param {{ x: number, y: number, w: number, h: number }} rect @param {{ x: number, y: number }} from @param {{ x: number, y: number }} toward */
function clipToBox(rect, from, toward) {
  const dx = toward.x - from.x;
  const dy = toward.y - from.y;
  if (dx === 0 && dy === 0) return from;
  const sx = dx === 0 ? Infinity : rect.w / 2 / Math.abs(dx);
  const sy = dy === 0 ? Infinity : rect.h / 2 / Math.abs(dy);
  const s = Math.min(sx, sy);
  return { x: from.x + dx * s, y: from.y + dy * s };
}

/** @param {{ x: number, y: number }[]} points */
function dedupe(points) {
  /** @type {{ x: number, y: number }[]} */
  const out = [];
  for (const point of points) {
    const last = out[out.length - 1];
    if (last === undefined || Math.abs(last.x - point.x) + Math.abs(last.y - point.y) > 0.5) out.push(point);
  }
  if (out.length >= 2) return out;
  if (out.length === 1) return [out[0], out[0]];
  return out;
}

/**
 * @param {{ x1: number, y1: number, x2: number, y2: number, from?: string, to?: string }} arrow
 * @param {Map<string, { x: number, y: number, w: number, h: number }>} boxes
 */
export function arrowRoute(arrow, boxes) {
  const a = arrow.from === undefined ? undefined : boxes.get(arrow.from);
  const b = arrow.to === undefined ? undefined : boxes.get(arrow.to);
  if (a !== undefined && b !== undefined) {
    const ends = edgeEnds(a, b);
    return dedupe(edgePath(ends.x1, ends.y1, ends.x2, ends.y2, ends.vertical));
  }
  const start = a ? { x: a.x + a.w / 2, y: a.y + a.h / 2 } : { x: arrow.x1, y: arrow.y1 };
  const end = b ? { x: b.x + b.w / 2, y: b.y + b.h / 2 } : { x: arrow.x2, y: arrow.y2 };
  const p1 = a ? clipToBox(a, start, end) : start;
  const p2 = b ? clipToBox(b, end, start) : end;
  return [p1, p2];
}

/** @param {{ x: number, y: number }[]} points */
export function longestSegment(points) {
  let best = { mid: points[0] ?? { x: 0, y: 0 }, length: 0, horizontal: true };
  for (let i = 0; i < points.length - 1; i++) {
    const from = points[i];
    const to = points[i + 1];
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const length = Math.hypot(dx, dy);
    if (length > best.length) best = { mid: { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 }, length, horizontal: Math.abs(dx) >= Math.abs(dy) };
  }
  return best;
}

/** @param {string} label @param {{ x: number, y: number }[]} points */
export function arrowLabelAt(label, points) {
  const run = longestSegment(points);
  const width = label.length * (ARROW_FONT * 0.55);
  if (run.length < width + 16) return null;
  return run.mid;
}

/** @param {{ x: number, y: number, w: number, h: number }} a @param {{ x: number, y: number, w: number, h: number }} b */
function boxesOverlap(a, b) {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

/** @param {{ x: number, y: number, w: number, h: number }} outer @param {{ x: number, y: number, w: number, h: number }} inner */
function contains(outer, inner) {
  return inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.w <= outer.x + outer.w && inner.y + inner.h <= outer.y + outer.h;
}

/** @param {Array<{ id: string, x: number, y: number, w: number, h: number, label?: string }>} boxes */
export function layoutNotes(boxes) {
  /** @type {string[]} */
  const notes = [];
  for (const box of boxes) {
    if (box.label === undefined || box.label === "") continue;
    if (boxHeightForLabel(box.w, box.label) > box.h + 0.5) notes.push(`${box.id} label clipped`);
  }
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i];
      const b = boxes[j];
      if (boxesOverlap(a, b) && !contains(a, b) && !contains(b, a)) notes.push(`${a.id} overlaps ${b.id}`);
    }
  }
  if (notes.length <= 8) return notes;
  return [...notes.slice(0, 8), `+${notes.length - 8} more`];
}
