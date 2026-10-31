# kagan

kagan is a desktop app for coding with agents. Each thread is a real terminal that runs the agent you already use, such as Claude Code or Codex, in its own folder or git worktree.

<!-- Screenshot: replace with a current capture of the window, light and dark. -->
<p align="center"><em>Screenshot coming soon.</em></p>

> [!WARNING]
> kagan is early (0.1.0). Expect rough edges. It is built and tested mostly on macOS; Linux and Windows builds exist and get less testing.

## Install

There are no published releases yet. Build from source.

You need Rust; `rust-toolchain.toml` picks the version. The first build takes several minutes.

- **macOS:** `brew install lld`. The build links with `ld64.lld`.
- **Linux (Debian, Ubuntu):** `sudo apt-get install build-essential pkg-config libfontconfig-dev libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev libvulkan1 libasound2-dev`

```bash
git clone https://github.com/kagan-sh/kagan.git
cd kagan
cargo run -p kg --release
```

That opens the window. The binary is `target/release/kg`. Put it on your `PATH` to use `kg` from a terminal.

## First run

1. Push **Add project** and pick a folder, usually a git repository.
2. Pick a program: Claude Code, Codex, or any command. Push Enter.
3. The program starts in a new thread, in a real terminal. Type to it as you always do.
4. Open **Changes** on the right to see what it changed. Comment on a line, then send the review back to the terminal.

The same works from a terminal. `kg help` lists every command, and `kg guide` explains the terms.

## What is different

- **Your agent, unchanged.** kagan does not wrap or replace the agent. It runs the program you pick in a terminal and keeps the threads, folders, and changes in one window.
- **Only a person approves.** A program in a thread can drive kagan through `kg`, but it cannot answer an approval. Installing a plugin, granting it more powers, and deleting a thread wait for a person in the window.
- **Plugins declare their powers.** A plugin is a folder of JavaScript with a manifest. The manifest lists what it may do, such as run named programs, use the network, or read named folders. kagan reads it before it runs any plugin code, and asks again when an update wants more.
- **No account, no telemetry.** Your data stays on your computer, in `~/.kagan` (`%USERPROFILE%\.kagan` on Windows). kagan uses the network itself only to fetch a git plugin you install or update. The agents you run, and plugins you allow, may use the network on their own.

## Learn more

The [book](https://docs.kagan.sh/) covers daily use, plugins, and the spec. Its source is in [`book/src`](book/src/index.md).

- [Get started](book/src/start.md)
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

`cargo t` runs the tests with the cargo-nextest version pinned in `Cargo.toml`; it needs `cargo install cargo-run-bin` once. Read [`AGENTS.md`](AGENTS.md) for the rules every change follows.

## License

MIT. See [LICENSE](LICENSE).

The mark is a KG monogram drawn for kagan. The wordmark sets "kagan" beside it in outlines of [Inter](assets/fonts/Inter-OFL.txt). The window uses Inter, and the terminal uses [JetBrains Mono](assets/fonts/JetBrainsMono-OFL.txt). Both fonts are under the SIL Open Font License 1.1. [`assets/THIRD-PARTY-NOTICES.txt`](assets/THIRD-PARTY-NOTICES.txt) holds those texts and the licenses of every dependency, and ships inside the app.
