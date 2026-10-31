# Canvas

A shared native drawing panel for people and agents. The plugin owns the model,
history, tools and controls in plain JavaScript; `SketchScene` retains the drawing
in GPUI. There is no build step.

## Install

From the repository root:

```sh
kg plugin check plugins/canvas
kg plugin install plugins/canvas
```

Or install from git:

```sh
kg plugin install git:https://github.com/kagan-sh/kagan.git@main --plugin canvas
```

Approve the clipboard, threads and database powers in kagan when prompted.
The install command waits for that decision. Git installs are locked to the
resolved commit until `kg plugin update dev.kagan.canvas`.

## Draw and share

Open **Canvas** as a center page, or press Cmd+Shift+D. The default canvas
is named after the project. The menu switches, creates, clears or deletes canvases.
Floating controls leave the drawing surface available underneath them.

| Key | Action |
| --- | --- |
| V / 1 | Select, move and resize |
| H | Hand |
| R / 2 | Box; Shift makes a square |
| A / 5 | Arrow; draw between boxes to bind it |
| L / 6 | Line |
| P / 7 | Pen |
| T / 8 | Text |
| E / 0 | Eraser |
| Q | Keep the tool after drawing |
| Space+drag, middle drag, wheel | Pan |
| Modifier+wheel, Cmd+plus/minus | Zoom |
| Shift+1 | Fit the drawing |
| Cmd+A / D | Select all / duplicate |
| Cmd+C / X / V | Copy / cut / paste |
| Cmd+Z / Shift+Z | Undo / redo |
| Arrow keys | Nudge; Shift increases the step |
| Double-click | Edit a label |
| Escape | Cancel a gesture or dismiss controls |

A drag ends when it leaves the surface. The six stroke colors are `ink`,
`accent`, `muted`, `working`, `needs`, and `failed`, resolved by the active theme.
Undo keeps 25 steps, including agent edits. Send posts a note and
`kg canvas show <name>` to the open thread; the picture is read only when asked.

```sh
kg canvas list
kg canvas show my-project
node --test plugins/canvas/*.test.js
```

Agents use `canvas_read`, `canvas_edit`, and the packaged canvas skill. Edits are
atomic; failed batches leave the drawing unchanged. A canvas must exist before
an agent edits it.
