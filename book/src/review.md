# Review changes

Review is how you read what a thread changed and send comments back to its program. **Changes** in the right sidebar lists those files. In a git folder, **Uncommitted** is the work not yet committed, and **Branch** is this branch against its base. Click a file, or **Open review**, to open the diff in the center. The review can also show **Last commit**. A folder that is not git says **Not a git folder.**

## Send comments to the program

1. Click a file.
2. Select a line, or drag across the line numbers.
3. Type a comment.
4. Click **Send review to terminal**, or press Cmd+Enter.

kagan pastes the comments into the program. It does not press Enter. Edit them, then press Enter to send them.

If the program cannot take the comments, the terminal shows **Not sent**. Your comments stay.

**Discard comments** deletes comments you did not send.

## Preview a file

On the review, **Preview** opens that file in a **Preview** tab in the right sidebar. Nothing in the tab is edited or saved. It shows text, Markdown, CSV, a PNG, or a JPEG. A file that is too large, or another kind, says why and shows nothing else. Markdown does not load images. An http or https link opens in the system browser.

```bash
kg preview show <path>
```

A relative path stays inside the thread's folder. Switching threads closes the tab.

## Review keys

| Key | Action |
| --- | --- |
| J, K | Next file, previous file |
| N, P | Next change, previous change |
| Alt+] and Alt+[ | Next change, previous change |
| V | Mark the file as viewed |
| Cmd+F | Find in diff |
| Alt+Z | Wrap lines |
| Cmd+G, Cmd+Alt+G | Next match, previous match in a review search |
| Escape | Return to the terminal |

[Keyboard shortcuts](keys.md) lists the window keys.
