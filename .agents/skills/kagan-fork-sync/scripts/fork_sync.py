#!/usr/bin/env python3
"""Merge upstream into kagan's forked git dependencies and re-pin kagan.

Subcommands (run from the kagan checkout, or pass --repo):
  plan          report each fork pin and how far it is behind upstream
  merge         merge upstream into a sync branch that starts at the pin
  push          push the sync branches to the forks
  pin           point kagan's Cargo.toml at the sync branch heads and update Cargo.lock

State lives in --work (default ~/.cache/kagan-fork-sync): one clone per fork and
state.json. Every step can be run again; finished work is kept.
"""

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

GIT_DEP = re.compile(r"https://github\.com/(?P<owner>[^/]+)/(?P<name>[^/\"]+?)(?:\.git)?$")


def run(cmd, cwd=None, check=True, capture=True):
    result = subprocess.run(cmd, cwd=cwd, text=True, capture_output=capture)
    if check and result.returncode != 0:
        sys.exit(f"fork-sync: {' '.join(cmd)} failed\n{result.stdout}{result.stderr}")
    return result


def token():
    return os.environ.get("GH_TOKEN") or run(["gh", "auth", "token"]).stdout.strip()


def github(path):
    out = run(["curl", "-sfL", "-m", "30", "-H", f"Authorization: Bearer {token()}",
               "-H", "Accept: application/vnd.github+json", f"https://api.github.com/{path}"]).stdout
    return json.loads(out)


def forks(repo, org):
    """Every git dependency on an org repo, grouped by repo, with the crates that use it."""
    manifest = tomllib.loads((repo / "Cargo.toml").read_text())
    tables = [manifest.get("workspace", {}).get("dependencies", {})]
    tables += list(manifest.get("patch", {}).values())
    found = {}
    for table in tables:
        for crate, spec in table.items():
            if not isinstance(spec, dict) or "git" not in spec:
                continue
            match = GIT_DEP.match(spec["git"])
            if not match or match["owner"] != org:
                continue
            if "rev" not in spec:
                sys.exit(f"fork-sync: {crate} follows a branch, not a rev; pin it first")
            entry = found.setdefault(match["name"], {"url": spec["git"], "rev": spec["rev"], "crates": []})
            if entry["rev"] != spec["rev"]:
                sys.exit(f"fork-sync: {match['name']} is pinned at two revs; make them one first")
            entry["crates"].append(crate)
    return found


def load_state(work):
    path = work / "state.json"
    return json.loads(path.read_text()) if path.exists() else {}


def save_state(work, state):
    (work / "state.json").write_text(json.dumps(state, indent=2) + "\n")


MARK = "kagan-fork-sync"


def clone(work, org, name, upstream):
    path = work / name
    if not (path / ".git").exists():
        run(["git", "clone", "--filter=blob:none", "--no-checkout",
             f"https://github.com/{org}/{name}.git", str(path)])
        (path / ".git" / MARK).touch()
    if not (path / ".git" / MARK).exists():
        sys.exit(f"fork-sync: {path} is not a clone this script made; point --work at an empty folder")
    remotes = run(["git", "remote"], cwd=path).stdout.split()
    if "upstream" not in remotes:
        run(["git", "remote", "add", "upstream", f"https://github.com/{upstream}.git"], cwd=path)
    run(["git", "fetch", "--quiet", "origin"], cwd=path)
    run(["git", "fetch", "--quiet", "upstream"], cwd=path)
    return path


def count(path, spec):
    return int(run(["git", "rev-list", "--count", spec], cwd=path).stdout.strip())


def drift(path, upstream_ref, head):
    """Our commits on head that upstream lacks, and the files where head still differs from upstream.

    `git cherry` marks a commit `-` when upstream holds a change with the same patch id,
    so a fix upstream took verbatim drops out. A fix upstream rewrote shows as still ours
    until the file diff is empty; read those by hand.
    """
    commits = []
    for line in run(["git", "cherry", upstream_ref, head], cwd=path).stdout.splitlines():
        mark, sha = line.split()
        if len(run(["git", "rev-list", "--parents", "-n1", sha], cwd=path).stdout.split()) > 2:
            continue
        subject = run(["git", "log", "-1", "--format=%s", sha], cwd=path).stdout.strip()
        commits.append({"sha": sha[:12], "subject": subject, "upstream_has_it": mark == "-"})
    files = run(["git", "diff", "--name-only", f"{upstream_ref}...{head}"], cwd=path).stdout.split()
    return {
        "our_commits": commits,
        "still_ours": sum(1 for c in commits if not c["upstream_has_it"]),
        "files_differing_from_upstream": files,
        "can_switch_to_upstream": not files,
    }


def cmd_plan(args):
    state = load_state(args.work)
    report = []
    for name, dep in forks(args.repo, args.org).items():
        info = github(f"repos/{args.org}/{name}")
        parent = info.get("parent")
        if not parent:
            report.append({"fork": name, "skip": "not a GitHub fork, so there is no upstream"})
            continue
        path = clone(args.work, args.org, name, parent["full_name"])
        upstream_ref = f"upstream/{parent['default_branch']}"
        entry = {
            "fork": name,
            "upstream": f"{parent['full_name']}@{parent['default_branch']}",
            "pin": dep["rev"],
            "crates": dep["crates"],
            "behind": count(path, f"{dep['rev']}..{upstream_ref}"),
            "ours": count(path, f"{upstream_ref}..{dep['rev']}"),
            "upstream_head": run(["git", "rev-parse", upstream_ref], cwd=path).stdout.strip(),
            "drift": drift(path, upstream_ref, dep["rev"]),
        }
        state.setdefault(name, {}).update(entry)
        report.append(entry)
    save_state(args.work, state)
    print(json.dumps(report, indent=2))


def cmd_merge(args):
    state = load_state(args.work)
    if not state:
        sys.exit("fork-sync: run plan first")
    branch = args.branch or f"kagan/upstream-sync-{dt.date.today():%Y-%m-%d}"
    blocked = False
    for name, entry in state.items():
        if "pin" not in entry or (args.only and name not in args.only):
            continue
        path = args.work / name
        if entry.get("behind", 0) == 0:
            entry["result"] = "up to date"
            continue
        merging = (path / ".git" / "MERGE_HEAD").exists()
        if not merging:
            # clone() refuses any clone it did not make, so a half checkout from an interrupted run is safe to reset.
            run(["git", "reset", "--quiet", "--hard"], cwd=path, check=False)
            if run(["git", "rev-parse", "-q", "--verify", f"refs/heads/{branch}"], cwd=path, check=False).returncode:
                run(["git", "checkout", "--quiet", "--force", "-b", branch, entry["pin"]], cwd=path)
            else:
                run(["git", "checkout", "--quiet", "--force", branch], cwd=path)
        upstream_ref = f"upstream/{entry['upstream'].split('@')[1]}"
        merged = run(["git", "merge-base", "--is-ancestor", upstream_ref, "HEAD"], cwd=path, check=False).returncode == 0
        if not merged:
            if merging:
                conflicts = run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=path).stdout.split()
                entry.update(result="conflict", conflicts=conflicts, branch=branch, path=str(path))
                blocked = True
                continue
            message = f"Merge {entry['upstream']} into the kagan pin"
            done = run(["git", "merge", "--no-ff", "-m", message, upstream_ref], cwd=path, check=False)
            if done.returncode:
                conflicts = run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=path).stdout.split()
                if not conflicts:
                    sys.exit(f"fork-sync: merging {name} failed without a conflict\n{done.stdout}{done.stderr}")
                entry.update(result="conflict", conflicts=conflicts, branch=branch, path=str(path))
                blocked = True
                continue
        entry.update(result="merged", branch=branch, head=run(["git", "rev-parse", "HEAD"], cwd=path).stdout.strip(),
                     drift_after_merge=drift(path, upstream_ref, "HEAD"))
        entry.pop("conflicts", None)
    save_state(args.work, state)
    print(json.dumps({k: {f: v.get(f) for f in ("result", "branch", "head", "conflicts", "path", "drift_after_merge")}
                      for k, v in state.items() if "pin" in v}, indent=2))
    if blocked:
        sys.exit(2)


def cmd_push(args):
    state = load_state(args.work)
    for name, entry in state.items():
        if entry.get("result") != "merged":
            continue
        path = args.work / name
        remote = run(["git", "ls-remote", "origin", f"refs/heads/{entry['branch']}"], cwd=path).stdout.split()
        if remote and remote[0] == entry["head"]:
            print(f"{name}: {entry['branch']} already pushed")
            continue
        if remote:
            sys.exit(f"fork-sync: {name} already has {entry['branch']} at another commit; pick a new --branch")
        run(["git", "push", "origin", f"HEAD:refs/heads/{entry['branch']}"], cwd=path, capture=False)
        entry["pushed"] = True
    save_state(args.work, state)


def cmd_pin(args):
    state = load_state(args.work)
    manifest = args.repo / "Cargo.toml"
    text = manifest.read_text()
    changed = []
    for name, entry in state.items():
        if entry.get("result") != "merged":
            continue
        if not entry.get("pushed") and not args.unpushed:
            sys.exit(f"fork-sync: push {name} before pinning, or kagan will point at a commit nobody can fetch")
        text, n = re.subn(rf'(git = "https://github\.com/{args.org}/{re.escape(name)}(?:\.git)?",[^\n]*?rev = ")[0-9a-f]+(")',
                          rf"\g<1>{entry['head']}\g<2>", text)
        if n:
            changed.append((name, entry))
    manifest.write_text(text)
    for name, entry in changed:
        update = run(["cargo", "update", "-p", entry["crates"][0]], cwd=args.repo, check=False)
        print(update.stderr.strip())
        if update.returncode:
            sys.exit(f"fork-sync: cargo update for {name} failed")
    check = run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=args.repo, check=False)
    unused = [l for l in check.stderr.splitlines() if "was not used in the crate graph" in l]
    if unused:
        sys.exit("fork-sync: a [patch] no longer applies, so kagan would build the crates.io copy:\n" + "\n".join(unused))
    print(json.dumps({name: entry["head"] for name, entry in changed}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", type=Path, default=Path.cwd(), help="kagan checkout")
    parser.add_argument("--org", default="kagan-sh")
    parser.add_argument("--work", type=Path, default=Path.home() / ".cache" / "kagan-fork-sync")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("plan")
    merge = sub.add_parser("merge")
    merge.add_argument("--branch")
    merge.add_argument("--only", nargs="*")
    sub.add_parser("push")
    pin = sub.add_parser("pin")
    pin.add_argument("--unpushed", action="store_true", help="local test only; never commit the result")
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    {"plan": cmd_plan, "merge": cmd_merge, "push": cmd_push, "pin": cmd_pin}[args.cmd](args)


if __name__ == "__main__":
    main()
