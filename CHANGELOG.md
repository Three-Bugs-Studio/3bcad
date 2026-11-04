# Changelog

All notable changes to 3BCAD are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While below 1.0, minor versions (0.x.0) may break
saved files and workflows; patch versions (0.x.y) only fix bugs.

## [Unreleased]

### Added

- Files and interop (v0.5.0 milestone):
  - Native `.3bcad` part files: the whole feature history, units and material as versioned, pretty-printed
    JSON. Damaged files and files from a newer 3BCAD are refused with a clear message, and a save writes
    a temporary file first, so a failed save never destroys the previous one (#60).
  - File menu with New (Ctrl+N), Open (Ctrl+O), Open Recent, Save (Ctrl+S) and Save As (Ctrl+Shift+S); the
    title bar shows the file name and `*` for unsaved changes, and closing, New or Open ask before
    discarding them (#61).
  - Autosave every 10 minutes (set in the File menu, 0 turns it off) and an offer to recover the
    autosaved part after a crash; recovery files are removed after a normal save or exit (#62).
  - STEP import (AP203 / AP214 / AP242): each closed shell becomes a body under an Imported feature, with
    inch and metre files scaled to mm and unreadable shells listed instead of failing the whole file (#63).
  - STEP export writes the part name as the product name, in millimetres, and writes the edges booleans
    leave as B-splines so more readers (3BCAD's own included) open them; CI reads every export back with
    OpenCASCADE and checks its volume (#64).
  - STL (binary and ASCII), 3MF and OBJ meshes import as Imported Mesh reference bodies: drawn, measured,
    included in mass properties and in STL/3MF export (#65).
  - DXF import into a new sketch (lines, arcs, circles, polylines with bulges, splines; `$INSUNITS`
    respected) and DXF export of the selected sketch or planar face (#66).
  - Document units: MMGS, CGS or IPS and the number of decimals, shown in the status bar, every length
    field, sketch dimensions, Measure and Mass Properties; typed units convert (`1in`, `2.5 cm`) (#67).
  - Material library (PLA, PETG, ABS, ASA, TPU, nylon, PC, resin, aluminium, steels, brass, copper,
    titanium, oak) and custom materials, stored in the part; the material's colour shades the model (#68).
  - Fuzz targets for the STEP, STL, OBJ, 3MF and DXF readers and a nightly fuzzing job; every reader is
    also run on truncated and corrupted files in the normal test suite (#69).
- 3 Bugs Studio logo: the window icon and the README header.

### Changed

- The interface uses neutral names: feature tree, Properties panel and ribbon.

## [0.4.0] - 2026-11-03

### Added

- Advanced part modeling (v0.4.0 milestone):
  - Swept Boss/Base and Swept Cut: a closed profile sketch, or a circle of a given diameter, carried along a
    sketch path or a helix, with an optional twist.
  - Lofted Boss/Base and Lofted Cut through two or more profiles, with start/end tangency and guide curves.
  - Helix and spiral curves: pitch and revolutions, height and pitch, height and revolutions, or a flat
    spiral; taper, start angle, clockwise or counterclockwise. Helices are drawn in the graphics area and
    can be picked as a sweep path (springs) or a curve-pattern path.
  - Thread: a modelled ISO 68 / ANSI thread on a rod (external) or in a hole (internal), full length or a set
    length and offset, right- or left-handed. The result is watertight and printable.
  - Variable Fillet (radius control points along a straight edge) and Face Fillet (rounds where two face sets
    meet).
  - Multibody tools: a Solid Bodies folder in the feature tree, Combine (add, subtract, common), Split by a
    plane or a sketch, and Delete/Keep Body.
  - Move/Copy Body by values, a rotation about an axis or by mating a face against a plane, and uniform or
    non-uniform Scale about a point.
  - Sketch Pattern (a copy at each sketch point) and Curve Pattern (equal or fixed spacing along a sketch
    curve or helix, optionally turning with it).
  - Dome on a flat face (an exact spherical cap on round faces), raised or sunk, and Wrap: emboss, deboss or
    scribe a sketch onto a cylinder without stretching it.
- Fillets on round edges: rod ends, hole rims and the root of a boss (the rest of #32).
- Ctrl+click a second sketch or helix (in the feature tree or the graphics area) to pick a sweep's path,
  a loft's next profile or a pattern's sketch.
- Sketch tools snap to the part origin: a point placed there is fixed to it, so a profile can be centred on the
  origin and fully defined without guessing its position.

### Changed

- Rib (#47) runs on to the nearest wall in the plane of the sketch and stops at the floor, so a rib between
  two walls fills the gap exactly.
- Exporting, mass properties and rebuilds of large curved parts are faster: mesh welding is linear
  instead of quadratic, and face analysis starts its surface searches from the mesh.

### Fixed

- A revolve whose profile touches the axis at a single corner is refused with a clear message instead of
  building a solid pinched to a point.
- A crash deep in the geometry kernel during a rebuild now fails only that feature (shown in What's Wrong)
  instead of closing the app.
- Split by a plane that lies on a body's face no longer shaves off a paper-thin body.
- The Properties panel panel stays open with nothing selected, so the graphics area no longer resizes (and
  the model jumps) between two clicks; a selection made in the graphics area shows at once.
- Undo and Redo clear the status bar instead of leaving the last error showing.

## [0.3.1] - 2026-10-30

### Fixed

- Feature rows in the feature tree take clicks, double-clicks and right-clicks again: drag-to-reorder had
  blocked selecting a feature, editing a sketch and the context menu.

## [0.3.0] - 2026-10-30

### Added

- Core part features (v0.3.0 milestone): Extruded Cut (with Flip side to cut), Revolved Boss/Base and Revolved
  Cut, Fillet (straight edges, tangent propagation), Chamfer (equal, distance-distance, angle-distance), Shell
  (inward/outward, removed faces), Hole Wizard (ISO M2-M20 and ANSI #2-1/2" simple, counterbore, countersink
  and tapped holes with cosmetic thread data), Linear and Circular Pattern (of features or bodies, with skipped
  instances), Mirror, Draft, Rib, and reference planes, axes, points and coordinate systems.
- Extrude end conditions: Blind, Through All, Up To Next, Up To Vertex, Up To Surface, Offset From Surface and
  Mid Plane, Direction 2, Reverse, draft angle and thin features from open or closed profiles.
- Merge result: bosses that touch existing bodies become one body (turn it off to keep a separate body).
- Sketches on a face follow the face when the model changes; Convert Entities copies the face's edges.
- Feature tree editing: edit any feature in the Properties panel, rename with F2, suppress/unsuppress, and
  drag to reorder (moves that would put a feature above its parents are refused).
- What's Wrong lists rebuild errors and warnings with a suggested fix and opens when a rebuild fails.
- Evaluate tab: Measure (distances, angles, radii, lengths, areas) and Mass Properties with material presets.
- Sketch pattern instance counts can be changed after the pattern is made.
- Sketch tab "Sketch on" buttons start a sketch on the Front, Top or Right plane, a reference plane or the
  selected face in one click; double-click a plane in the feature tree to sketch on it.
- Keyboard Shortcuts window (F1 or the Shortcuts button in the ribbon).
- Geometry regression corpus (`crates/3bcad-core/tests/parts`) and `cargo bench -p threebcad-core`.

### Changed

- Light look: light panels, a ribbon with large icons above each command and
  the tabs underneath, icons in the feature tree, and a white-to-grey gradient behind the model.
- Command, sketch-tool and feature tree icons are shaded 3D icons: lit gold
  solids, blue sketches and translucent planes, green "add" and red "remove" arrows.
- Faces and edges keep stable names across rebuilds, so fillets and sketches on faces survive dimension
  changes; a reference that is really gone is reported on its feature.
- Rebuilds reuse cached results: editing the last feature only re-evaluates that feature (Ctrl+Q rebuilds all).

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
  more, added from the Properties panel.
- Sketch status: under-defined geometry is blue, fully defined black and conflicts red; the status bar shows
  the remaining degrees of freedom and the feature tree marks sketches (-) or (+). Diagnose lists the
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
- Desktop CAD layout: command ribbon, feature tree with a draggable rollback bar, Properties panel for
  live dimension editing, undo / redo.
- Parametric sketches (points, lines, circles) with Fix, Coincident, Horizontal, Vertical, Distance, Angle,
  Tangent and Radius constraints, solved by Levenberg-Marquardt.
- Extruded Boss/Base from rectangle and circle profiles on the pure-Rust `truck` B-Rep kernel.
- Watertight binary STL and 3MF export, exact STEP export.
- Send to Slicer for OrcaSlicer, Bambu Studio and PrusaSlicer.
- App version shown in the title bar and ribbon, linking to these release notes.

[Unreleased]: https://github.com/Three-Bugs-Studio/3bcad/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.4.0
[0.3.1]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.3.1
[0.3.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.3.0
[0.2.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.2.0
[0.1.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.1.0
