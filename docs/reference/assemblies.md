# Assemblies

An assembly (`.3bcasm`) puts saved parts and other assemblies together. Each part stays in its own file:
the assembly shows a part as it is now whenever its file changes.

## Start an assembly

1. Save the parts you need as `.3bcad` files.
2. Click **File > New Assembly**. The ribbon shows the **Assembly** tab.
3. Click **Insert** and pick a part or an assembly, or pick a recent file from **Recent**. The first
   component is fixed at the origin; the next ones land beside it.

The tree lists the components, the **Mates** and the **Exploded View** steps. Select a component to see
its **Name**, **Fixed** box and **Position** in the Properties panel. **Turn 90°** with **X**, **Y** or
**Z** turns it a quarter turn. Right-click a component for **Fix** (or **Float**), **Edit Part** and
**Delete**.

## Mates

Mates hold components against each other. They work between flat and round faces.

1. Click a face on one component, then Ctrl+click a face on another.
2. Click a mate on the ribbon. Only the mates that fit the two faces are available:
   **Coincident**, **Concentric**, **Distance**, **Angle**, **Parallel**, **Perpendicular**, **Tangent** and
   **Lock**.

A new mate keeps the distance or angle the faces have now. Select a mate in the tree to set its
**Distance** or **Angle**, tick **Flip alignment** if a part lands the wrong way round, or click **Delete
Mate**. Right-click a mate for **Flip** and **Delete**.

A solver moves the components that are not fixed until their mates hold. If a mate contradicts the ones
before it, it turns red in the tree and nothing moves for it; hover over it to see why. **Fix** keeps a
component where it is; the others move to it.

## Moving components

- Drag a component to move it, and Shift+drag to turn it. It moves only as far as its mates let it.
- Type a **Position** in the Properties panel, for a component that is not fixed.
- Press `Delete` to remove the selected component, mate or explode step.

## Editing a part

Select a component and click **Edit Part**, or double-click it in the tree. The part opens. Change it,
then click **◀ Back to** *assembly name* at the top: the part is saved and the assembly shows the change.

## Interference

Click **Interference** to find components that overlap. Each overlapping pair is listed with the shared
volume and shown in red. Parts that only touch, like a pin in a hole of its own size, do not count. Click
**Check Again** after a change.

## Exploded views

1. Select one or more components, and optionally a flat face to give the direction.
2. Click **Explode Step**. The components move out 30 mm along the face's normal, or up.
3. Select the step in the tree to set its **Distance** and **Direction**, or click **Delete Step**.

**Explode** plays the steps as an animation, and **Collapse** puts the parts back. A slider lets you scrub
through the steps.

## Bill of materials

Click **BOM** for the bill of materials. Tick **Indented** to list sub-assemblies with their parts.
The columns come from each part's **File > Properties…** (part number, description and your own
properties) and its material. **Export CSV…** saves it for a spreadsheet.

## Export

- **STEP**: each part once, and every component as a named, placed instance.
- **3MF**: one object per part, each on its print face and laid out on the bed, ready for the slicer.
- **Send to Slicer**: the same layout, opened in the slicer chosen on the 3D Print tab of a part.
