---
name: canvas
description: Read and draw the side-panel canvas with canvas_read, canvas_edit, or kg canvas. Use when the user mentions a canvas or asks for a diagram.
---

# Canvas

The Canvas plugin gives the user a drawing panel next to the thread. The user
draws boxes, arrows, text, and freehand strokes. You see the same drawing as
text through `kg canvas show <name>` or the `canvas_read` tool. You change it
through `canvas_edit`. Every change shows in the panel at once.

## Model

- One canvas has a name. The user picks it in the panel. A message that shares a
  canvas names it and gives the command to read it: `kg canvas show my-project`.
- Coordinates range from −1,000,000 to 1,000,000, negative included. `y` grows
  down. The user pans and zooms, and Zoom to fit (Shift+1) puts the whole
  drawing on screen, so nothing gets lost.
- `canvas_read` starts with a summary line saying how many elements there are
  and which area they use. Place new work beside that area, not on top of it.
  On an empty canvas, draw near `0,0`.
- Each element has a stable id: `b1` box, `a2` arrow, `t3` text, `p4` freehand
  path. Ids only increase. A deleted id is never reused.
- An arrow with `from` and `to` (two box ids) sticks to those boxes. It routes
  on right angles and follows the boxes. Delete a box and its arrows go too.
- `heads` says which ends carry an arrowhead: `end` (the default), `start`,
  `both`, or `none`. An arrow with `heads: "none"` is a plain line, and
  `canvas_read` prints it as `line`.
- Colors: `ink accent muted working needs failed`. The active Workbench theme resolves them; omitted color is `ink`. Theme roles may share a color, so use labels as well.

## Draw a readable diagram

The panel clips a label to the box. A long sentence in a small box is lost.
Keep the picture small and sparse.

- A box label is a noun phrase: `"web"`, `"coach-tools"`, `"blocked"`. Not a
  sentence. Not a paragraph.
- Size the box to the label: `w = max(200, chars * 10 + 24)`, `h = 80`. A
  wrapped label can use `h = 80 + 20 * extra lines`.
- Leave a gap of 80 between boxes. Align a row on one `y`. Align a column on
  one `x`.
- Bind every arrow with `from` and `to`, and set `x1`, `y1`, `x2`, and `y2` to `0`. Do not compute endpoints. The panel also binds an arrow the person draws when the pointer ends on or just beside a box.
- Put a title or a note in `text` above or beside the row. Never put a title
  inside a large empty box that other boxes sit in: the title lands in the
  middle and covers them.
- One color per lane or per row. Skip an arrow label unless it names the
  relation in a few characters (`"GraphQL"`, `"blocks"`).
- Keep the whole diagram inside about 1600 by 1200 so it fits the panel at one
  glance.

Do not:

- Dump an essay into a box.
- Stack labels on the same point.
- Draw a diagonal arrow across another row. Bind the arrow; the route stays
  orthogonal.
- Overlap boxes that are not nested. A lane box may contain cards. `canvas_edit`
  reports `layout: b1 overlaps b2` only when two boxes collide without nesting.

## Read

`kg canvas show <name>` and `canvas_read` with `name` print the same text: a
summary line, then one line per element:

```
b1 box 100,80 300x80 accent "web server"
b2 box 100,240 300x80 "database"
a3 arrow 250,160 -> 250,240 b1->b2 "writes"
a6 line 700,300 -> 900,300
t4 text 100,40 "notes"
p5 freehand 42 points, bounds 900,200 to 1100,380 failed
```

`kg canvas list` and `canvas_read` without `name` list the canvases. The open
canvas is usually the project name, such as `personal`, not a name you invent.
Take the name from the user's message, or from that list.

A canvas must already exist. If the name is wrong, both tools fail and list the
real names. Names ignore case and spaces: `My App` and `my-app` are one canvas.
Only the user makes a new canvas, in the panel.

## Edit

`canvas_edit` takes `name` and `ops`. Ops apply in order. If one op fails,
nothing is applied. The result has the summary line, the new and deleted ids,
and only the element lines the edit changed. Read the canvas for the rest.

```json
{
  "name": "my-project",
  "ops": [
    { "op": "add", "element": { "type": "box", "x": 100, "y": 80, "w": 300, "h": 80, "label": "web", "color": "accent" } },
    { "op": "add", "element": { "type": "box", "x": 100, "y": 240, "w": 300, "h": 80, "label": "db" } },
    { "op": "add", "element": { "type": "arrow", "x1": 0, "y1": 0, "x2": 0, "y2": 0, "from": "b1", "to": "b2", "label": "writes" } },
    { "op": "update", "id": "b1", "patch": { "color": "working" } },
    { "op": "move", "id": "b2", "dx": 0, "dy": 50 },
    { "op": "delete", "id": "t4" }
  ]
}
```

### Name what you create

An id exists only after the op runs, so give a new element an alias with `as`
and use that alias later in the same call. This is how you connect two boxes
you are drawing right now.

```json
{
  "name": "my-project",
  "ops": [
    { "op": "add", "as": "web", "element": { "type": "box", "x": 100, "y": 80, "w": 220, "h": 80, "label": "web" } },
    { "op": "add", "as": "db", "element": { "type": "box", "x": 400, "y": 80, "w": 220, "h": 80, "label": "db" } },
    { "op": "add", "element": { "type": "arrow", "x1": 0, "y1": 0, "x2": 0, "y2": 0, "from": "web", "to": "db" } }
  ]
}
```

An alias starts with a letter, holds letters, digits, `_` and `-`, must not
look like an id (`b1`), and is unique in the call. It lives only for that call:
the result tells you the real ids. Anywhere an id is taken, an alias works too.

`clear` removes everything: `{ "op": "clear" }`. Ask before you use it. The user
can undo any change with the Undo button, one step at a time.

An arrow held by `from` and `to` cannot be moved; move its boxes instead.

## Procedure

1. When the user shares the canvas, the message names it but does not hold
   the picture. Run `kg canvas show <name>` first. Say in one or two sentences
   what you see. If they did not say what to do, ask.
2. Read the canvas again right before you edit. The user keeps drawing while
   you work.
3. Put the whole change in one `canvas_edit` call. The user sees one update.
   The reply includes `added:` with the real ids. Later calls use those ids,
   not the aliases from this call.
4. Keep a new diagram compact and beside the existing work, so the user sees
   it after one Zoom to fit.
5. Do not redraw what already exists. Move or update by id.
6. If the result includes a `layout:` line, fix those overlaps or clipped
   labels in the next edit.
