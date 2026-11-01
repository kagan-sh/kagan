<h3 align="center">
  <img src="assets/brand/kagan-app-icon.svg" width="80" alt="kagan" /><br />
  kagan
</h3>

<p align="center">A desktop app for coding with agents</p>

<p align="center">
  <a href="#install">Install</a> · <a href="https://docs.kagan.sh/">Guide</a> · <a href="https://docs.kagan.sh/plugins.html">Plugins</a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/readme/kagan-dark.png" />
    <img src="assets/readme/kagan-light.png" alt="The kagan window: threads on the left, a terminal in the center, and changed files on the right" width="900" />
  </picture>
</p>

kagan is a desktop app for coding with agents. Each thread is a real terminal. The thread runs the agent you already use, such as Claude Code or Codex, in its own folder.

> [!WARNING]
> kagan is early (0.1.0). CI tests it on macOS, Linux, and Windows, but most daily use is on macOS.

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
4. Add the `kg` command to your `PATH`. [Get started](book/src/start.md#install) shows how. To open kagan later, run `kg`.

## First run

1. Click **Add project** in the left sidebar. Select a folder, usually a git repository.
2. The New thread screen opens. Select Claude Code, Codex, or another program, and press Enter.
3. The program starts in a new thread. Type to it as you do in any terminal.
4. Open **Changes** in the right sidebar. Comment on a line, then click **Send review to terminal**. On a narrow window, press Cmd+Backslash to show the right sidebar.

For a git repository, each thread gets its own git worktree by default. A worktree is a second checkout of the repository, so your own folder does not change. [Review changes](book/src/window.md#review-changes) and [Threads](book/src/threads.md) give more.

## What is different

- **Your agent, unchanged.** kagan runs the program you select in a terminal. For Claude Code and Codex, kagan also adds its `kg` tools and a short note, so that the agent can use kagan. Settings → Agents turns this off.
- **Only a person approves.** A program in a thread can use `kg` to start, stop, and archive threads, and to operate the window. It cannot answer an approval. A plugin install, more powers for a plugin, and a thread delete wait for you in the window.
- **Plugins declare their powers.** A plugin is a folder of JavaScript with a manifest. The manifest lists what the plugin can do, such as run named programs or use the network. Each plugin runs in its own JavaScript engine, and kagan gives it only the powers that you approve.
- **No account, no telemetry, no update check.** Your data stays on your computer in `~/.kagan`. kagan itself uses the network only to fetch a git plugin that you install or update. [Your data](book/src/data.md) tells what programs and plugins can do.

A program in a thread runs with your user access and gets your environment variables. Run only programs that you trust.

## Learn more

The [book](https://docs.kagan.sh/) is the user guide. Its source is in [`book/src`](book/src/index.md).

- [Get started](book/src/start.md)
- [Threads](book/src/threads.md) and [The window](book/src/window.md)
- [Your data](book/src/data.md)
- [Plugins](book/src/plugins.md) and [Build a plugin](book/src/build-plugin.md)
- [The `kg` command](book/src/cli.md)

## Develop

```bash
git config core.hooksPath .githooks
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo t --workspace --locked
```

`cargo t` runs the tests with the cargo-nextest version in `Cargo.toml`. Run `cargo install cargo-run-bin` one time first. [`AGENTS.md`](AGENTS.md) has the rules for every change. The spec is in [`docs`](docs/architecture.md).

## License

MIT. See [LICENSE](LICENSE).

The window uses [Inter](assets/fonts/Inter-OFL.txt), and the terminal uses [JetBrains Mono](assets/fonts/JetBrainsMono-OFL.txt). Both fonts use the SIL Open Font License 1.1. The wordmark sets "kagan" in outlines of Inter. [`assets/THIRD-PARTY-NOTICES.txt`](assets/THIRD-PARTY-NOTICES.txt) has the license of each dependency and ships inside the app.
