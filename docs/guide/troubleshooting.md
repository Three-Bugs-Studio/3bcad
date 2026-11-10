# Troubleshooting and reporting a bug

## A feature turns red

A feature that cannot be built turns red in the feature tree, and the status bar shows **⚠ errors,
warnings**. Click that button, or **What's Wrong** on the **Evaluate** tab, to list every failing feature
with a suggested fix. The rest of the part still builds. The usual causes:

- **Missing reference**: the face or edge the feature used is gone, for example after you deleted the
  feature that made it. Select the feature, pick the face or edge again and click **Use selection** in the
  Properties panel.
- **The boolean operation failed**: the geometry kernel could not intersect two faces, most often faces
  that lie exactly on top of each other. Change a dimension slightly, or untick **Merge result**.
- **Open or branching contour**: the sketch has no closed loop. Edit it and close the gap, or tick
  **Thin feature** for an open profile.
- **Over defined**: edit the sketch and click **Diagnose** to find the relations that conflict.
- **The parent feature has an error**: fix the first red feature in the tree; the ones after it often
  follow.

To find which step broke a part, drag the blue rollback bar up the tree one feature at a time, or
right-click a feature and choose **Suppress**.

## Other problems

- **The slicer does not open**: 3BCAD looks for it in its usual install folder and on the `PATH`. Click
  **Locate…** under the slicer list on the **3D Print** tab and pick the slicer's program.
- **The part does not fit the bed**: check the printer on the **3D Print** tab, and use **Lay Flat** to
  turn the part.
- **Printed parts do not fit together**: use **Fit** with a looser preset, or print a small test pair to
  find your printer's clearance. See [Fit clearances](../reference/printing.md#fit-clearances).
- **A file will not open**: a file saved by a newer 3BCAD is refused; update 3BCAD. A damaged file is
  refused too, with a message in the status bar.
- **3BCAD closed unexpectedly**: start it again and click **Recover** to get back the last autosave.
- **The window stays empty or 3BCAD does not start**: 3BCAD needs a graphics card with Vulkan, DirectX 12
  or Metal. Update your graphics driver.
- **Windows or macOS refuses to start 3BCAD**: see [Unsigned builds](installing.md#unsigned-builds).

## Known problems

These are limits of the geometry kernel in this version. Each has a way around it.

- **Some exports have small gaps.** Holes made with the **Hole Wizard**, **Up To Next** cuts, and fillets
  on some outside edges can export STL and 3MF meshes that are not quite closed. Most slicers repair this
  by themselves. To avoid it, make round holes with a circle and **Extruded Cut** (**Blind** or **Through
  All**), and use a chamfer where a fillet gives trouble.
- **Threads.** An inside thread works only in a hole that is open at both ends, along the whole hole: a
  blind hole or a partial length fails. Add nothing after the thread that touches the threaded ends. A
  thread on a revolved body fails with "the thread end does not meet a flat face of the body". Export
  threaded parts at **Fine** quality; **Coarse** can leave gaps. The
  [bottle cap tutorial](../tutorials/bottle-cap.md) shows a design that works.
- **Some shapes fail to join.** A cylinder sketched on the end of a cylinder of the same diameter, a boss
  sketched as an outline with a hole in it (one rectangle inside another) on a face, and a ring placed
  inside a round pocket all fail with "the boolean operation failed". Make a hollow boss as a solid boss
  and an **Extruded Cut**, as the lid's lip in the [enclosure tutorial](../tutorials/enclosure.md) does.

- **A few STEP files do not open, or open slightly off.** Files whose edges use curves the kernel cannot
  read are refused with a message, and an unusual surface (the dome of some screw heads) can come in a
  little wrong. Exporting the model again as STEP AP214 from its CAD program often helps; for printing,
  its STL or 3MF download opens as a mesh.

## Reporting a bug

Open an issue on GitHub with the
[bug report form](https://github.com/Three-Bugs-Studio/3bcad/issues/new/choose). It asks for:

- what you did, what you expected and what happened;
- the steps to reproduce it;
- the 3BCAD version, shown in the title bar and next to the 3BCAD name at the top left;
- your system and graphics card;
- the log, if you have one.

Attach the `.3bcad` file if you can: it is small, readable text, and it lets us rebuild your part exactly.
For a wrong import, attach the STEP, STL, 3MF, OBJ or DXF file too.

3BCAD writes a log as it runs. **Help > Open Log Folder** opens the folder: attach `3bcad.log` (this run)
or `3bcad.1.log` (the one before). After a crash, 3BCAD offers the crash report (`crash-<number>.txt`) the
next time it starts; attach that too. The folder is `%APPDATA%\3BCAD\logs` on Windows,
`~/Library/Application Support/3BCAD/logs` on macOS and `~/.config/3bcad/logs` on Linux.

Do not report security problems in a public issue. Use
[GitHub Security Advisories](https://github.com/Three-Bugs-Studio/3bcad/security/advisories/new) instead,
as described in the project's security policy.
