<p align="center">
  <img src="assets/logo-wordmark.svg" alt="3BCAD by 3 Bugs Studio" width="440">
</p>

<p align="center">
  <a href="https://github.com/Three-Bugs-Studio/3bcad/actions/workflows/ci.yml"><img src="https://github.com/Three-Bugs-Studio/3bcad/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Three-Bugs-Studio/3bcad/releases/latest"><img src="https://img.shields.io/github/v/release/Three-Bugs-Studio/3bcad" alt="Latest release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b>Download:</b>
  <a href="https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-windows-x64.msi">Windows</a> ·
  <a href="https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-macos-universal.dmg">macOS</a> ·
  <a href="https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-linux-x86_64.AppImage">Linux</a> ·
  <a href="#download">other builds</a>
</p>

# 3BCAD

**Open-source parametric 3D CAD, written in Rust, for makers, mechanical engineers and the 3D-printing
community.**

3BCAD is a feature-based solid modeller. You draw constrained 2D sketches, turn them into solids with
features (extrude, revolve, sweep, loft, fillet, shell, holes, patterns...), and every feature stays
editable: change a dimension and the whole part rebuilds. Parts go together in assemblies, held by mates
you can drag against, checked for interference, exploded step by step and listed in a bill of materials.
When the part is done, the built-in 3D Print studio lays it out on your printer's bed, shows where it needs
support or is too thin to print, adds clearances so printed parts fit together, and sends it to your
slicer.

Everything is written in memory-safe Rust: the B-Rep geometry kernel ([`truck`](https://github.com/ricosjp/truck)),
the 2D constraint solver, the feature history, the GPU viewport and the file formats. There is no C++
toolchain to install, and one `cargo run` builds the whole application on Windows, macOS and Linux.

> **Status: 1.0, ready for everyday use.** You can sketch, model, save and reopen parametric parts, put them
> together in assemblies, make engineering drawings of them, open STEP, STL, 3MF, OBJ and DXF files from other
> apps, check and orient parts for printing, and export STL / 3MF / STEP / DXF / PDF / SVG. It comes with
> installers for Windows, macOS and Linux, updates itself, and speaks English and Vietnamese. Files saved by
> earlier versions still open. Next up is in the [roadmap](ROADMAP.md).

## Contents

- [Features](#features)
- [Your first part](#your-first-part)
- [Printing a part](#printing-a-part)
- [Your first assembly](#your-first-assembly)
- [Your first drawing](#your-first-drawing)
- [User guide and tutorials](https://three-bugs-studio.github.io/3bcad/) (also in [`docs/`](docs/SUMMARY.md))
- [Download](#download) ([code signing policy](#code-signing-policy)) and [build from source](#build-from-source)
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
- Global variables and equations with units (`wall = 2`, `D1@Boss-Extrude1 = wall * 10 + 1in`), solved
  in dependency order, with circular references shown on the equation that has them.
- Configurations: variants of a part with their own dimensions and suppressed features, switched from the
  feature tree and exported one at a time.
- Appearances: a colour per face, body or feature, shown in the viewport and written to 3MF and STEP.

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

### Assemblies

- Insert parts and sub-assemblies as components. The first one is fixed at the origin; each part stays in
  its own file, and the assembly picks up a part's changes as soon as the file is saved.
- Mates between flat and cylindrical faces: coincident, concentric, distance, angle, parallel,
  perpendicular, tangent and lock, each with Flip. A solver places the parts, and a mate that contradicts
  the others is named in the tree instead of moving anything.
- Drag a component to move it, Shift+drag to turn it: it moves only where its mates let it.
- Interference check: parts that overlap are listed with the shared volume and shown in red; parts that
  only touch, like a pin in a hole of its own size, are fine.
- Exploded views, step by step, played as an animation or scrubbed with a slider.
- Bill of materials, top-level or indented, with part numbers, descriptions, materials and your own
  custom properties, exported as CSV.
- Export the assembly as STEP (each part once, every component named and placed) or as a 3MF with one
  object per part, laid out on the bed for the slicer.

### Drawings

- Drawing sheets in ISO A4 to A0 or ANSI A to E, as many as you need, each with its own scale and first- or
  third-angle projection, and a title block whose cells fill in from the part's and the drawing's
  properties (part number, material, scale, sheet 1 of 2...).
- Standard views and projected views with hidden lines removed or dashed, lined up with the view they come
  from. The drawing reads the part from its file, so the views follow every change to the part.
- Section views (A-A) hatched for the part's material, and detail views of a circle at a larger scale.
- Dimensions brought in from the part's sketches and extrusions, reference dimensions measured off a view
  (lengths, diameters, radii), notes with leaders, centre marks and centrelines.
- Export every sheet as one true-scale vector PDF, or an SVG or a layered DXF per sheet.

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

## Your first assembly

1. Save the parts you want to put together (for example a plate with a hole and a pin) as `.3bcad` files.
2. **File → New Assembly**, then **Insert** on the **Assembly** tab and pick the plate: it is fixed at the
   origin. Insert the pin; it lands beside the plate.
3. Click the pin's round face, Ctrl+click the hole's face and click **Concentric**. Click the pin's end and
   Ctrl+click the plate's bottom, then **Coincident** (tick **Flip alignment** in the Properties panel if it
   lands the wrong way round).
4. Drag the pin: it only turns in the hole. Delete the coincident mate and it slides along it too.
5. Click **Interference** to check that nothing overlaps, **Explode Step** to pull the pin out along a
   picked face, and **BOM** for the parts list (part numbers come from **File → Properties…**).
6. To change a part, select it and click **Edit Part**; save it and click **Back to** the assembly.
7. Export the assembly as **STEP**, or as **3MF** to print every part in one go.

## Your first drawing

1. Save the part (for example a bracket with a hole). **File → New Drawing**, then **Model** on the
   **Drawing** tab and pick the part: the front view and two projected views appear on an A3 sheet.
2. Click the sheet in the tree to set its size, scale and projection angle, and its title block cells.
   Cells like `{Part number}` fill in from **File → Properties…** of the part or the drawing.
3. Click **Section**, then click a view where the cut goes: the section view lands beside it, hatched.
   **Detail**, then a click on a view, shows that area twice as large.
4. **Model Items** brings in the part's dimensions; **Smart Dimension** measures an edge, a circle or two
   vertices; **Note** adds a note with a leader to what you click. Drag views, dimensions and notes to
   tidy the sheet.
5. Change the part (**Edit Part**, save, **Back to** the drawing): the views and dimensions follow.
6. Export **PDF** (every sheet, print at 100 %), **SVG** or **DXF**.

## Download

Click the link for your computer; it always downloads the newest version:

| Platform | Installer | Or the portable build |
| --- | --- | --- |
| Windows 10/11 (x64) | [**3BCAD-windows-x64.msi**](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-windows-x64.msi): installs for you (no admin rights), adds a Start menu entry, opens `.3bcad` files and appears under "Open with" for STEP, STL, 3MF, OBJ and DXF | [.zip](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-windows-x64.zip): unzip, run `3bcad.exe` |
| macOS 11+ (Apple Silicon and Intel) | [**3BCAD-macos-universal.dmg**](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-macos-universal.dmg): drag `3BCAD.app` onto Applications | [.zip](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-macos-universal.zip) |
| Linux (x64) | [**3BCAD-linux-x86_64.AppImage**](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-linux-x86_64.AppImage): `chmod +x` it and run it | [.tar.gz](https://github.com/Three-Bugs-Studio/3bcad/releases/latest/download/3BCAD-linux-x64.tar.gz): extract, run `./3bcad` |

Release notes, checksums and older versions are on the [Releases page](https://github.com/Three-Bugs-Studio/3bcad/releases).

Windows releases are being set up for code signing through the SignPath Foundation (see the
[code signing policy](#code-signing-policy)). Until a release is signed, Windows SmartScreen ("More info → Run
anyway") and macOS Gatekeeper (right-click `3BCAD.app` → **Open**) warn on first launch, and even signed downloads
can show a SmartScreen warning for a while until they have built up reputation.
The version you are running is shown in the title bar; see [CHANGELOG.md](CHANGELOG.md) for what changed in
each release.

From 0.8.0 on, 3BCAD checks for a newer release when it starts and offers **Update now**: it downloads the
release for your computer, checks it against the release's `SHA256SUMS.txt`, installs it in place and restarts.
Turn the check off with **File → Check for updates at start**, or check at any time with **File → Check for
Updates…**. It needs `curl` and `tar`, which Windows 10 and later, macOS and Linux include.

### Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

- Committers and reviewers: [members of Three Bugs Studio](https://github.com/orgs/Three-Bugs-Studio/people)
- Approvers: owners of the [Three Bugs Studio organization](https://github.com/Three-Bugs-Studio)

Only programs built from this repository by its GitHub Actions release workflow are signed, and every signing
request is approved by hand. Changes from people outside the team are reviewed by a team member before they are
merged. See [RELEASING.md](RELEASING.md#code-signing) for how it works.

**Privacy:** 3BCAD sends nothing about you, your files or your use of it. The only network request it makes on its
own is the update check at start, which asks GitHub for the newest release (GitHub's
[privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) applies);
turn it off with **File → Check for updates at start**. Updates are downloaded only when you choose **Update now**,
and links in the app (such as the **Help** menu) open in your browser only when you click them.

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
| Standard views | `Ctrl+1` … `Ctrl+7` (Front, Back, Left, Right, Top, Bottom, Isometric), `Ctrl+8` Normal To, the View toolbar or the view cube |
| Perspective / orthographic | `P`, or **Perspective** in the View toolbar |
| Display style | `D` (next style), or the View toolbar |
| Section view | **Section** in the View toolbar |
| Turn the view from the keyboard | Arrow keys (15°, with Shift 90°), `Ctrl`+arrows pan, Page Up / Down zoom |
| The tab's commands at the mouse | `S` |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| New / Open / Save / Save As | `Ctrl+N` / `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S`, or the **File** menu |
| Open STEP, STL, 3MF, OBJ or DXF as a new part | **File → Open…**, or drag the file onto the window |
| Import STEP, STL, 3MF, OBJ or DXF into the open part | **File → Import…** |
| Settings: language (English, Tiếng Việt), theme, interface size, shortcuts | **File → Settings…** (`Ctrl+,`) |
| Change units or material | Click the unit system in the status bar, or **File → Units / Material** |
| Roll back | Drag the blue bar in the feature tree, or right-click a feature |
| Start a sketch | Sketch tab → Front / Top / Right (or a reference plane), double-click a plane in the feature tree, or select a flat face → Sketch |
| Edit a sketch | Double-click it in the feature tree or the graphics area |
| Disable snapping while drawing | Hold `Ctrl` |
| Edit a dimension | Double-click its value |
| Fillet / Chamfer / Shell / Fit | Select edges or faces (Ctrl+click for more), then the Features tab |
| Sweep / Loft | Select the profile sketch, Ctrl+click the path (or the next profiles), then the Features tab |
| Print on another face | 3D Print tab: select the face → **Lay Flat** |
| Mate two parts | Click a face, Ctrl+click a face of the other part, then a mate on the Assembly tab |
| Move or turn a component | Drag it, or Shift+drag it (assemblies) |
| Delete a component, mate or explode step | Select it and press `Delete` |
| Rename a feature | Select it and press `F2` |
| Variables, equations and configurations | **Evaluate → Equations**, or the **Configuration** box at the top of the tree |
| Colour a face, body or feature | Pick it and **Evaluate → Appearance**, or right-click a feature or body in the tree |
| Rebuild everything | `Ctrl+Q` |
| Keyboard shortcuts | `F1` or the Shortcuts button (top right); change them in **Settings → Shortcuts** |
| Log file for a bug report | **Help → Open Log Folder** |

## Files

| File | What it is |
| --- | --- |
| `.3bcad` | A part: its feature history, equations, configurations, colours, units, material, print orientation and custom properties as pretty-printed JSON. Geometry is rebuilt on open. |
| `.3bcasm` | An assembly: the files it uses (relative to the assembly, so they can move together), where each component sits, its mates and exploded view. |
| `.3bcdrw` | A drawing: the part it shows (relative to the drawing), its sheets, views, dimensions, notes and title block. Views are projected again on open. |
| `settings.txt` | Preferences (recent files, autosave, slicer and its location, printer, check and export settings) in `%APPDATA%\3BCAD`, `~/Library/Application Support/3BCAD` or `~/.config/3bcad`. |
| `recovery/` | Autosaves next to `settings.txt`, removed after a save or a normal exit. |

## How it is built

3BCAD is a Cargo workspace of six crates, each owning one system:

| Crate (folder) | Package | Role |
| --- | --- | --- |
| `crates/3bcad-core` | `threebcad-core` | Feature history graph (`petgraph`), rebuild with persistent face naming and caching, part, assembly and drawing documents, bill of materials, undo/redo |
| `crates/3bcad-sketch` | `threebcad-sketch` | 2D entities, relations, the Levenberg-Marquardt solver, profiles and sketch tools |
| `crates/3bcad-kernel` | `threebcad-kernel` | `truck` B-Rep bridge: feature tools, booleans, fillets, threads, tessellation, mass properties, print checks, the mate solver, interference and hidden-line removal |
| `crates/3bcad-render` | `threebcad-render` | wgpu pipelines, orbit camera, scene buffers and picking |
| `crates/3bcad-io` | `threebcad-io` | STL, 3MF, OBJ, STEP and DXF import/export, drawing PDF / SVG / DXF, printer presets, slicer launcher |
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
| v0.6.0 | 3D Print studio: overhang and thin-wall checks, orientation, fits, rich 3MF (done) |
| v0.7.0 | Assemblies: mates, interference, exploded views, BOM (done) |
| v0.8.0 | Drawings: views, sections, dimensions, PDF/DXF (done) |
| v0.9.0 | Design intent: equations, configurations, appearances (done, unreleased) |
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

3BCAD was created by Duong Phu Dong at Three Bugs Studio. If you share or build on it, keep the license files and
[NOTICE](NOTICE), and say your version is based on 3BCAD by Three Bugs Studio. The names and logos are covered by
the [trademark policy](TRADEMARKS.md): a modified version needs its own name. To cite 3BCAD, use **Cite this
repository** on GitHub ([CITATION.cff](CITATION.cff)).

3BCAD is an independent open-source project and is not affiliated with, endorsed by or sponsored by any CAD
vendor. Product names and trademarks belong to their owners.
