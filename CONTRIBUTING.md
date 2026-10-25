# Contributing to 3BCAD

Thanks for helping build open-source CAD! This guide covers how to propose changes.
By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report bugs** with the bug template. Include your OS, GPU and steps to reproduce; attach the exported file if it is relevant.
- **Suggest features** with the feature template. Say what you are trying to model or print, not only the tool you want.
- **Send code**: look for issues labelled `good first issue` or `help wanted`. For anything large
  (new feature types, kernel changes, new dependencies) open an issue first so we can agree on the design.
- **Test prints**: exporting parts and checking them in your slicer is very valuable feedback.

## Development setup

```bash
git clone https://github.com/Three-Bugs-Studio/3bcad.git
cd 3bcad
cargo run          # launch the app
cargo test --workspace
```

Rust 1.87+ is required. Linux needs `libgtk-3-dev libxkbcommon-dev libwayland-dev`.

## Before you open a pull request

CI runs these; please run them locally first:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- Keep pull requests focused: one feature or fix per PR.
- Add a test for new logic (solver constraints, kernel operations, file formats). Geometry tests
  should check something physical: volume, watertightness, solved coordinates.
- Follow the existing crate boundaries: `core` must not depend on `render` or `app`; `kernel` must not depend on `sketch`.
- New dependencies need a reason in the PR description. Prefer pure-Rust crates with MIT/Apache-2.0 compatible licenses.
- Update `README.md` if you change user-facing behaviour.

## Commit messages

Use short imperative subjects, optionally prefixed by the crate: `sketch: add arc entity`, `io: write 3MF colours`.

## Licensing of contributions

3BCAD is dual licensed under MIT or Apache-2.0. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

Do not submit code copied from proprietary CAD software, or from projects under licenses incompatible with
MIT/Apache-2.0 (for example GPL or LGPL code).
