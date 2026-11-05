<p align="center">
  <img src="assets/logo-wordmark.svg" alt="3BCAD by 3 Bugs Studio" width="440">
</p>

<p align="center">
  <a href="https://github.com/Three-Bugs-Studio/3bcad/actions/workflows/ci.yml"><img src="https://github.com/Three-Bugs-Studio/3bcad/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Three-Bugs-Studio/3bcad/releases/latest"><img src="https://img.shields.io/github/v/release/Three-Bugs-Studio/3bcad" alt="Latest release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0"></a>
</p>

# 3BCAD

**Open-source parametric 3D CAD, written in Rust, for makers, mechanical engineers and the 3D-printing
community.**

3BCAD is a feature-based solid modeller. You draw constrained 2D sketches, turn them into solids with
features (extrude, revolve, sweep, loft, fillet, shell, holes, patterns...), and every feature stays
editable: change a dimension and the whole part rebuilds. When the part is done, the built-in 3D Print
studio lays it out on your printer's bed, shows where it needs support or is too thin to print, adds
clearances so printed parts fit together, and sends it to your slicer.

Everything is written in memory-safe Rust: the B-Rep geometry kernel ([`truck`](https://github.com/ricosjp/truck)),
the 2D constraint solver, the feature history, the GPU viewport and the file formats. There is no C++
toolchain to install, and one `cargo run` builds the whole application on Windows, macOS and Linux.

> **Status: early preview (v0.x).** You can sketch, model, save and reopen parametric parts, import STEP,
> meshes and DXF, check and orient them for printing, and export STL / 3MF / STEP / DXF. Assemblies and
> drawings are on the [roadmap](#roadmap). Expect breaking changes before 1.0.

## Contents

- [Features](#features)
- [Your first part](#your-first-part)
- [Printing a part](#printing-a-part)
- [Download](#download) and [build from source](#build-from-source)
- [Controls](#controls)
- [Files](#files)
- [How it is built](#how-it-is-built)
- [Roadmap](#roadmap), [contributing](#contributing) and [license](#license)

## Features

### Sketching

- Sketch on the Front, Top or Right plane, on a reference plane, or on any flat face of the model.
- Lines, rectangles (corner, center, 3-point), circles, arcs (center, tangent, 3-point), splines, slots and
  polygons; sketch fillets and chamfers, trim, extend, offset, mirror, linear and circular patterns, and
  converting model edges into the sketch.
- Relations: coincident, horizontal, vertical, parallel, perpendicular, collinear, equal, midpoint,
  concentric, symmetric, tangent, point-on-curve and more. Inferencing adds them while you draw; hold `Ctrl`
  to draw freely.
- Dimensions placed in the viewport and edited in place. A Levenberg-Marquardt solver with exact
  derivatives keeps the sketch solved as you drag it, and colours show whether a sketch is under-defined
  (blue), fully defined (black) or over-defined (red, with the conflicting relations listed).

### Part modelling

- Extrude and revolve, adding or cutting material, with blind, through-all, up-to-next, up-to-vertex,
  up-to-surface, offset-from-surface and mid-plane end conditions, two directions, draft angles and thin walls.
- Sweeps along a sketch path or a helix (with twist), and lofts through several profiles with guide curves.
- Fillets (constant, variable and face-to-face), chamfers, shell, draft, rib, dome and wrap.
- A hole tool with ISO and ANSI simple, counterbored, countersunk and tapped holes, and modelled threads
  on rods and in holes.
- Linear, circular, mirror, sketch-driven and curve-driven patterns of features or bodies.
- Reference planes, axes, points and coordinate systems.
- Multibody parts: combine (add, subtract, common), split, delete or keep, move or copy, and scale bodies,
  listed in a Solid Bodies folder.
- A feature tree with a draggable rollback bar, rename, reorder, suppress, edit and delete, persistent face
  and edge names (features keep pointing at the right geometry when dimensions change), incremental
  rebuilds, and undo/redo for every change.

### 3D Print studio

- **Bed layout**: pick your printer (Bambu Lab X1 / P1 / A1 / A1 mini, Prusa MK4 / MINI / XL, Creality
  Ender 3, Voron 2.4 or a custom build volume), choose the face the part rests on, and see it on the
  build plate, with a warning when it does not fit.
- **Overhangs**: faces steeper than your printer's overhang limit are coloured, measured from the real
  build direction, so you can turn the part until it needs less support.
- **Thin walls**: walls thinner than the nozzle (red) or than your minimum wall (orange) are coloured.
- **Fit clearances**: moves a hole or pin (one click picks all of it) or the faces of a tab or slot by a
  press, slip or loose clearance, with presets for FDM (PLA/PETG, ABS/ASA/nylon), resin and powder printers.
- **Export quality**: coarse, fine or custom chord and angle tolerance, with the triangle count shown
  before you export.
- **3MF with names, materials and colours**: one named object per body, the part's material and colour,
  and the printer and nozzle it was prepared for; STL and 3MF are written on the bed, ready to slice.
- **Send to Slicer** opens the part in OrcaSlicer, Bambu Studio or PrusaSlicer. The chosen slicer is
  remembered, and if it is installed somewhere unusual you can point 3BCAD at it once.

### Files and interop

- The native `.3bcad` format stores the whole feature history as versioned, diff-friendly JSON. Saves are
  atomic, damaged files and files from a newer 3BCAD are refused with a clear message, and autosave with
  crash recovery keeps your work safe.
- Import STEP solids (AP203 / AP214 / AP242), STL / 3MF / OBJ meshes as reference bodies, and DXF drawings
  as sketches.
- Export STL, 3MF, STEP (exact geometry, millimetres, the part's name) and DXF of a sketch or flat face.
- Units per part (millimetre, centimetre or inch systems, with your choice of decimals): type `1in` in a
  millimetre part and it converts.
- A material library (PLA, PETG, ABS, ASA, TPU, nylon, PC, resin, aluminium, steels, brass, copper,
  titanium, oak) or your own materials, used for mass and for the model's colour.

### Evaluate

- Measure distances, angles, radii and lengths of the selection.
- Mass properties: volume, area, mass, center of mass and moments of inertia.
- A rebuild errors panel that lists every failing feature with a suggested fix, instead of failing the
  whole part.

## Your first part

1. **Sketch.** On the **Sketch** tab, click **Top**. Draw a rectangle with the corner rectangle tool, then
   pick **Smart Dimension**, click an edge, click where the dimension goes and type its length.
   **Exit Sketch** (or the green check in the corner).
2. **Extrude.** With the sketch selected, click **Extruded Boss/Base** on the **Features** tab and type the
   depth in the Properties panel on the right. Every value there stays editable later.
3. **Add a hole.** Click the top face, then the hole tool: a hole goes at the face's center. Pick a size
   and type (counterbore, countersink, tapped) in the Properties panel.
4. **Round it.** Ctrl+click a few edges and click **Fillet**.
5. **Change your mind.** Double-click the first sketch in the feature tree, change a dimension and exit:
   the extrude, hole and fillets follow.
6. **Save** with `Ctrl+S`.

## Printing a part

1. Open the **3D Print** tab. The part appears on the bed of the selected printer, standing on its bottom.
2. To print it on another face, click that face and **Lay Flat** (or select an edge or axis to point it
   up). **Reset** puts it back on its bottom.
3. Turn on **Overhangs** and **Thin Walls** to see what needs support and what is too thin; set the
   overhang angle, nozzle and minimum wall for your printer. The colours update as you edit the part.
4. For parts that fit together, select the faces of a hole or pin and click **Fit** on the **Features**
   tab, then choose a press, slip or loose preset.
5. Pick a **Quality** and **Export 3MF** or **Export STL**, or choose your slicer and **Send to Slicer**.

## Download

Get the desktop app from the [Releases page](https://github.com/Three-Bugs-Studio/3bcad/releases/latest):

| Platform | File | How to run |
| --- | --- | --- |
| Windows 10/11 (x64) | `3BCAD-<version>-windows-x64.zip` | Unzip, run `3bcad.exe` |
| macOS 11+ (Apple Silicon and Intel) | `3BCAD-<version>-macos-universal.zip` | Unzip, right-click `3BCAD.app` → **Open** |
| Linux (x64) | `3BCAD-<version>-linux-x64.tar.gz` | Extract, run `./3bcad` |

Builds are not code-signed yet, so Windows SmartScreen and macOS Gatekeeper warn on first launch.
The version you are running is shown in the title bar; see [CHANGELOG.md](CHANGELOG.md) for what changed in
each release.

## Build from source

You need Rust **1.87+** ([rustup.rs](https://rustup.rs)) and a GPU with Vulkan, DirectX 12 or Metal.

```bash
git clone https://github.com/Three-Bugs-Studio/3bcad.git
cd 3bcad
cargo run --release
```

On Linux, install the windowing and file-dialog libraries first, e.g. on Debian/Ubuntu:

```bash
sudo apt-get install libgtk-3-dev libxkbcommon-dev libwayland-dev
```

Run the checks CI runs before sending a change:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Controls

| Action | Input |
| --- | --- |
| Orbit | Left drag (or middle drag) |
| Pan | Right drag (or Ctrl + middle drag) |
| Zoom | Mouse wheel |
| Zoom to fit | `F` or double-click |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| New / Open / Save / Save As | `Ctrl+N` / `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S`, or the **File** menu |
| Import STEP, STL, 3MF, OBJ or DXF | **File → Import…** |
| Change units or material | Click the unit system in the status bar, or **File → Units / Material** |
| Roll back | Drag the blue bar in the feature tree, or right-click a feature |
| Start a sketch | Sketch tab → Front / Top / Right (or a reference plane), double-click a plane in the feature tree, or select a flat face → Sketch |
| Edit a sketch | Double-click it in the feature tree or the graphics area |
| Disable snapping while drawing | Hold `Ctrl` |
| Edit a dimension | Double-click its value |
| Fillet / Chamfer / Shell / Fit | Select edges or faces (Ctrl+click for more), then the Features tab |
| Sweep / Loft | Select the profile sketch, Ctrl+click the path (or the next profiles), then the Features tab |
| Print on another face | 3D Print tab: select the face → **Lay Flat** |
| Rename a feature | Select it and press `F2` |
| Rebuild everything | `Ctrl+Q` |
| Keyboard shortcuts | `F1` or the Shortcuts button (top right) |

## Files

| File | What it is |
| --- | --- |
| `.3bcad` | A part: its feature history, units, material and print orientation as pretty-printed JSON. Geometry is rebuilt on open. |
| `settings.txt` | Preferences (recent files, autosave, slicer and its location, printer, check and export settings) in `%APPDATA%\3BCAD`, `~/Library/Application Support/3BCAD` or `~/.config/3bcad`. |
| `recovery/` | Autosaves next to `settings.txt`, removed after a save or a normal exit. |

## How it is built

3BCAD is a Cargo workspace of six crates, each owning one system:

| Crate (folder) | Package | Role |
| --- | --- | --- |
| `crates/3bcad-core` | `threebcad-core` | Feature history graph (`petgraph`), rebuild with persistent face naming and caching, part documents, undo/redo |
| `crates/3bcad-sketch` | `threebcad-sketch` | 2D entities, relations, the Levenberg-Marquardt solver, profiles and sketch tools |
| `crates/3bcad-kernel` | `threebcad-kernel` | `truck` B-Rep bridge: feature tools, booleans, fillets, threads, tessellation, mass properties, print checks |
| `crates/3bcad-render` | `threebcad-render` | wgpu pipelines, orbit camera, scene buffers and picking |
| `crates/3bcad-io` | `threebcad-io` | STL, 3MF, OBJ, STEP and DXF import/export, printer presets, slicer launcher |
| `crates/3bcad-app` | `threebcad-app` | Desktop app (`eframe` = winit + egui + egui-wgpu), binary `3bcad` |

Cargo package names cannot start with a digit, so packages are named `threebcad-*`.
The [architecture guide](docs/architecture.md) explains the data flow of a rebuild and walks through
adding a feature type or a sketch relation. Every importer has a fuzz target in `fuzz/`, run nightly.

## Roadmap

3BCAD is working toward the everyday parametric CAD workflow (parts, assemblies, drawings) plus first-class
3D-printing tools. See [ROADMAP.md](ROADMAP.md) for every planned task, each tracked as a GitHub issue:

| Milestone | Focus |
| --- | --- |
| v0.2.0 | Interactive sketcher: picking, sketch tools, relations, dimensions (done) |
| v0.3.0 | Core part features: cut, revolve, fillet, chamfer, shell, holes, patterns (done) |
| v0.4.0 | Advanced modeling: sweep, loft, multibody, threads (done) |
| v0.5.0 | Files: native save/load, STEP/DXF/mesh import, units, materials (done) |
| v0.6.0 | 3D Print studio: overhang and thin-wall checks, orientation, fits, rich 3MF (done, unreleased) |
| v0.7.0 | Assemblies: mates, interference, exploded views, BOM |
| v0.8.0 | Drawings: views, sections, dimensions, PDF/DXF |
| v0.9.0 | Design intent: equations, configurations, appearances |
| v1.0.0 | Production ready: installers, signing, performance, docs, accessibility |

## Contributing

Contributions are welcome! Read [CONTRIBUTING.md](CONTRIBUTING.md), the
[architecture guide](docs/architecture.md) and our [Code of Conduct](CODE_OF_CONDUCT.md). Issues labelled
`good first issue` are a good place to start. To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

3BCAD is an independent open-source project and is not affiliated with, endorsed by or sponsored by any CAD
vendor. Product names and trademarks belong to their owners.
