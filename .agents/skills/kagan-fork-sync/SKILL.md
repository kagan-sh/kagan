---
name: kagan-fork-sync
description: >-
  Merge the latest upstream changes into kagan's forked git dependencies (kagan-sh/gpui-kit,
  kagan-sh/gpui-fast, kagan-sh/wry, or any other kagan-sh repo pinned by rev in kagan's
  Cargo.toml), re-pin kagan, update Cargo.lock, and open a pull request. Also reports which of
  our fork changes upstream has already taken, and when a fork can be dropped for the upstream
  crate. Use this whenever someone asks to sync, update, rebase or catch up the forks, pull
  upstream fixes from longbridge (gpui-kit, gpui-fast, wry), bump the fork pins, check how far
  the forks have drifted, or asks whether kagan can stop using a fork, even if they don't say
  "fork sync".
---

# Sync kagan's forks with upstream

kagan depends on a few kagan-sh forks, pinned by commit in `Cargo.toml` (both
`[workspace.dependencies]` and `[patch.crates-io]`). Dependabot cannot update git pins, so
upstream fixes only arrive when someone merges them by hand. This skill does that merge, re-pins
kagan, and reports how much of each fork is still ours, so the forks shrink over time instead of
drifting.

The pins do not sit on the forks' default branches. A pin is whatever commit kagan builds today,
often on a feature branch such as `webview-release`. So every sync starts from the pin, never
from the fork's `main`.

`.agents/skills/kagan-fork-sync/scripts/fork_sync.py` does the mechanical part. Each step can run again; finished work is kept.
It keeps one clone per fork plus `state.json` in `--work` (default `~/.cache/kagan-fork-sync`).

```
script=.agents/skills/kagan-fork-sync/scripts/fork_sync.py
python3 $script plan     # pins, how far behind, what is still ours
python3 $script merge    # sync branch from the pin + upstream merge
python3 $script push     # push the sync branches to the forks
python3 $script pin      # re-pin kagan and update Cargo.lock
```

Run them from the kagan checkout, or pass `--repo <kagan checkout>`.

It needs `git`, `gh` logged in with push rights on kagan-sh, and network access to github.com
and api.github.com.

## 1. Plan

Run `plan` and read its JSON. For each fork it gives:

- `behind`: upstream commits the pin lacks. Zero means nothing to merge.
- `drift.our_commits`: our commits upstream lacks. `upstream_has_it: true` means upstream already
  holds the same patch.
- `drift.files_differing_from_upstream`: files we still change.
- `drift.can_switch_to_upstream`: true when nothing of ours is left.

If every fork has `behind: 0`, report that and stop.

## 2. Merge

Run `merge`. It creates `kagan/upstream-sync-<date>` at each pin and merges the upstream default
branch with a merge commit. It exits 2 when a fork has conflicts. The JSON then names the files
and the clone path.

To resolve a conflict, first learn what our side was for. Run
`git log --oneline <pin> --not upstream/<branch> -- <file>` in the clone and read those commits.
Keep upstream's new structure and carry our fix into it. If upstream already fixed the same thing
another way, take upstream's version and say so in the report: that change is one less to carry.
For `Cargo.lock`, take upstream's copy (`git checkout --theirs Cargo.lock`), let cargo settle it
during the build check, and commit. Then `git commit --no-edit` and run `merge` again. It sees
the merge is done and records the new head.

When you are not sure what a conflicted change is for, do not guess. Run `git merge --abort` for
that fork, leave its pin as it is, sync the others, and list the fork as blocked in the PR with
the conflicted files.

Before pushing, check that each merged fork still compiles for the crates kagan uses. Run
`cargo check -p <crate>` in the clone (the crate names are `crates` in `state.json`). A compile
error found only in kagan costs another round.

## 3. Push

Run `push`. It pushes new branches only. It refuses to overwrite a branch that exists at another
commit. Never force-push, and never delete a `kagan/upstream-sync-*` branch: kagan's `Cargo.lock`
points at it, and a deleted branch can make the pinned commit unfetchable.

## 4. Re-pin kagan

In the kagan checkout, branch from a fresh `origin/main` (for example
`chore/fork-sync-<date>`), then run `pin`. It rewrites every `rev` for the merged forks and runs
`cargo update` for them. It also fails when a `[patch.crates-io]` entry stops applying. That
happens when upstream moved to a crate version our fork does not provide, and kagan would then
quietly build the crates.io copy without our fixes.

Then run kagan's own gate from its `AGENTS.md`: `cargo fmt --check`, clippy with `-D warnings`,
`cargo t --workspace --locked`, and the size budget. A changed `Cargo.lock` also needs
`sh scripts/third-party-notices.sh`, or the notices test fails. Fix kagan code that upstream API
changes broke, in the same branch.

## 5. Switch back to upstream when a fork is no longer needed

When `drift_after_merge.can_switch_to_upstream` is true, the fork no longer changes anything.
Point kagan at upstream instead:

- If upstream has published a crates.io release that contains the merged upstream commit, use
  that version. Check `https://crates.io/api/v1/crates/<crate>` and the upstream tags. Drop a
  `[patch.crates-io]` entry entirely when the release covers it.
- Otherwise pin the upstream git repo (`https://github.com/<upstream>`) at that commit.

When only some commits are left (`still_ours` is small), name them in the PR. Each is a
candidate for a pull request upstream, and every one upstream takes brings the switch closer.

## 6. Open the pull request

Open it the way your setup requires (in bb, `signoff_ask` with kind `pr`). Title:
`chore(deps): merge upstream into the forks (<date>)`. Body:

```
Upstream fixes for the forked dependencies.

| Fork | Upstream | Commits merged | Our changes left | New pin |
|---|---|---|---|---|
| gpui-kit | longbridge/gpui-kit@main | 87 | 29 commits, 37 files | <sha> on kagan/upstream-sync-<date> |

Upstream now has: <our commits where upstream_has_it is true, or "none">
Can switch to upstream: <forks with can_switch_to_upstream, or "none yet">
Blocked: <forks left at their old pin, with the conflicted files, or "none">
Conflicts resolved: <file: what we kept and why>

Checked: fmt, clippy, tests, size budget, notices.
```

## Running it on a schedule

To catch upstream fixes regularly, run this monthly, for example from a scheduled agent whose
prompt is "sync kagan's forks with upstream". The skill stops at the PR, so a person still reviews
every sync.
