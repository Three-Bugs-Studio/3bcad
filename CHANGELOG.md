# Changelog

All notable changes to 3BCAD are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While below 1.0, minor versions (0.x.0) may break
saved files and workflows; patch versions (0.x.y) only fix bugs.

## [Unreleased]

### Added

- Open models from other apps and websites: **File → Open**, dragging a file onto the window and
  `3bcad <file>` (what "Open with" passes) open STEP, STL, 3MF, OBJ and DXF files as a new part, named
  after the file. File → Import still adds them to the open part.
- Installers (#100, #102): a per-user Windows `.msi` (no admin rights, Start menu entry, uninstall entry; it opens
  `.3bcad`, `.3bcasm` and `.3bcdrw` files and offers 3BCAD under "Open with" for STEP, STL, 3MF, OBJ and DXF),
  a macOS `.dmg` with an Applications shortcut, and a Linux AppImage with a desktop entry, icons and file types.
  The `.zip` and `.tar.gz` downloads stay, and in-app updates keep working for installed copies.
- App icon in every size (ICO, ICNS, PNG 16–512), embedded in `3bcad.exe` with its name and version and used by
  the installers, the macOS bundle and the Linux desktop entry (#103).
- Release code signing, ready for when the project has certificates: Windows Authenticode, and macOS Developer
  ID with notarization and stapling. It runs only when its secrets exist, and CI verifies the signatures
  (#101, see RELEASING.md).

- View controls (#94): a View toolbar over the model (standard views, zoom to fit, display style, section view,
  perspective), a clickable view cube (faces, edges and corners), SolidWorks keys Ctrl+1 … Ctrl+8 for Front, Back,
  Left, Right, Top, Bottom, Isometric and Normal To, animated view changes, and an orthographic projection
  (`P`). The arrow keys turn the view 15° (Shift: 90°), Ctrl+arrows pan, Page Up / Down zoom.
- Display styles (#95): Shaded With Edges, Shaded, Hidden Lines Removed, Hidden Lines Visible and Wireframe, with
  the outlines of curved faces in the line styles. `D` goes to the next style.
- Section view (#96): cut the model open along a plane parallel to Front, Top or Right, with the cut capped in
  orange, a slider and a drag arrow in the view, and Flip. The part does not change; clicking picks what lies
  behind the cut.
- Themes (#97): light, dark, high contrast, or the system's, with a matching background behind the model.
- Keyboard shortcuts and mouse gestures (#98): change any command's shortcut in **Settings → Shortcuts** (a key
  already in use is reported, not taken), `S` brings the current tab's commands to the mouse, and an optional
  right-drag gesture wheel gives the Top, Front and Isometric views and zoom to fit.
- **File → Settings…** (Ctrl+,) (#99): language, theme, interface size, units of new parts, autosave, update
  check, the view 3BCAD starts with, mouse, shortcuts, slicers and performance, saved per user, with **Reset to
  defaults**.
- Log file and crash reports (#105): `3bcad.log` in the user folder (the last three runs kept), and a crash
  report after a crash, offered at the next start. **Help → Open Log Folder**; the bug report form asks for
  both. A new **Help** menu also links the user guide and the bug report form.
- Large models (#106): parts are meshed for display in the background with a progress note (the window keeps
  responding), models over 300 000 triangles draw a simplified copy while the view moves, and picking skips
  most of a large mesh. An 800 000-triangle scan (NASA's Vesta) opens in a few seconds and drew at about
  450 frames a second still and 800 while orbiting on the test machine. **Settings → Performance** can show the frame rate.
- Vietnamese (#108): the whole interface in Tiếng Việt (**Settings → General → Language**), with a guide to
  adding languages in `docs/guide/translating.md`. Vietnamese letters use the system's font (Segoe UI, Arial,
  DejaVu Sans), as egui's own has none.
- Accessibility (#109): the interface size (50–300 %, also Ctrl + plus / minus), a high-contrast theme,
  keyboard control of the view, and screen-reader support through AccessKit.

### Changed

- **File → Settings…** replaces the File menu's autosave and update-check items; **Check for Updates…** and
  **About** moved to the new **Help** menu.
- New parts use the units chosen in Settings.

### Fixed

- STEP import, checked against OpenCASCADE on files from SolidWorks, Fusion 360, Onshape, OCCT and the
  NIST AP203/AP242 test parts:
  - AP242 files with PMI (tessellated annotation, annotation planes) no longer fail as "not a STEP file".
  - Units come from the shape's own context: a part named "Foot" is no longer scaled 304.8×, and mm parts
    with inch PMI no longer come in 25.4× too big. Cones in files that give angles in degrees are right.
  - Holes, bosses, chamfers, fillets and ball ends no longer mesh halfway or balloon: cylinders and cones
    bounded by two circles (SolidWorks, Parasolid), fillets on the inner side of a torus, sphere corners,
    drill points and countersinks, and small holes in files written in metres (Onshape) now come in with
    the right shape and volume.
  - OpenCASCADE files with rounded corners (FreeCAD, CadQuery, KiCad models) no longer fall apart into
    open faces.
  - A file whose arcs cannot be read is refused with a message instead of running out of memory.

## [0.9.0] - 2026-11-08

### Added

- Design intent and configurations (v0.9.0 milestone):
  - Global variables and equations: **Evaluate → Equations** sets variables (`wall = 2`) and drives any
    dimension from an expression (`D1@Boss-Extrude1 = wall * 10`). Expressions take units (`5 mm`, `1in`,
    `3"`, `30deg`), `+ - * / ^`, brackets, `pi` and `sin cos tan asin acos atan sqrt abs min max round
    floor ceil` (angles in degrees). Equations are solved in dependency order whatever order they are
    listed in; a circular reference, an unknown name or a typing mistake is shown on the equation that has
    it, and the others still apply. Sketch dimensions are named `D1@Sketch1` and keep their number when
    others are deleted (the sketch's dimension box shows it); **+ Dimension** lists every dimension of
    the part with its name and value. Length fields and the sketch dimension box take arithmetic too
    (`10 + 2in`) (#90).
  - Configurations: variants of one part, each with its own suppressed features and its own equations
    over the part's (tick **This configuration only**), so a configuration can change any dimension.
    Switch from the **Configuration** box at the top of the feature tree; add, rename and delete
    configurations in the Equations window. The 3D Print tab picks the configuration STL, 3MF and STEP
    exports and Send to Slicer are made in (#91).
  - Appearances: colour a face, a whole body or the faces a feature made (**Evaluate → Appearance**, or
    right-click a feature or a body in the tree). A face shows its own colour, else its body's, else its
    feature's, else the material's. Colours follow their faces through rebuilds, show in parts and in
    assemblies, and are written to 3MF (a base material per colour, set per triangle) and STEP (AP214
    face styles, read by FreeCAD and other OpenCASCADE-based CAD) (#92).
  - Custom properties: drawing notes fill in `{Part number}`, `{Material}` and every other property the
    way title blocks do, and File → Properties has an **Author** row button and wider fields (#93).
- **About 3BCAD** (click the 3BCAD name at the top left, or File → About 3BCAD…): who made 3BCAD, its
  license and its home page.
- A `NOTICE` file (shipped in the release downloads), `CITATION.cff` for GitHub's **Cite this repository**,
  and a trademark policy (`TRADEMARKS.md`) for forks and derived products.

### Changed

- Command and tree icons use 3BCAD's own teal and orange palette: teal solids, orange add and move
  arrows, the logo's blues for sketches; green is kept for OK and the Y axis.
- Part files are now format version 4 (equations, configurations, appearances and sketch dimension
  numbers). Older parts open as before; 3BCAD 0.8 and older refuse parts saved by this version.

## [0.8.0] - 2026-11-07

### Added

- Drawings (v0.8.0 milestone):
  - Drawing documents (`.3bcdrw`) of a part, with sheets in ISO A4 to A0 and ANSI A to E sizes, as many
    as you need, each with its own scale and first- or third-angle projection. The title block's cells
    fill in from the part's and the drawing's properties (`{Part number}`, `{Material}`, `{Scale}`,
    `{Sheet}` as "1 of 2", ...) and can be changed, added and removed (#85).
  - Standard views (the front view with the top and side views projected from it), model views from any
    side or isometric, and projected views that stay lined up with the view they come from. Hidden lines
    are dashed or removed. The drawing reads the part from its file, so the views follow every change
    saved to the part (#86).
  - Section views along a vertical or horizontal line, labelled A-A on the view they cut and hatched for
    the part's material (ANSI31/32/33/37), and detail views of a circle of a view, twice as large by
    default, labelled with their scale (#87).
  - Dimensions brought in from the part's sketches and extrusions (Model Items), reference dimensions
    measured off a view (an edge, two vertices, a circle's diameter or an arc's radius) that follow the
    part when it changes, notes with leaders, and centre marks and centrelines (#88).
  - Export every sheet as one vector PDF at true scale, or an SVG or an R12 DXF per sheet, the DXF with a
    layer and line type for visible, hidden and centre lines, hatching, dimensions, notes and the sheet
    format (#89).
- File > New Drawing, and Edit Part from a drawing with a button back to it.
- Updates from inside the app: 3BCAD asks GitHub for a newer release when it starts (File > Check for
  updates at start turns it off; File > Check for Updates asks at any time) and shows what's new in it.
  **Update now** downloads the release for your computer, checks it against the release's checksums,
  installs it in place and restarts. Nothing is sent but the request to GitHub. 3BCAD 0.7.0 and older
  cannot do this: download 0.8.0 once by hand, and it updates itself from then on (#104).

## [0.7.0] - 2026-11-06

### Added

- Assemblies (v0.7.0 milestone):
  - Assembly documents (`.3bcasm`): insert parts and other assemblies from a file or the recent list; the
    first component is fixed at the origin. Components are read from their own files, and the assembly
    shows a part as it is now whenever its file changes (#77).
  - Mates between flat and cylindrical faces: coincident, concentric, distance, angle, parallel,
    perpendicular, tangent and lock, each with Flip. New mates keep the distance or angle the faces have
    and the alignment closest to how they sit. Mates are listed in the tree and edited in the Properties
    panel (#78).
  - An assembly solver that moves the free components until their mates hold, and names the mates that
    contradict the ones before them (over-defined) instead of moving anything for them (#79).
  - Drag a component to move it and Shift+drag to turn it; mated components move only as their mates
    allow (#80).
  - Interference check: pairs of parts that overlap, with the shared volume, shown in red; parts that
    only touch do not count (#81).
  - Exploded views: steps that move components along a direction, played as an animation or scrubbed
    with a slider (#82).
  - Bill of materials, top-level or indented by sub-assembly, with part number, description, material
    and any other custom property, exported as CSV. Parts and assemblies have custom properties under
    File > Properties (#83).
  - Assembly export: STEP with each part once and every component as a named, placed instance; 3MF with
    one object per part, each on its print face and laid out on the bed for the slicer (#84).
- File > New Assembly, and Edit Part from an assembly with a button back to it.

### Changed

- Part files are format version 3 (custom properties). 3BCAD still opens version 1 and 2 files; 3BCAD 0.6
  refuses version 3 files with a message to update.

## [0.6.0] - 2026-11-05

### Added

- 3D Print studio (v0.6.0 milestone):
  - Overhang check: faces steeper than the overhang limit (45° by default, measured from vertical) are
    coloured for the build direction the part is laid out in; faces resting on the bed are not (#70).
  - Thin wall check: walls thinner than the nozzle (red) or than the minimum wall (orange) are coloured,
    with the nozzle diameter and minimum wall as settings (#71).
  - Print layout: the 3D Print tab shows the part on the bed of a chosen printer (Bambu Lab, Prusa,
    Creality, Voron presets or a custom build volume) and warns when it does not fit. Lay Flat puts a
    selected face on the bed (or points an edge or axis up); the orientation is saved with the part (#72).
  - 3MF export writes one named object per body with the part's material and colour, the part's name, and
    the printer, nozzle and overhang limit as metadata; STL and 3MF are written as the part sits on the
    bed (#73).
  - Send to Slicer remembers the chosen slicer, and when a slicer is not found offers to locate its
    program, used from then on (#74).
  - Export quality: Coarse, Fine or Custom chord and angle tolerance, with the export's triangle count
    shown before exporting (#75).
  - Fit: moves the faces of a hole, pin, tab or slot into the part by a clearance, with press, slip and
    loose presets for FDM, resin and powder printers; one click on a hole or pin takes all of it (#76).
- Revolve can turn about the world X, Y or Z axis, so imported profiles need no centerline.
- Smart Dimension can measure to the part origin: clicking it adds a point fixed there.

### Changed

- Part files are format version 2 (the Fit feature and the print orientation). 3BCAD still opens version 1
  files; 3BCAD 0.5 refuses version 2 files with a message to update.

### Fixed

- Opening, recovering or starting a part could keep showing the previous part when both had been
  rebuilt the same number of times.
- Send to Slicer on macOS reported success when the slicer was not installed.
- The status bar kept showing a feature's error after the error was fixed in the Properties panel.
- The "saved by a newer 3BCAD" message had a run of spaces in the middle.

## [0.5.0] - 2026-11-04

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

[Unreleased]: https://github.com/Three-Bugs-Studio/3bcad/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.9.0
[0.8.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.8.0
[0.7.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.7.0
[0.6.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.6.0
[0.5.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.5.0
[0.4.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.4.0
[0.3.1]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.3.1
[0.3.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.3.0
[0.2.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.2.0
[0.1.0]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.1.0
