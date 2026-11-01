# Get started

## Install

There is no release yet. Build kagan from source.

1. Install Rust from [rustup.rs](https://rustup.rs). The file `rust-toolchain.toml` sets the version.
2. Install the build tools for your system:
   - macOS: `brew install lld`
   - Debian or Ubuntu:

     ```bash
     sudo apt-get install build-essential pkg-config \
       libfontconfig-dev libwayland-dev libxkbcommon-x11-dev \
       libx11-xcb-dev libssl-dev libzstd-dev libvulkan1 libasound2-dev
     ```

   - Windows: Visual Studio Build Tools with C++, and MinGW for `dlltool` (`choco install mingw`).
3. Build kagan and open it:

   ```bash
   git clone https://github.com/kagan-sh/kagan.git
   cd kagan
   cargo run -p kg --release
   ```

   The first build takes several minutes. Then the window opens.
4. Add the `kg` command to your `PATH`. On macOS and Linux, run this in the `kagan` folder, and add the same line to your shell profile, such as `~/.zshrc`:

   ```bash
   export PATH="$PWD/target/release:$PATH"
   ```

   On Windows, add the `target\release` folder, which has `kg.exe`, to your `Path`.

To open kagan later, run `kg`.

On the first start, kagan makes its data folder, `~/.kagan`, and a project named Personal. Personal is for work that is not in a git repository. [Your data](data.md) lists all the places where kagan writes.

## First thread

1. Click **Add project** in the left sidebar. Select a folder, usually a git repository.
2. The New thread screen opens. Select Claude Code, Codex, or another program, and press Enter.
3. The program starts in a new thread. Type to it as you do in any terminal.
4. Open **Changes** in the right sidebar to see what the program changed. On a narrow window, press Cmd+Backslash to show the right sidebar. [Review changes](window.md#review-changes) tells you how to send comments back.

For a git repository, the thread gets its own git worktree by default. [Threads](threads.md#folders) tells you about the folder choices.

## Use `kg` from a terminal

`kg` controls the open kagan window. Open kagan first, then try these commands:

```bash
# Check that kg works
kg project list
kg guide
kg help
```

- `kg project list` shows your projects and their ids. Personal is always there.
- `kg guide` explains the words that kagan uses.
- `kg help` lists all commands. [`kg` command](cli.md) has the full list.

To add a project and start a thread in it:

```bash
kg project add ~/code/my-app
kg thread run --project-id <project-id> --program claude
```

`kg project add` shows the id of the new project. If the program is not installed, the command fails with `program claude was not found`.
