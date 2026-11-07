# Get started

Install kagan, open a project, and start a thread. The thread runs one program, such as Claude Code, Codex, or a shell, in one terminal. You review the files it changes in the same window.

## Install

On a Mac with macOS 11 or later, Intel or Apple silicon, download the app or use Homebrew. Otherwise, build from source. [Releases](https://github.com/kagan-sh/kagan/releases) also have a Windows installer, and a `.deb` and an AppImage for x86_64 and aarch64. The Linux app needs WebKitGTK 4.1 and `libxkbcommon-x11.so.0` (`libwebkit2gtk-4.1-0` and `libxkbcommon-x11-0` on Debian and Ubuntu). The `.deb` asks for both. The AppImage uses those same libraries from the system.

### Download

1. Download the signed `.dmg` from [Releases](https://github.com/kagan-sh/kagan/releases).
2. Open it and drag `kagan.app` to `/Applications`.
3. Add `kg` to your `PATH`:

   ```bash
   sudo mkdir -p /usr/local/bin
   sudo ln -sf /Applications/kagan.app/Contents/MacOS/kg /usr/local/bin/kg
   ```

### Homebrew

```bash
brew install --cask kagan-sh/tap/kagan
```

That puts `kagan.app` in `/Applications` and `kg` on your `PATH`.

### Build from source

1. Install Rust from [rustup.rs](https://rustup.rs). `rust-toolchain.toml` sets the version.
2. Install the build tools:
   - macOS: `brew install lld`
   - Debian or Ubuntu:

     ```bash
     sudo apt-get install build-essential pkg-config \
       libfontconfig-dev libwayland-dev libxkbcommon-x11-dev \
       libx11-xcb-dev libssl-dev libzstd-dev libvulkan1 libasound2-dev \
       libwebkit2gtk-4.1-dev libgtk-3-dev
     ```

   - Windows: Visual Studio Build Tools with C++, and MinGW for `dlltool` (`choco install mingw`).
3. Build and open kagan. The first build takes several minutes, then the window opens.

   ```bash
   git clone https://github.com/kagan-sh/kagan.git
   cd kagan
   cargo run -p kg --release
   ```

4. Put `kg` on your `PATH`. On macOS and Linux, add this to your shell profile. If the clone is not `$HOME/kagan`, change that path. On Windows, add the `target\release` folder to your `Path`.

   ```bash
   export PATH="$HOME/kagan/target/release:$PATH"
   ```

Run `kg` to open kagan later. On first start, kagan makes `~/.kagan`.

## First thread

1. Click **Add project** in the left sidebar and select a folder.
2. Select Claude Code, Codex, or another program on the New thread screen.
3. Press Enter.
4. Type to the program as you do in any terminal.
5. Click **Changes** in the right sidebar to see the changed files.

For a git repository, the thread gets its own git worktree by default. Your project folder does not change. [Threads](threads.md#folders) explains the other choices.

[Review changes](review.md) shows how to send comments to the program. On a narrow window, press Cmd+Backslash to show the right sidebar.

When the built-in window is not enough, [install a plugin](plugins.md) or [build one](build-plugin.md).

## Use `kg` from a terminal

Open kagan, then run:

```bash
# Check that kg works
kg project list
kg guide
kg help
```

`kg project list` shows projects and their ids. `kg guide` explains the terms. `kg help` lists every command.

```bash
kg project add ~/code/my-app
kg thread run --project-id <project-id> --program claude
```

`kg project add` prints the new id. If the program is missing, the command fails with `program claude was not found`.
