# Keyboard shortcuts

On Linux and Windows, Cmd in this book means Ctrl+Shift. For example, Cmd+N means Ctrl+Shift+N.

Quit (Cmd+Q) and Close (Cmd+W) are only on macOS. On Linux and Windows, Ctrl+Shift+C copies terminal text and Ctrl+Shift+V pastes it. Ctrl alone always goes to the terminal program. On macOS, Alt is the Option key.

## Window keys

| Key | Action |
| --- | --- |
| Cmd+N | New thread |
| Cmd+. | Stop thread |
| Cmd+Shift+S | Start again |
| Cmd+Shift+R | Restore thread |
| Cmd+Shift+M | Thread actions |
| Cmd+Shift+O | Add project |
| Cmd+Shift+Backspace | Archive thread |
| Cmd+Shift+P | Pin or unpin thread |
| Cmd+1 to Cmd+9 | Go to thread 1 to 9 |
| Cmd+Shift+] and Cmd+Shift+[ | Next thread, previous thread |
| Cmd+Alt+P | Project folder on the New thread screen |
| Cmd+Alt+W | New worktree on the New thread screen |
| Cmd+Alt+R | Retry branches on the New thread screen |
| Cmd+Alt+L | Retry programs on the New thread screen |
| Cmd+Alt+C | Custom command on the New thread screen |
| Cmd+T | New terminal in the thread's folder |
| Cmd+Alt+] and Cmd+Alt+[ | Move to the next terminal, the previous terminal |
| Cmd+Shift+W | Close the shown extra terminal |
| Cmd+Alt+O | Open the newest local link, such as localhost:3000 |
| Cmd+click (Ctrl+click on Linux and Windows) | Open the link under the pointer |
| Cmd+Alt+J | Next thread that needs you |
| Cmd+Shift+L | Move the keyboard focus to the left sidebar |
| Cmd+B and Cmd+Backslash | Show or hide the left or right sidebar |
| Cmd+Shift+A | Command list |
| Cmd+Alt+F | Search threads |
| Cmd+Enter | Send review |
| Cmd+F | Find in diff |
| Alt+] and Alt+[ | Next change, previous change |
| Alt+Z | Wrap lines |
| Cmd+G and Cmd+Alt+G | Next match, previous match in a review search |
| Cmd+, | Settings |
| Cmd+= and Cmd+- | Bigger or smaller terminal text |
| Cmd+0 | Actual terminal text size |
| Cmd+Alt+B | Use the built-in thread list |
| Cmd+Alt+V | Use the built-in review |

In the left sidebar, the Up and Down keys move between rows. Enter or Space opens a thread. In the terminal, the Up and Down keys go to the program.

[Review changes](review.md#review-keys) lists the review keys.

## Change a key

1. Open Settings → Keyboard.
2. Click **Change** or **Bind** for a command.
3. Press the new key.

A key must use Cmd, Alt, Ctrl+Shift, or an F key. The terminal needs the plain keys. kagan does not save a key that belongs to a different command.

From a terminal:

```bash
kg keymap get
kg keymap set <id>:<command> <key>
kg keymap remove <id>:<command>
```

Settings → Keyboard lists plugin commands under the plugin name.
