# Release plan: v0.2.0 – Interactive Sketcher

Last updated: 2026-10-26. The general release procedure lives in [RELEASING.md](RELEASING.md); this file
tracks the next release only.

| | |
| --- | --- |
| Version | 0.2.0 (minor: new features, no saved-file format yet) |
| Contents | PR [#120](https://github.com/Three-Bugs-Studio/3bcad/pull/120), 24 of 25 v0.2.0 issues (#15 partly) |
| Release notes | `CHANGELOG.md` → `[Unreleased]` (already written) |
| Status | Waiting for manual UI test and merge |

## Checklist

- [x] Features implemented and committed (32 single-file commits)
- [x] `cargo test --workspace` (65 tests), fmt and strict clippy pass locally
- [x] CI green on PR #120 (Linux, Windows, macOS)
- [x] CHANGELOG `[Unreleased]` section written
- [ ] Manual test script below passed on Windows
- [ ] Manual smoke test on macOS and Linux (optional but recommended)
- [ ] PR #120 merged
- [ ] Release PR `release/v0.2.0`: `version = "0.2.0"` in `Cargo.toml`, rename `[Unreleased]` to
      `[0.2.0] - <date>`, add an empty `[Unreleased]`, add the `[0.2.0]` link at the bottom of CHANGELOG.md
- [ ] Release PR merged, then `git tag -a v0.2.0 -m "3BCAD v0.2.0"` and `git push origin v0.2.0`
- [ ] Release workflow green; download the Windows zip and check it opens
- [ ] Close the v0.2.0 milestone (move #15 to v0.3.0 if still open)

## Manual test script

Run `cargo run` on the branch, then:

1. **Picking**: hover the cube; faces, edges and corners highlight orange. Click selects (blue), Ctrl+click
   adds, Esc clears. Click the Front Plane in the FeatureManager; it highlights.
2. **Sketch mode**: with the Top Plane selected, Sketch tab → ✏ Sketch. The view animates straight down.
3. **Drawing**: draw a Corner Rectangle, a Circle inside it, a Line chain that closes, a 3 Point Arc, a Slot
   and a Polygon. Previews follow the cursor; snapping shows ◉ / M markers and dotted alignment lines.
4. **Dimensions**: Smart Dimension the rectangle's width and height (drop above/beside for H/V) and the circle's
   diameter. Edit values inline. Geometry turns black when fully defined; the status bar shows the DOF.
5. **Relations**: select two lines, add Parallel / Equal from the PropertyManager; delete one with ✖.
6. **Over-defined**: add a dimension that repeats another; the sketch turns red. Diagnose lists the culprits;
   deleting one turns it back.
7. **Edit tools**: Fillet a rectangle corner, Trim a line between two crossings, Offset a chain, Mirror about a
   centerline, Linear Pattern a circle and change its spacing.
8. **Drag**: with Select, drag an under-defined point; it follows. Dragging a fully defined point does nothing.
9. **Exit**: ✔ Exit Sketch; the sketch appears in the tree with (-) or no prefix. Features tab → Extruded
   Boss/Base: the rectangle-with-a-hole extrudes with a hole. Ctrl+Z undoes the extrude, then the sketch.
10. **Sketch on a face**: click the top face of the cube, Sketch, draw a circle, exit, extrude.

## Known limits to mention in the release notes

- A sketch on a face does not follow the face when the model changes (#42).
- Pattern instance count cannot be edited after applying (#15).
- Splines cannot be trimmed or offset; no box selection yet.
- Builds are unsigned (SmartScreen / Gatekeeper warnings).
