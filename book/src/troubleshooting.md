# Troubleshooting

Start here when the window or `kg` does not do what you expected.

## `kg` is not found

The macOS download does not put `kg` on your `PATH`. [Add the command link](start.md#download), or install with Homebrew.

## A program does not start

When `kg` reports `program claude was not found`, install that program or select a different program.

When a setup script fails, read the error on the New thread screen. Correct the script, and click **Retry**. The program does not start until the script is successful.

## kagan does not remove a worktree

The worktree has files that git does not ignore. Commit the named files, delete them, or ignore them. Then click **Remove again**. The branch and its commits stay.

## Review comments do not reach the program

This can occur after the program stops. Click **Start again**, and then send the review again. Your comments stay after **Not sent**.

## A plugin fails

In Settings → Plugins, open **Details…**, and then **Logs**. A tool or a page that fails shows **Logs**, **Reload**, and **Disable**.

```bash
kg plugin logs <id> --follow
```

That command prints new log lines until you stop it.

When a plugin draws the thread list or the review, kagan uses the built-in version after a failure. kagan also uses the built-in version when you disable the plugin. Cmd+Alt+B restores the thread list. Cmd+Alt+V restores the review.

## A theme has an error

[Themes](themes.md#make-your-own-theme) gives the procedure for a theme file that is not correct.

## Report a problem

[Your data and privacy](data.md#diagnostics) gives the procedure for the diagnostics file. Attach that file to a [GitHub issue](https://github.com/kagan-sh/kagan/issues).
