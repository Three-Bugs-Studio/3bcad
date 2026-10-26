"""Tests for release.py. Run: python -m unittest discover -s .github/scripts"""

import subprocess
import tempfile
import unittest
from pathlib import Path

import release
from release import BREAKING, MINOR, NONE, PATCH

CHANGELOG = """# Changelog

## [Unreleased]

{unreleased}
## [0.1.0] - 2026-10-25

### Added

- First preview.

[Unreleased]: https://github.com/Three-Bugs-Studio/3bcad/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.1.0
"""

CARGO = """[workspace]
members = ["a"]

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
foo = { version = "1.0" }
"""


class Rules(unittest.TestCase):
    def test_conventional_subjects(self):
        for ok in ["feat(sketch): add arc tool", "fix: crash", "docs!: rewrite", "ci(release.yml): x", "Merge pull request #1"]:
            self.assertTrue(release.is_conventional(ok), ok)
        for bad in ["sketch: add arc tool", "Add arc tool", "feat:missing space", "feature: x", "feat(): x"]:
            self.assertFalse(release.is_conventional(bad), bad)

    def test_levels(self):
        self.assertEqual(release.commit_level("feat(app): x"), MINOR)
        self.assertEqual(release.commit_level("fix: x"), PATCH)
        self.assertEqual(release.commit_level("perf(sketch): x"), PATCH)
        self.assertEqual(release.commit_level("docs: x"), NONE)
        self.assertEqual(release.commit_level("refactor!: x"), BREAKING)
        self.assertEqual(release.commit_level("fix: x", "BREAKING CHANGE: files"), BREAKING)
        self.assertEqual(release.commit_level("sketch: not conventional"), NONE)
        self.assertEqual(release.changelog_level("### Added\n\n- x"), MINOR)
        self.assertEqual(release.changelog_level("### Fixed\n\n- x"), PATCH)
        self.assertEqual(release.changelog_level("### Removed\n\n- x"), BREAKING)
        self.assertEqual(release.changelog_level(""), NONE)

    def test_bump_follows_semver_with_pre_1_0_rule(self):
        self.assertEqual(release.bump("0.1.0", PATCH), "0.1.1")
        self.assertEqual(release.bump("0.1.3", MINOR), "0.2.0")
        self.assertEqual(release.bump("0.4.2", BREAKING), "0.5.0")
        self.assertEqual(release.bump("1.4.2", BREAKING), "2.0.0")


class Apply(unittest.TestCase):
    def test_curated_unreleased_section_becomes_the_release(self):
        log = CHANGELOG.format(unreleased="### Added\n\n- Sketcher.\n")
        new_log, new_toml = release.apply(log, CARGO, "0.2.0", "2026-11-01", [])
        self.assertIn('version = "0.2.0"', new_toml)
        self.assertIn('foo = { version = "1.0" }', new_toml, "only the workspace version changes")
        self.assertIn("## [Unreleased]\n\n## [0.2.0] - 2026-11-01\n\n### Added\n\n- Sketcher.\n\n## [0.1.0]", new_log)
        self.assertIn("[Unreleased]: https://github.com/Three-Bugs-Studio/3bcad/compare/v0.2.0...HEAD", new_log)
        self.assertIn("[0.2.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.2.0\n[0.1.0]:", new_log)
        self.assertEqual(release.unreleased(new_log), "")

    def test_empty_unreleased_is_filled_from_commits(self):
        commits = [("feat(sketch): add arc tool", ""), ("fix: crash on exit", ""), ("docs: typo", ""), ("feat!: new file format", "")]
        new_log, _ = release.apply(CHANGELOG.format(unreleased=""), CARGO, "0.2.0", "2026-11-01", commits)
        self.assertIn("### Added\n\n- sketch: Add arc tool", new_log)
        self.assertIn("### Changed\n\n- **Breaking:** New file format", new_log)
        self.assertIn("### Fixed\n\n- Crash on exit", new_log)
        self.assertNotIn("Typo", new_log)

    def test_nothing_to_release_is_an_error(self):
        with self.assertRaises(SystemExit):
            release.apply(CHANGELOG.format(unreleased=""), CARGO, "0.1.1", "2026-11-01", [("docs: x", "")])


class Plan(unittest.TestCase):
    """End to end against a throwaway git repository."""

    def repo(self, unreleased=""):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        root = Path(d.name)
        run = lambda *a: subprocess.run(a, cwd=root, check=True, capture_output=True)
        run("git", "init", "-q")
        run("git", "config", "user.email", "t@example.com")
        run("git", "config", "user.name", "t")
        (root / "Cargo.toml").write_text(CARGO)
        (root / "CHANGELOG.md").write_text(CHANGELOG.format(unreleased=unreleased))
        run("git", "add", ".")
        run("git", "commit", "-qm", "chore: initial")
        run("git", "tag", "v0.1.0")
        return root, run

    def test_plans_from_commits_and_changelog(self):
        root, run = self.repo()
        self.assertIsNone(release.plan(root), "nothing since the tag")
        run("git", "commit", "-q", "--allow-empty", "-m", "docs: readme")
        self.assertIsNone(release.plan(root), "docs alone do not release")
        run("git", "commit", "-q", "--allow-empty", "-m", "fix(io): stl header")
        self.assertEqual(release.plan(root), "0.1.1")
        run("git", "commit", "-q", "--allow-empty", "-m", "feat(sketch): arcs")
        self.assertEqual(release.plan(root), "0.2.0")

    def test_curated_changelog_releases_even_without_conventional_commits(self):
        root, run = self.repo(unreleased="### Added\n\n- Sketcher.\n")
        run("git", "commit", "-q", "--allow-empty", "-m", "sketch: old-style message")
        self.assertEqual(release.plan(root), "0.2.0")

    def test_waits_while_a_bumped_version_is_untagged(self):
        root, run = self.repo()
        (root / "Cargo.toml").write_text(CARGO.replace("0.1.0", "0.2.0"))
        run("git", "commit", "-qam", "chore(release): v0.2.0")
        self.assertIsNone(release.plan(root))


if __name__ == "__main__":
    unittest.main()
