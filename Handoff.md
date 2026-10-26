# Handoff

Last updated: 2026-10-26 · Branch `feat/v0.2.0-interactive-sketcher` · PR [#120](https://github.com/Three-Bugs-Studio/3bcad/pull/120)

## Task

Implement the **v0.2.0 – Interactive Sketcher** milestone (issues #1–#25): picking, sketch mode on planes and
faces, every SolidWorks-style sketch tool, inferencing, Smart Dimension, relations, definition status,
diagnosis, multi-loop profiles, a faster solver, dragging and an architecture guide.

## Current Phase

Optimize / review. All code is written and committed (one file per commit). CI on PR #120 passes
(`fmt` and `check` on Linux, Windows, macOS). Waiting for a hands-on UI test and the merge.

## Scope

- In: issues #1–#25 except the "editable instance count" part of #15 (#15 stays open).
- Out: everything in v0.3.0+ (booleans, Cut, Revolve, fillets on solids, persistent face naming #42, save/load).

## Files Inspected

All crates. Main changes:

| Crate | Files |
| --- | --- |
| sketch | `scalar.rs` (new), `geom.rs` (new), `entity.rs`, `constraint.rs`, `solver.rs`, `profile.rs` (new), `tools.rs` (new), `inference.rs` (new), `lib.rs` |
| kernel | `solid.rs` (Plane), `operations.rs` (region extrude), `mesher.rs` (per-face meshing) |
| core | `feature.rs`, `history.rs`, `document.rs` |
| render | `camera.rs`, `pipeline.rs`, `scene.rs`, `pick.rs` (new) |
| app | `app.rs`, `ui/sketch_mode.rs` (new), `ui/viewport.rs`, `ui/ribbon.rs`, `ui/tree_view.rs`, `ui/properties.rs` |
| docs | `docs/architecture.md` (new), README, CONTRIBUTING, CHANGELOG |

## Decisions Made

- **Z-up planes**: Top = XY (normal +Z, the print bed), Front = XZ (normal −Y), Right = YZ (normal +X).
- **Exact Jacobians by forward-mode autodiff**: each relation's residual is written once over `Scalar`;
  `Dual` gives derivatives. Max 8 local parameters per relation.
- **Solver**: split into independent components; dense Cholesky up to 300 parameters, Jacobi-preconditioned
  CG above. Redundancy is judged on user relations only (implicit arc relations are often implied).
- **Tangent at a shared endpoint** uses "radius ⟂ line" (the distance form is degenerate there).
- **Circumscribed polygons** use a circumcircle + one tangent inner circle (all-sides-tangent + equal sides is
  redundant for even side counts, by Pitot's theorem).
- **Fillet/chamfer keep a virtual sharp** (old corner on both lines) so dimensions to the corner survive.
- **Drag** pins dragged points with temporary Fix relations; if that cannot solve, relations win.
- **Sketch editing works on a copy**; Exit Sketch writes it back as one document undo step, Cancel discards.
- **Highlighting** is a GPU per-id state buffer; meshes are never re-uploaded for hover/selection.
- **Sketch on a face** stores the face's plane at creation (does not follow the face until #42).

## Assumptions

- Units are millimetres everywhere.
- A sketch profile must be closed loops only; open curves make the extrude fail with a clear message.
- Pattern copies are rigid; spacing/angle are editable dimensions, the instance count is not (see #15).

## Tests Run

- `cargo test --workspace`: 65 passed (sketch 37, kernel 8, core 7, render 5, app 5 + 2 release, io 1).
- `cargo test --release -p threebcad-sketch -- --ignored`: 1000-entity sketch solves in < 50 ms.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` on Rust 1.92 and 1.99: clean.
- App launch smoke test (12 s, no errors; shaders and pipelines validate).
- CI on PR #120: all green.

## Remaining Risks

- **Nobody has clicked through the new UI yet** (no desktop control was available in the dev session).
  Tool flows, snapping feel, dimension placement and the confirmation corner need a manual pass.
- macOS/Linux UI only covered by CI builds, not by a person.
- Large sketches: analysis (eigen-decomposition per component) runs every drag frame; fine for typical
  sketches, may stutter on very large connected ones.

## Next Action

Check out the branch and walk through the manual test script in [Release.md](Release.md#manual-test-script),
then merge PR #120 with the admin bypass if it passes.
