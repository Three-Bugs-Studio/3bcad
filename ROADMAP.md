# 3BCAD Roadmap

3BCAD aims for the everyday workflow of SolidWorks — parts, assemblies and drawings — plus first-class
3D-printing tools. Every item below is a GitHub issue; pick one, comment that you are on it, and see
[CONTRIBUTING.md](CONTRIBUTING.md). Issues labelled `good first issue` are a good start.

Order is a plan, not a promise: milestones ship when they are ready.

**Done in v0.1.0:** wgpu viewport, ribbon/FeatureManager/PropertyManager layout, parametric sketches with
a Levenberg-Marquardt solver, Extruded Boss from rectangles/circles, rollback bar, undo/redo,
STL/3MF/STEP export and Send to Slicer.

## [v0.2.0 – Interactive Sketcher](https://github.com/Three-Bugs-Studio/3bcad/milestone/1)

Draw, constrain and dimension sketches directly in the viewport.

- [ ] #1 Viewport picking: select faces, edges and vertices
- [ ] #2 Selection pre-highlight and highlight colours
- [ ] #3 Show Front / Top / Right reference planes and origin
- [ ] #4 Sketch mode: edit a sketch on a plane with Normal To view
- [ ] #5 Line tool with chaining and auto-close
- [ ] #6 Rectangle tools: corner, center and 3-point
- [ ] #7 Circle tools: center-radius and perimeter
- [ ] #8 Arc entity and arc tools: centerpoint, tangent and 3-point
- [ ] #9 Spline entity (B-spline through points)
- [ ] #10 Slot tool (straight and center-point slots)
- [ ] #11 Polygon tool (inscribed / circumscribed)
- [ ] #12 Sketch fillet and sketch chamfer
- [ ] #13 Trim and extend entities
- [ ] #14 Offset entities
- [x] #15 Mirror entities and linear / circular sketch patterns
- [ ] #16 Construction geometry toggle and centerlines
- [ ] #17 Sketch inferencing and automatic relations
- [ ] #18 Smart Dimension tool in the viewport
- [ ] #19 More sketch relations: Parallel, Perpendicular, Equal, Midpoint, Concentric, Collinear, Symmetric, Point-on-curve
- [ ] #20 Sketch definition status colours (under / fully / over-defined)
- [ ] #21 Diagnose over-defined sketches (SketchXpert-lite)
- [ ] #22 Multi-loop profiles: regions with holes and unordered lines
- [ ] #23 Solver performance: analytic Jacobians and sparse solve
- [ ] #24 Drag under-defined sketch geometry with live solving
- [ ] #25 Developer architecture guide

## [v0.3.0 – Core Part Features](https://github.com/Three-Bugs-Studio/3bcad/milestone/2)

The everyday SolidWorks part toolbox: cut, revolve, fillet, chamfer, shell, holes, patterns.

- [x] #26 Integrate boolean operations (truck-shapeops)
- [x] #27 Extruded Cut
- [x] #28 Merge result: combine touching bodies into one
- [x] #29 Extrude end conditions: Through All, Up To Next/Vertex/Surface, Mid Plane, Offset From Surface
- [x] #30 Extrude draft angle and thin feature
- [x] #31 Revolved Boss/Base and Revolved Cut
- [ ] #32 Fillet: constant-radius edge fillet
- [x] #33 Chamfer
- [x] #34 Shell
- [x] #35 Hole Wizard: simple, counterbore, countersink and tapped holes
- [x] #36 Linear Pattern
- [x] #37 Circular Pattern
- [x] #38 Mirror features and bodies
- [x] #39 Reference geometry: planes, axes, points and coordinate systems
- [x] #40 Sketch on a planar face
- [x] #41 Edit Feature / Edit Sketch, rename and reorder in the FeatureManager
- [x] #42 Suppress / unsuppress features
- [x] #43 Persistent face/edge naming across rebuilds (topological naming)
- [x] #44 Incremental rebuild: only re-evaluate dirty features
- [x] #45 What's Wrong: rebuild error and warning panel
- [x] #46 Draft feature
- [ ] #47 Rib feature
- [x] #48 Measure tool
- [x] #49 Mass properties
- [x] #50 Geometry regression corpus and benchmarks

## [v0.4.0 – Advanced Part Modeling](https://github.com/Three-Bugs-Studio/3bcad/milestone/3)

Sweeps, lofts, multibody tools and threads.

- [ ] #51 Swept Boss/Base and Swept Cut
- [ ] #52 Lofted Boss/Base and Lofted Cut
- [ ] #53 Variable-size fillet and face fillet
- [ ] #54 Multibody tools: Combine, Split, Delete/Keep Body
- [ ] #55 Thread feature (modeled threads)
- [ ] #56 Helix and spiral curves
- [ ] #57 Curve-driven and sketch-driven patterns
- [ ] #58 Move/Copy Body and Scale
- [ ] #59 Dome and Wrap

## [v0.5.0 – Files & Interop](https://github.com/Three-Bugs-Studio/3bcad/milestone/4)

Native save/load, STEP/DXF/mesh import, units and materials.

- [ ] #60 Native .3bcad document format (save / load)
- [ ] #61 File menu: New, Open, Save, Save As, recent files and unsaved-changes prompt
- [ ] #62 Autosave and crash recovery
- [ ] #63 STEP import (AP203/AP214/AP242)
- [ ] #64 STEP export: units, names and round-trip validation
- [ ] #65 Mesh import: STL, 3MF and OBJ as reference bodies
- [ ] #66 DXF import into sketches and DXF export of sketches
- [ ] #67 Document units and precision (mm / cm / inch)
- [ ] #68 Material library
- [ ] #69 Fuzz testing for all importers

## [v0.6.0 – 3D Print Studio](https://github.com/Three-Bugs-Studio/3bcad/milestone/5)

Printability checks, orientation and rich 3MF for modern slicers.

- [ ] #70 Overhang analysis overlay
- [ ] #71 Thin wall and minimum feature size check
- [ ] #72 Print orientation: place on face and preview on build plate
- [ ] #73 Rich 3MF: plates, colours, materials and print settings metadata
- [ ] #74 Slicer settings: custom executable paths and default slicer
- [ ] #75 Export quality settings: chord and angle tolerance per export
- [ ] #76 Fit/clearance presets for printed assemblies

## [v0.7.0 – Assemblies](https://github.com/Three-Bugs-Studio/3bcad/milestone/6)

Multi-part assemblies with mates, interference checks, exploded views and BOM.

- [ ] #77 Assembly document and component tree
- [ ] #78 Standard mates: coincident, concentric, distance, angle, parallel, perpendicular, tangent, lock
- [ ] #79 3D assembly constraint solver
- [ ] #80 Move and rotate components with the mouse
- [ ] #81 Interference detection
- [ ] #82 Exploded views
- [ ] #83 Bill of materials
- [ ] #84 Export assemblies to STEP and multi-object 3MF

## [v0.8.0 – Drawings](https://github.com/Three-Bugs-Studio/3bcad/milestone/7)

2D engineering drawings with projected/section views, dimensions and PDF/DXF export.

- [ ] #85 Drawing document, sheets and title blocks
- [ ] #86 Standard views and projected views with hidden-line removal
- [ ] #87 Section and detail views
- [ ] #88 Dimensions and annotations in drawings
- [ ] #89 Export drawings to PDF, SVG and DXF

## [v0.9.0 – Design Intent & Configurations](https://github.com/Three-Bugs-Studio/3bcad/milestone/8)

Equations, global variables, configurations and appearances.

- [ ] #90 Global variables and equations
- [ ] #91 Configurations
- [ ] #92 Appearances: colour per body, face and feature
- [ ] #93 Custom document properties

## [v1.0.0 – Production Ready](https://github.com/Three-Bugs-Studio/3bcad/milestone/9)

Polished UX, installers, signing, performance, docs and accessibility.

- [ ] #94 View controls: standard views, view cube and orthographic projection
- [ ] #95 Display styles: shaded with edges, shaded, hidden lines removed/visible, wireframe
- [ ] #96 Section view in the viewport
- [ ] #97 Light and dark themes
- [ ] #98 Customizable keyboard shortcuts and mouse gestures
- [ ] #99 Settings / System Options dialog
- [ ] #100 Windows installer (MSI) with Start menu entry and file associations
- [ ] #101 Code signing: Windows Authenticode and macOS Developer ID + notarization
- [ ] #102 macOS DMG and Linux AppImage/Flatpak with desktop entry
- [ ] #103 App icon and branding
- [ ] #104 Update check: notify when a new release is available
- [ ] #105 Log file and crash reports for release builds
- [ ] #106 Performance: tessellate off the UI thread, LOD and large models
- [ ] #107 User guide and tutorials (mdBook)
- [ ] #108 Localization framework (English, Vietnamese)
- [ ] #109 Accessibility: UI scaling, high contrast and keyboard navigation

## [Future](https://github.com/Three-Bugs-Studio/3bcad/milestone/10)

Post-1.0 workbenches: sheet metal, weldments, surfacing, simulation, scripting.

- [ ] #110 Sheet metal: base flange, edge flange, bends and flat pattern
- [ ] #111 Weldments: structural members from profiles
- [ ] #112 Surface modeling: extruded/revolved/lofted surfaces, knit, thicken
- [ ] #113 Sketch text and emboss/deboss
- [ ] #114 Split part for the printer bed with alignment pins
- [ ] #115 Scripting and plugin API
- [ ] #116 Simulation: linear static FEA
- [ ] #117 Photorealistic rendering
- [ ] #118 Web build (WebGPU)
