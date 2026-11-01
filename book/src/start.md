# Get started

## Install

On an Apple silicon Mac, download the app or use Homebrew. Otherwise, build kagan from source. [Releases](https://github.com/kagan-sh/kagan/releases) also have a Windows installer, a `.deb`, and an AppImage.

### Download

1. Download the signed `.dmg` from [Releases](https://github.com/kagan-sh/kagan/releases).
2. Open the file and drag `kagan.app` to `/Applications`.
3. Add `kg` to your `PATH`:

   ```bash
   sudo mkdir -p /usr/local/bin
   sudo ln -sf /Applications/kagan.app/Contents/MacOS/kg /usr/local/bin/kg
   ```

### Homebrew

```bash
brew install --cask kagan-sh/tap/kagan
```

Homebrew puts `kagan.app` in `/Applications` and `kg` on your `PATH`.

### Build from source

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
4. Add the `kg` command to your `PATH`. On macOS and Linux, add this line to your shell profile:

   ```bash
   export PATH="$HOME/kagan/target/release:$PATH"
   ```

   If you cloned kagan to a different folder, change `$HOME/kagan` to that folder. On Windows, add the `target\release` folder to your `Path`.

Run `kg` to open kagan later.

## First thread

1. Click **Add project** in the left sidebar and select a folder.
2. Select Claude Code, Codex, or another program on the New thread screen.
3. Press Enter.
4. Type to the program as you do in any terminal.
5. Click **Changes** in the right sidebar to see the changed files.

For a git repository, the thread gets its own git worktree by default. Your project folder does not change. [Threads](threads.md#folders) explains the folder choices.

[Review changes](review.md) shows how to send comments to the program. On a narrow window, press Cmd+Backslash to show the right sidebar.

## Use `kg` from a terminal

Open kagan, then run:

```bash
# Check that kg works
kg project list
kg guide
kg help
```

`kg project list` shows your projects and their ids. `kg guide` explains kagan's terms. `kg help` lists all commands.

To add a project and start a thread:

```bash
kg project add ~/code/my-app
kg thread run --project-id <project-id> --program claude
```

`kg project add` shows the id of the new project. If your computer does not have the program, the command fails with `program claude was not found`.
