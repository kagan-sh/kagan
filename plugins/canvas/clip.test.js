// @ts-check
import assert from "node:assert/strict";
import { test } from "node:test";
import { copyElements, decodeClip, encodeClip, pasteOps } from "./clip.js";
import { applyOps, emptyDoc } from "./model.js";

test("clipboard preserves copied bindings and detaches missing boxes", () => {
  const doc = applyOps(emptyDoc("test"), [
    { op: "add", as: "left", element: { type: "box", x: 0, y: 0, w: 100, h: 50 } },
    { op: "add", as: "right", element: { type: "box", x: 300, y: 0, w: 100, h: 50 } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "left", to: "right" } },
  ]).doc;
  const clip = copyElements(["b1", "b2", "a3"], doc.elements);
  assert.deepEqual(decodeClip(encodeClip(clip)), clip);
  const pasted = applyOps(doc, pasteOps(clip, 0, 200)).doc;
  assert.deepEqual(pasted.elements.at(-1), { type: "arrow", id: "a6", x1: 0, y1: 0, x2: 0, y2: 0, from: "b4", to: "b5" });
  const partial = copyElements(["a3"], doc.elements);
  assert.deepEqual(partial[0], { type: "arrow", id: "a3", x1: 100, y1: 25, x2: 300, y2: 25 });
  assert.equal(decodeClip('{"kaganCanvas":1,"elements":[{"id":"b1","type":"box"}]}'), null);
  assert.equal(decodeClip('{"kaganCanvas":1,"elements":[{"id":"p1","type":"path","points":[[0,0],[null,1]]}]}'), null);
});
