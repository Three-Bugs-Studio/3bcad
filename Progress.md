# Progress

Last updated: 2026-10-26. Full plan: [ROADMAP.md](ROADMAP.md).

| Milestone | Status |
| --- | --- |
| v0.1.0 – First preview | ✅ Released 2026-10-26 |
| v0.2.0 – Interactive Sketcher | 🟡 Implemented in PR [#120](https://github.com/Three-Bugs-Studio/3bcad/pull/120), CI green, awaiting manual UI test + merge |
| v0.3.0 – Core Part Features | ⏳ Not started |
| v0.4.0 – v1.0.0, Future | ⏳ Not started |

## v0.2.0 – Interactive Sketcher

✅ done in PR #120 · 🟡 partly done · ⏳ open

| # | Issue | Status | Notes |
| --- | --- | --- | --- |
| 1 | Viewport picking | ✅ | Faces, edges, vertices, planes; Ctrl adds, Esc clears; occlusion-aware |
| 2 | Pre-highlight and highlight colours | ✅ | GPU state buffer, no mesh re-upload |
| 3 | Front / Top / Right planes and origin | ✅ | Translucent, show/hide and selectable in the FeatureManager |
| 4 | Sketch mode with Normal To | ✅ | Planes or flat faces; animated Normal To; OK / Cancel corner |
| 5 | Line tool | ✅ | Chaining, auto-close, snapping |
| 6 | Rectangle tools | ✅ | Corner, center, 3-point |
| 7 | Circle tools | ✅ | Center (radius can be typed), perimeter |
| 8 | Arc entity and tools | ✅ | Centerpoint, tangent, 3-point |
| 9 | Spline | ✅ | Catmull-Rom → Béziers; points can be related/dimensioned |
| 10 | Slot tool | ✅ | Straight and centerpoint |
| 11 | Polygon tool | ✅ | Inscribed / circumscribed |
| 12 | Sketch fillet and chamfer | ✅ | Virtual sharp keeps corner dimensions |
| 13 | Trim and extend | ✅ | Splines cannot be trimmed yet |
| 14 | Offset entities | ✅ | Chains stay joined; splines not supported yet |
| 15 | Mirror and patterns | 🟡 | Mirror ✅, patterns ✅ with editable spacing; **instance count not editable after Apply** (issue stays open) |
| 16 | Construction geometry | ✅ | Toggle + Centerline tool |
| 17 | Inferencing | ✅ | Points, midpoints, curves, H/V alignment; Ctrl disables |
| 18 | Smart Dimension | ✅ | H / V / aligned, radius, diameter, angle, point-line; inline edit |
| 19 | More relations | ✅ | 24 relation/dimension types; Add Relations panel |
| 20 | Definition status colours | ✅ | Blue / black / red, DOF in status bar, (-)/(+) in tree |
| 21 | Over-defined diagnosis | ✅ | Lists relations whose removal fixes it |
| 22 | Multi-loop profiles | ✅ | Holes, islands, any order, arcs, splines |
| 23 | Solver performance | ✅ | Exact Jacobians (autodiff), components, CG; 1000 entities < 50 ms |
| 24 | Drag with live solving | ✅ | Free geometry follows the cursor; fixed geometry stays |
| 25 | Architecture guide | ✅ | `docs/architecture.md` |

## Test coverage

65 automated tests (`cargo test --workspace`) plus a release-mode solver benchmark. Manual UI testing is
still to do; see [Release.md](Release.md#manual-test-script).
