# Changelog

All notable changes to 3BCAD are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While below 1.0, minor versions (0.x.0) may break
saved files and workflows; patch versions (0.x.y) only fix bugs.

## [Unreleased]

## [0.2.0] - 2026-10-26

### Added

- Interactive sketcher (v0.2.0 milestone): sketch on the Front, Top or Right plane or on any flat face, with
  an animated Normal To view and an OK / Cancel confirmation corner. Edits are one undo step, with their own
  undo/redo inside the sketch.
- Sketch tools: Line (chaining), Centerline, Corner / Center / 3 Point Rectangle, Circle, Perimeter Circle,
  Centerpoint / Tangent / 3 Point Arc, Spline, Straight / Centerpoint Slot, Polygon, Point, Sketch Fillet,
  Sketch Chamfer, Trim, Extend, Offset, Mirror, Linear and Circular Sketch Pattern, construction geometry.
- Inferencing: snapping to points, midpoints and curves with automatic Coincident / Midpoint / Horizontal /
  Vertical relations and dotted alignment lines (hold Ctrl to disable).
- Smart Dimension with horizontal / vertical / aligned placement, radius, diameter, angle and point-to-line
  distances, edited inline in the graphics area.
- Relations: Parallel, Perpendicular, Collinear, Equal, Midpoint, Concentric, Symmetric, point on curve and
  more, added from the PropertyManager.
- Sketch status: under-defined geometry is blue, fully defined black and conflicts red; the status bar shows
  the remaining degrees of freedom and the FeatureManager marks sketches (-) or (+). Diagnose lists the
  relations whose removal fixes an over-defined sketch.
- Drag sketch geometry with live solving.
- Profiles with holes and islands, arcs and splines; contours can be drawn in any order.
- Picking and highlighting of faces, edges, vertices and reference planes in the 3D view.
- Developer architecture guide (`docs/architecture.md`).

### Changed

- The sketch solver now uses exact (auto-differentiated) Jacobians and solves independent parts of a sketch
  separately; a 1000-entity sketch solves in milliseconds.
- The 3D camera tumbles freely over the top of the model.

## [0.1.0] - 2026-10-25

First public preview of 3BCAD.

### Added

- Desktop app for Windows, macOS and Linux with a wgpu 3D viewport: shaded faces, B-Rep edge highlights,
  ground grid, XYZ triad, orbit / pan / zoom and zoom-to-fit.
- SolidWorks-style layout: CommandManager ribbon, FeatureManager tree with a draggable rollback bar,
  PropertyManager for live dimension editing, undo / redo.
- Parametric sketches (points, lines, circles) with Fix, Coincident, Horizontal, Vertical, Distance, Angle,
  Tangent and Radius constraints, solved by Levenberg-Marquardt.
- Extruded Boss/Base from rectangle and circle profiles on the pure-Rust `truck` B-Rep kernel.
- Watertight binary STL and 3MF export, exact STEP export.
- Send to Slicer for OrcaSlicer, Bambu Studio and PrusaSlicer.
- App version shown in the title bar and ribbon, linking to these release notes.

[Unreleased]: https://github.com/Three-Bugs-Studio/3bcad/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.2.0
[0.1.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.1.0
