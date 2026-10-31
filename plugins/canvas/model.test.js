// @ts-check
import assert from "node:assert/strict";
import { test } from "node:test";
import { applyOps, arrowEnds, contentBounds, emptyDoc, exportText, layoutReport } from "./model.js";

test("ids increase and are never reused; deleting a box deletes its arrows", () => {
  const first = applyOps(emptyDoc("t"), [
    { op: "add", element: { type: "box", x: 0, y: 0, w: 100, h: 50, label: "web" } },
    { op: "add", element: { type: "box", x: 300, y: 0, w: 100, h: 50, label: "db" } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "b1", to: "b2" } },
  ]);
  assert.deepEqual(first.added, ["b1", "b2", "a3"]);
  assert.equal(first.doc.rev, 1);

  const second = applyOps(first.doc, [{ op: "delete", id: "b2" }]);
  assert.deepEqual(second.doc.elements.map((element) => element.id), ["b1"]);

  const third = applyOps(second.doc, [{ op: "add", element: { type: "text", x: 1, y: 1, text: "hi" } }]);
  assert.deepEqual(third.added, ["t4"]);
});

test("a failing op applies nothing", () => {
  const doc = emptyDoc("t");
  assert.throws(() =>
    applyOps(doc, [
      { op: "add", element: { type: "box", x: 0, y: 0, w: 10, h: 10 } },
      { op: "delete", id: "b99" },
    ]),
  );
  assert.equal(doc.elements.length, 0);
});

test("attached arrow ends snap to box borders and follow a move", () => {
  const { doc } = applyOps(emptyDoc("t"), [
    { op: "add", element: { type: "box", x: 0, y: 0, w: 100, h: 100 } },
    { op: "add", element: { type: "box", x: 300, y: 0, w: 100, h: 100 } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "b1", to: "b2" } },
  ]);
  const arrow = doc.elements[2];
  assert.equal(arrow.type, "arrow");
  if (arrow.type !== "arrow") return;
  assert.deepEqual(arrowEnds(arrow, doc.elements), { x1: 100, y1: 50, x2: 300, y2: 50 });

  const moved = applyOps(doc, [{ op: "move", id: "b2", dx: 0, dy: 200 }]).doc;
  const movedArrow = moved.elements[2];
  assert.equal(movedArrow.type, "arrow");
  if (movedArrow.type !== "arrow") return;
  const ends = arrowEnds(movedArrow, moved.elements);
  assert.equal(ends.x2, 300);
  assert.ok(ends.y2 > 200 && ends.y2 < 300);
  assert.match(exportText(moved), /^a3 arrow .* b1->b2$/m);
});

test("update sets a field the element does not have yet, and refuses a wrong one", () => {
  const { doc } = applyOps(emptyDoc("t"), [
    { op: "add", element: { type: "box", x: 10, y: 10, w: 100, h: 50 } },
  ]);
  const labelled = applyOps(doc, [
    { op: "update", id: "b1", patch: { label: "web", color: "failed" } },
  ]).doc;
  assert.match(exportText(labelled), /b1 box 10,10 100x50 failed "web"/);

  assert.throws(
    () => applyOps(labelled, [{ op: "update", id: "b1", patch: { text: "nope" } }]),
    /b1 is a box; it has no "text"/,
  );
});

test("a batch names what it creates and connects it in the same call", () => {
  const { doc, added } = applyOps(emptyDoc("t"), [
    { op: "add", element: { type: "box", x: 0, y: 0, w: 200, h: 100, label: "web" }, as: "web" },
    { op: "add", element: { type: "box", x: 500, y: 0, w: 200, h: 100, label: "db" }, as: "db" },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "web", to: "db" } },
    { op: "update", id: "web", patch: { color: "working" } },
  ]);
  assert.deepEqual(added, ["b1", "b2", "a3"]);
  assert.match(exportText(doc), /^a3 arrow .* b1->b2$/m);
  assert.match(exportText(doc), /^b1 box .* working "web"$/m);

  assert.throws(
    () => applyOps(emptyDoc("t"), [{ op: "update", id: "ghost", patch: { color: "failed" } }]),
    /no element ghost/,
  );
});

test("an arrow with no heads reads as a line and keeps its attachment", () => {
  const { doc } = applyOps(emptyDoc("t"), [
    { op: "add", as: "a", element: { type: "box", x: 0, y: 0, w: 200, h: 100 } },
    { op: "add", as: "b", element: { type: "box", x: 500, y: 0, w: 200, h: 100 } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "a", to: "b", heads: "none" } },
    { op: "add", element: { type: "arrow", x1: 10, y1: 300, x2: 400, y2: 300, heads: "both" } },
  ]);
  const text = exportText(doc);
  assert.match(text, /^a3 line 200,50 -> 500,50 b1->b2$/m);
  assert.match(text, /^a4 arrow 10,300 -> 400,300 heads both$/m);

  const plain = applyOps(doc, [{ op: "update", id: "a4", patch: { heads: "none" } }]).doc;
  assert.match(exportText(plain), /^a4 line /m);
});

test("a long box label stays the authored size and reports clipped", () => {
  const { doc } = applyOps(emptyDoc("t"), [
    {
      op: "add",
      element: {
        type: "box",
        x: 0,
        y: 0,
        w: 200,
        h: 50,
        label: "Read Entra Graph signInActivity which tenants cannot buy",
      },
    },
  ]);
  const box = doc.elements[0];
  assert.equal(box?.type, "box");
  if (box?.type !== "box") return;
  assert.equal(box.h, 50);
  assert.match(layoutReport(doc), /b1 label clipped/);
});

test("no element ever carries an undefined value", () => {
  const { doc } = applyOps(emptyDoc("t"), [
    { op: "add", as: "a", element: { type: "box", x: 0, y: 0, w: 100, h: 100 } },
    { op: "add", as: "b", element: { type: "box", x: 400, y: 0, w: 100, h: 100 } },
    { op: "add", element: { type: "arrow", x1: 10, y1: 200, x2: 300, y2: 200 } },
    { op: "add", element: { type: "arrow", x1: 0, y1: 0, x2: 0, y2: 0, from: "a", to: "b" } },
  ]);
  const detached = applyOps(doc, [{ op: "update", id: "a4", patch: { from: null, to: null } }]).doc;
  for (const element of detached.elements) {
    for (const [key, value] of Object.entries(element)) assert.notEqual(value, undefined, `${element.id}.${key} is undefined`);
  }
  assert.equal(JSON.parse(JSON.stringify(detached)).elements.length, 4);
});

test("moves and patches cannot push a box past the world boundary", () => {
  const { doc } = applyOps(emptyDoc("edge"), [{ op: "add", element: { type: "box", x: 999900, y: 0, w: 100, h: 50 } }]);
  assert.throws(() => applyOps(doc, [{ op: "move", id: "b1", dx: 1, dy: 0 }]), /within/);
  assert.throws(() => applyOps(doc, [{ op: "update", id: "b1", patch: { w: 101 } }]), /within/);
  assert.equal(doc.elements[0].x, 999900);
});

test("only the six Workbench colors are accepted", () => {
  for (const color of ["ink", "accent", "muted", "working", "needs", "failed"]) {
    assert.doesNotThrow(() => applyOps(emptyDoc("color"), [{ op: "add", element: { type: "text", x: 0, y: 0, text: "a", color } }]));
  }
  assert.throws(() => applyOps(emptyDoc("color"), [{ op: "add", element: { type: "text", x: 0, y: 0, text: "a", color: "red" } }]), /unknown color/);
});

test("bounds handle many pen points without overflowing the argument stack", () => {
  const elements = Array.from({ length: 200 }, (_, ix) => ({ type: "path", id: `p${ix + 1}`, points: Array.from({ length: 2000 }, (_, x) => [x, ix]) }));
  assert.deepEqual(contentBounds({ ...emptyDoc("large"), elements }), { x: 0, y: 0, w: 1999, h: 199 });
});
