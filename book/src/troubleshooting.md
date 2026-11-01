# Troubleshooting

## `kg` is not found

The macOS download does not add `kg` to your `PATH`. [Add the command link](start.md#download), or install kagan with Homebrew.

## A program does not start

If `kg` reports `program claude was not found`, install the program or select a different program.

If a setup script fails, read the error on the New thread screen. Correct the script, then click **Retry**. The program does not start until the script succeeds.

## kagan does not remove a worktree

The worktree has files that git does not ignore. Commit, delete, or ignore the named files, then click **Remove again**. The branch and its commits stay.

## Review comments do not reach the program

This error can occur after the program stops. Click **Start again**, then send the review again. kagan keeps your comments after **Not sent**.

## A plugin fails

In Settings → Plugins, click **Details…** for the plugin, then click **Logs**. A failed plugin card in the right panel also has **Logs**.

From a terminal:

```bash
kg plugin logs <id> --follow
```

The command prints new log lines until you stop it.

If a plugin draws the thread list or the review, kagan returns to the built-in version after a failure or when you disable it. Press Cmd+Alt+B to restore the thread list or Cmd+Alt+V to restore the review yourself.

## A theme has an error

[Themes](themes.md#make-your-own-theme) covers theme errors.

## Report a problem

[Your data and privacy](data.md#diagnostics) explains the diagnostics file. Attach the file to a [GitHub issue](https://github.com/kagan-sh/kagan/issues).
