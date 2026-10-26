#!/usr/bin/env python3
"""Release automation for 3BCAD (standard library only). See RELEASING.md.

  release.py check-commits BASE HEAD  fail if a commit in BASE..HEAD is not a Conventional Commit
  release.py plan                     print the next version, or nothing when no release is due
  release.py apply VERSION [DATE]     bump Cargo.toml and turn [Unreleased] into the VERSION section
"""

import datetime
import re
import subprocess
import sys
from pathlib import Path

REPO_URL = "https://github.com/Three-Bugs-Studio/3bcad"
TYPES = "feat|fix|perf|refactor|docs|test|ci|build|chore|revert|style"
CONVENTIONAL = re.compile(rf"^(?P<type>{TYPES})(?:\((?P<scope>[\w./-]+)\))?(?P<bang>!)?: (?P<desc>\S.*)$")
NONE, PATCH, MINOR, BREAKING = range(4)


def git(*args, cwd="."):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout


def is_conventional(subject):
    return subject.startswith("Merge ") or CONVENTIONAL.match(subject) is not None


def commit_level(subject, body=""):
    m = CONVENTIONAL.match(subject)
    if not m:
        return NONE
    if m["bang"] or "BREAKING CHANGE" in body:
        return BREAKING
    return {"feat": MINOR, "fix": PATCH, "perf": PATCH}.get(m["type"], NONE)


def unreleased(changelog):
    """Body of the [Unreleased] section, stripped."""
    m = re.search(r"^## \[Unreleased\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: |\Z)", changelog, re.M | re.S)
    return m[1].strip() if m else ""


def changelog_level(section):
    if re.search(r"^### Removed|\*\*Breaking", section, re.M):
        return BREAKING
    if re.search(r"^### (Added|Changed|Deprecated)", section, re.M):
        return MINOR
    if re.search(r"^### (Fixed|Security)|^- ", section, re.M):
        return PATCH
    return NONE


def bump(version, level):
    major, minor, patch = map(int, version.split("."))
    if level == BREAKING and major == 0:
        level = MINOR  # before 1.0, breaking changes bump the minor version (see CHANGELOG.md)
    if level == BREAKING:
        return f"{major + 1}.0.0"
    if level == MINOR:
        return f"{major}.{minor + 1}.0"
    if level == PATCH:
        return f"{major}.{minor}.{patch + 1}"
    return version


def notes_from_commits(commits):
    """Keep a Changelog sections from (subject, body) pairs, for when [Unreleased] was left empty."""
    groups = {"Added": [], "Changed": [], "Fixed": []}
    for subject, body in commits:
        m = CONVENTIONAL.match(subject)
        if not m or commit_level(subject, body) == NONE and m["type"] != "refactor":
            continue
        desc = m["desc"][0].upper() + m["desc"][1:]
        if m["scope"]:
            desc = f"{m['scope']}: {desc}"
        if commit_level(subject, body) == BREAKING:
            groups["Changed"].append(f"**Breaking:** {desc}")
        elif m["type"] == "feat":
            groups["Added"].append(desc)
        elif m["type"] == "fix":
            groups["Fixed"].append(desc)
        else:
            groups["Changed"].append(desc)
    return "\n\n".join(f"### {k}\n\n" + "\n".join(f"- {line}" for line in v) for k, v in groups.items() if v)


def cargo_version(cargo_toml):
    m = re.search(r'^\[workspace\.package\][^\[]*?^version = "([^"]+)"', cargo_toml, re.M | re.S)
    if not m:
        raise SystemExit("Cargo.toml has no [workspace.package] version")
    return m[1]


def apply(changelog, cargo_toml, version, date, commits):
    """New (changelog, Cargo.toml) text for releasing `version`."""
    old = cargo_version(cargo_toml)
    cargo_toml = re.sub(
        r'(^\[workspace\.package\][^\[]*?^version = ")[^"]+(")',
        lambda m: m[1] + version + m[2],
        cargo_toml,
        count=1,
        flags=re.M | re.S,
    )
    notes = unreleased(changelog) or notes_from_commits(commits)
    if not notes:
        raise SystemExit(f"Nothing to release: [Unreleased] is empty and no feat/fix commits since v{old}")
    changelog = re.sub(
        r"^## \[Unreleased\][^\n]*\n.*?(?=^## \[|^\[[^\]]+\]: |\Z)",
        lambda _: f"## [Unreleased]\n\n## [{version}] - {date}\n\n{notes}\n\n",
        changelog,
        count=1,
        flags=re.M | re.S,
    )
    changelog = re.sub(
        r"^\[Unreleased\]: .*$",
        lambda _: f"[Unreleased]: {REPO_URL}/compare/v{version}...HEAD\n[{version}]: {REPO_URL}/releases/tag/v{version}",
        changelog,
        count=1,
        flags=re.M,
    )
    return changelog, cargo_toml


def commits_since(tag, cwd="."):
    log = git("log", f"{tag}..HEAD", "--format=%s%x1f%b%x1e", cwd=cwd)
    return [tuple(c.strip("\n").split("\x1f", 1)) for c in log.split("\x1e") if c.strip()]


def plan(cwd="."):
    """The next version, or None. None too while a bumped version is waiting to be tagged."""
    root = Path(cwd)
    current = cargo_version((root / "Cargo.toml").read_text(encoding="utf-8"))
    if f"v{current}" not in git("tag", "-l", "v*", cwd=cwd).split():
        return None
    level = max(
        [commit_level(s, b) for s, b in commits_since(f"v{current}", cwd)]
        + [changelog_level(unreleased((root / "CHANGELOG.md").read_text(encoding="utf-8")))]
    )
    return bump(current, level) if level else None


def main(argv):
    cmd = argv[1] if len(argv) > 1 else ""
    if cmd == "check-commits" and len(argv) == 4:
        subjects = git("log", "--no-merges", "--format=%s", f"{argv[2]}..{argv[3]}").splitlines()
        bad = [s for s in subjects if not is_conventional(s)]
        for s in bad:
            print(f"::error::Not a Conventional Commit: {s!r} (expected e.g. 'feat(sketch): add arc tool')")
        return 1 if bad else 0
    if cmd == "plan":
        print(plan() or "")
        return 0
    if cmd == "apply" and len(argv) in (3, 4):
        version = argv[2]
        date = argv[3] if len(argv) == 4 else datetime.date.today().isoformat()
        if not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise SystemExit(f"Not a version: {version}")
        changelog, cargo = Path("CHANGELOG.md"), Path("Cargo.toml")
        current = cargo_version(cargo.read_text(encoding="utf-8"))
        new_log, new_toml = apply(
            changelog.read_text(encoding="utf-8"),
            cargo.read_text(encoding="utf-8"),
            version,
            date,
            commits_since(f"v{current}"),
        )
        changelog.write_text(new_log, encoding="utf-8", newline="\n")
        cargo.write_text(new_toml, encoding="utf-8", newline="\n")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
