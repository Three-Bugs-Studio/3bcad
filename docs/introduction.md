# Introduction

3BCAD is a free, open-source parametric CAD program for Windows, macOS and Linux. You draw 2D sketches
with dimensions, turn them into solids with features such as extrude, revolve, fillet, shell and holes,
and every value stays editable: change a dimension and the part rebuilds. Parts go together in
assemblies and onto engineering drawings, and the 3D Print tab gets them ready for your printer and
slicer.

This guide shows you how to:

- [install 3BCAD](guide/installing.md) and [model your first part](guide/getting-started.md);
- use each tool, in the [reference](reference/sketching.md) chapters;
- build three printable parts from start to finish: a [phone stand](tutorials/phone-stand.md), an
  [enclosure with a lid](tutorials/enclosure.md) and a [threaded bottle cap](tutorials/bottle-cap.md);
- [fix common problems and report a bug](guide/troubleshooting.md).

## Words used in this guide

- **Ribbon**: the row of commands at the top. Its tabs (**Features**, **Sketch**, **Evaluate**,
  **3D Print**) sit under the commands.
- **Feature tree**: the panel on the left. It lists the planes, the bodies and every feature in the
  order they are built.
- **Properties panel**: the panel on the right. It shows the values of the selected feature, or the
  sketch tools' options while you sketch.
- **Graphics area**: the 3D view in the middle.
- **Status bar**: the line at the bottom. It shows messages, the unit system, the number of bodies and
  any rebuild errors.

Lengths are in millimetres unless a step says otherwise. The Z axis points up, so the **Top Plane** is
the XY plane (the printer's bed), the **Front Plane** is XZ and the **Right Plane** is YZ.

3BCAD is still before version 1.0. Saved files and workflows can change between minor versions; the
[changelog](https://github.com/Three-Bugs-Studio/3bcad/blob/main/CHANGELOG.md) lists what changed in each
release.
