# Get started

## The macOS developer preview

[Releases](https://github.com/kagan-sh/kagan/releases) are published when someone starts the Release workflow from `main` and types the version. Increase the middle number for features, the last number for fixes, and the first number for breaking changes. The macOS file is a `.dmg`. It is unsigned and not notarized unless the Apple secrets are set: `APPLE_CERTIFICATE` signs it, and `APPLE_ID` notarizes it. macOS will warn when you open an unsigned copy. That warning is the current preview. Removing the quarantine flag is not a finished release.

On an Apple silicon Mac, Homebrew installs the latest release:

```bash
brew install --cask kagan-sh/tap/kagan
```

It copies `kagan.app` to `/Applications` and links `kg` on your `PATH`. `brew upgrade --cask kagan` updates it. Prereleases are not in the tap.

Without Homebrew, download the disk image and `SHA256SUMS`. Check the file before you open it:

```bash
gh release download v0.1.0 --repo kagan-sh/kagan --pattern SHA256SUMS --pattern '*.dmg'
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Open the disk image and copy `kagan.app` to `/Applications`. To update, download the newer disk image and replace `/Applications/kagan.app`. The app has no updater.

`kg` is the binary inside the app. The disk image does not put it on your `PATH`. Add it yourself:

```bash
ln -sf /Applications/kagan.app/Contents/MacOS/kg /usr/local/bin/kg
```

A `kg` you run from a terminal talks to the open app. On macOS the socket is `~/Library/Application Support/kagan/kg.sock` unless `KG_SOCKET` is set.

The app includes the license texts of its dependencies and of Inter and JetBrains Mono. In the bundle that file is `kagan.app/Contents/Resources/THIRD-PARTY-NOTICES.txt`.

The same workflow also builds a Windows installer, a `.deb`, and an AppImage. The Windows installer is unsigned unless `WINDOWS_CERTIFICATE` is set. The Linux packages are unsigned.

## Build and run

Install Rust. This repo pins the compiler in `rust-toolchain.toml`.

```bash
cargo run -p kg
```

That opens the window. `cargo run -p kg --release` builds a faster binary.

The app keeps its data in `~/.kagan`. Set `KG_DATA_DIR` to use another folder. On Windows the default is `%USERPROFILE%\.kagan`.

The first launch creates a Personal project in that folder. Personal is for work that is not a git repository. Its id is `personal`.

## First thread

In the window, push Add project, pick a folder, pick a program, and start the thread.

From a terminal, with the app running or not (the command starts it):

```bash
# first-run
kg project list
kg thread run --project-id personal --environment personal --program /usr/bin/true
kg guide
kg help
```

`kg project list` prints the projects. Personal has no git repository, so the thread uses `--environment personal`. `/usr/bin/true` exits at once, so you can see a thread without an agent. For Claude Code, use `--program claude`. A missing program stops with `program claude was not found`.

[Threads](threads.md) covers the folder and stop.

`kg guide` prints the names this app uses. `kg guide threads` prints one chapter. `kg help` lists every command.
