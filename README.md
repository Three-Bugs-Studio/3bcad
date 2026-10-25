# 3BCAD

**Open-source parametric 3D CAD in Rust, built for makers, mechanical engineers and the 3D-printing community.**

3BCAD copies the module split that made SolidWorks work (B-Rep kernel, 2D constraint solver,
feature history, GPU viewport, print tooling) and builds each part in memory-safe Rust with no C++ toolchain.

> **Status: early prototype (Phase 1).** You can model boxes and cylinders from parametric sketches,
> edit dimensions, roll back the feature tree, and export to STL / 3MF / STEP. Expect breaking changes.

## Features (Phase 1)

- **wgpu 3D viewport**: shaded faces, B-Rep edge highlights, ground grid, XYZ triad, orbit/pan/zoom camera.
- **SolidWorks-style layout (egui)**: CommandManager ribbon, FeatureManager tree with a draggable
  rollback bar, PropertyManager for live dimension editing, undo/redo.
- **Parametric sketches**: points, lines and circles with Fix, Coincident, Horizontal, Vertical, Distance,
  Angle, Tangent and Radius constraints, solved with Levenberg-Marquardt least squares.
- **B-Rep modelling** on the pure-Rust [`truck`](https://github.com/ricosjp/truck) kernel: Extruded Boss/Base
  from rectangle and circle profiles.
- **3D-print export**: watertight binary STL and 3MF (one object per body), exact STEP, and
  **Send to Slicer** for OrcaSlicer, Bambu Studio and PrusaSlicer.

## Download

Get the desktop app from the [Releases page](https://github.com/Three-Bugs-Studio/3bcad/releases/latest):

| Platform | File | How to run |
| --- | --- | --- |
| Windows 10/11 (x64) | `3BCAD-<version>-windows-x64.zip` | Unzip, run `3bcad.exe` |
| macOS 11+ (Apple Silicon and Intel) | `3BCAD-<version>-macos-universal.zip` | Unzip, right-click `3BCAD.app` → **Open** |
| Linux (x64) | `3BCAD-<version>-linux-x64.tar.gz` | Extract, run `./3bcad` |

Builds are not code-signed yet, so Windows SmartScreen and macOS Gatekeeper warn on first launch.
The version you are running is shown in the title bar; see [CHANGELOG.md](CHANGELOG.md) for what changed in each release.

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

### Controls

| Action | Input |
| --- | --- |
| Orbit | Left drag (or middle drag) |
| Pan | Right drag (or Ctrl + middle drag) |
| Zoom | Mouse wheel |
| Zoom to fit | `F` or double-click |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` |
| Roll back | Drag the blue bar in the FeatureManager, or right-click a feature |

## Workspace layout

| Crate (folder) | Package | Role |
| --- | --- | --- |
| `crates/3bcad-core` | `threebcad-core` | Feature history DAG (`petgraph`), rollback, part document, undo/redo |
| `crates/3bcad-sketch` | `threebcad-sketch` | 2D entities, constraints, Levenberg-Marquardt solver |
| `crates/3bcad-kernel` | `threebcad-kernel` | `truck` B-Rep bridge: extrude operations, tessellation |
| `crates/3bcad-render` | `threebcad-render` | wgpu pipelines, orbit camera, scene buffers |
| `crates/3bcad-io` | `threebcad-io` | STL, 3MF, STEP export, slicer launcher |
| `crates/3bcad-app` | `threebcad-app` | Desktop app (`eframe` = winit + egui + egui-wgpu), binary `3bcad` |

Cargo package names cannot start with a digit, so packages are named `threebcad-*`.

## Roadmap

- [ ] Extruded Cut, Revolve, Fillet/Chamfer (boolean ops via `truck-shapeops`)
- [ ] Interactive sketch mode (draw and pick in the viewport), arcs, sketch on faces/planes
- [ ] Save/load native documents, STEP import
- [ ] Incremental rebuild (only re-evaluate dirty features)
- [ ] Assemblies and mates

## Contributing

Contributions are welcome! Read [CONTRIBUTING.md](CONTRIBUTING.md) and our
[Code of Conduct](CODE_OF_CONDUCT.md). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

SolidWorks, Parasolid and D-Cubed are trademarks of their respective owners. 3BCAD is not affiliated with them.
