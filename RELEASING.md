# Releasing 3BCAD

3BCAD uses [Semantic Versioning](https://semver.org). The version lives in one place, `version` under
`[workspace.package]` in the root `Cargo.toml`. The app shows it in its title bar and ribbon.

| Change | Bump | Example |
| --- | --- | --- |
| Bug fixes only | patch | 0.1.0 -> 0.1.1 |
| New features, or (before 1.0) breaking changes | minor | 0.1.1 -> 0.2.0 |
| Pre-release for testers | pre-release suffix | 0.2.0-beta.1 |

## Steps

1. Make sure `main` is green in CI.
2. Set the new `version` in the root `Cargo.toml`, then run `cargo build` so `Cargo.lock` updates.
3. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, add a fresh empty
   `## [Unreleased]` above it, and update the compare links at the bottom.
4. `main` is protected, so release through a pull request: on a `release/vX.Y.Z` branch,
   `git commit -am "Release vX.Y.Z"`, push, open a PR, wait for CI and merge it.
5. Update your local `main` (`git checkout main && git pull`), then tag the merged commit and push the tag:

   ```bash
   git tag -a vX.Y.Z -m "3BCAD vX.Y.Z"
   git push origin vX.Y.Z
   ```

The [Release workflow](.github/workflows/release.yml) then:

- fails early if the tag does not match the `Cargo.toml` version or `CHANGELOG.md` has no section for it,
- builds the desktop app for Windows (x64 `.zip`), macOS (universal `.app` in a `.zip`) and Linux (x64 `.tar.gz`),
- publishes a GitHub Release with those files plus `SHA256SUMS.txt`, using the changelog section as release notes.
  Tags with a suffix such as `-beta.1` are marked as pre-releases.

If a release build fails, fix it on `main`, delete the tag (`git push --delete origin vX.Y.Z` and
`git tag -d vX.Y.Z`) and tag again. Never reuse a version number that users have already downloaded.

## Known limitations

The builds are not code-signed yet. On first launch Windows SmartScreen shows "Windows protected your PC"
(click **More info → Run anyway**), and macOS needs a right-click on `3BCAD.app` → **Open**.
