// @ts-check
import assert from "node:assert/strict";
import { test } from "node:test";
import { syncScene } from "./scene.js";
import { applyOps, emptyDoc } from "./model.js";

test("retained scene only resends changed geometry, including bound arrows", () => {
  const calls = [];
  const scene = {
    view() {},
    rect(key) { calls.push(key); },
    line(key) { calls.push(key); },
    text(key) { calls.push(key); },
    ink(key) { calls.push(key); },
    remove(key) { calls.push(`remove:${key}`); },
  };
  const doc = applyOps(emptyDoc("test"), [
    { op: "add", as: "left", element: { type: "box", x: 0, y: 0, w: 100, h: 50, label: "Left" } },
    { op: "add", as: "right", element: { type: "box", x: 300, y: 0, w: 100, h: 50 } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "left", to: "right" } },
  ]).doc;
  const painted = new Set();
  const sent = new Map();
  const view = { x: 0, y: 0, scale: 1 };
  syncScene(scene, doc.elements, view, [], null, painted, sent);
  assert.deepEqual(calls.splice(0), ["b1", "b1:label", "b2", "a3"]);
  syncScene(scene, doc.elements, { ...view, x: 200 }, [], null, painted, sent);
  assert.deepEqual(calls, []);
  const moved = applyOps(doc, [{ op: "move", id: "b2", dx: 20, dy: 0 }]).doc;
  syncScene(scene, moved.elements, view, [], null, painted, sent);
  assert.deepEqual(calls.splice(0), ["b2", "a3"]);
  const cleared = applyOps(moved, [{ op: "delete", id: "b1" }]).doc;
  syncScene(scene, cleared.elements, view, [], null, painted, sent);
  assert.deepEqual(calls, ["remove:b1", "remove:b1:label", "remove:a3"]);
});
