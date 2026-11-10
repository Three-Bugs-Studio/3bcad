# View controls, display styles and settings

## Moving around

| Do this | To |
| --- | --- |
| Left drag (or middle drag) on empty space | Orbit |
| Right drag (or Ctrl + middle drag) | Pan |
| Mouse wheel | Zoom |
| `F`, or double-click empty space | Zoom to fit |
| Arrow keys | Turn the view 15° (with Shift: 90°) |
| Ctrl + arrow keys | Pan |
| Page Up / Page Down | Zoom |

The arrow keys work when no text field or button has the keyboard focus (press Esc first).

## The View toolbar

The bar at the top of the viewport has:

- **Views**: Front, Back, Left, Right, Top, Bottom, Isometric and **Normal To** (look straight at the
  selected planar face or plane). The view turns there smoothly.
- **Fit**: zoom to fit.
- The **display style** (see below).
- **Section**: cut the model open (see below).
- **Perspective**: on by default. Turn it off for an orthographic view, where parallel lines stay
  parallel and sizes do not shrink with distance.

The standard views also have keys, as in SolidWorks:

| Keys | View |
| --- | --- |
| Ctrl+1 / Ctrl+2 | Front / Back |
| Ctrl+3 / Ctrl+4 | Left / Right |
| Ctrl+5 / Ctrl+6 | Top / Bottom |
| Ctrl+7 | Isometric |
| Ctrl+8 | Normal To |
| `P` | Perspective on or off |

## The view cube

The cube in the top-right corner of the viewport turns with the model. Click a face to look at that
side, an edge to look between two sides, or a corner for an isometric view from that corner.
**Settings → View** hides it.

## Display styles

| Style | What you see |
| --- | --- |
| Shaded With Edges | Shaded faces with their edges (the default) |
| Shaded | Shaded faces only |
| Hidden Lines Removed | White faces and the edges you can see, like a drawing |
| Hidden Lines Visible | The same, with the edges behind faces in light grey |
| Wireframe | Every edge, no faces |

The last three also draw the outline of curved faces (the sides of a cylinder). Press `D` to go to
the next style. **Settings → View** sets the style 3BCAD starts with.

## Section view

Click **Section** in the View toolbar to cut the model open and look inside. The cut faces show in
orange. In the **Section View** window you choose the plane the cut is parallel to (Front, Top or
Right), move it with the slider, and **Flip** it to keep the other side. You can also drag the orange
arrow in the viewport. The section view only changes what you see: the part, its mass and its exports
stay whole. Clicking picks what lies behind the cut.

## The shortcut bar and mouse gestures

Press `S` over the viewport to get the current tab's commands at the mouse. Click one, or press Esc.

With **Settings → Mouse → Shows the mouse gesture wheel**, dragging with the right mouse button shows
a wheel of four commands: drag up for the Top view, down for Front, left for Isometric and right to zoom
to fit. Shift + right drag still pans.

## Settings

**File → Settings…** (Ctrl+,) holds every preference. They are saved for your user, in
`%APPDATA%\3BCAD\settings.txt` on Windows, `~/Library/Application Support/3BCAD` on macOS and
`~/.config/3bcad` on Linux.

- **General**:
  - the language (English or Tiếng Việt);
  - the theme: same as the system, light, dark or high contrast;
  - the interface size (Ctrl + plus and Ctrl + minus change it too, and Ctrl+0 resets it);
  - the units of new parts;
  - autosave;
  - checking for updates.
- **View**: the display style, perspective and view cube 3BCAD starts with.
- **Mouse**: what the right mouse drag does.
- **Shortcuts**: click a command's shortcut, then press the keys you want. If they are already
  another command's, 3BCAD says which and leaves both as they were. **Clear** removes a shortcut and
  **Default** puts the original one back.
- **3D printing**: the slicer Send to Slicer opens, and where 3BCAD found each one.
- **Performance**:
  - draw large models simplified while the view moves;
  - show the frame rate.

**Reset to defaults** puts every setting back as 3BCAD comes, but keeps your recent files and where
your slicers are. Press it twice to confirm.

## Large models

3BCAD meshes parts for display in the background. While it works, the status bar shows
"Meshing 3 / 12 bodies…" and you can keep orbiting the previous model. Models with more than
300 000 triangles (scans, detailed STL downloads) are drawn simplified while the view moves and in
full as soon as it stops; **Settings → Performance** turns that off.

## Accessibility

- **Interface size** (Settings → General) scales every panel, menu and text from 50 % to 300 %.
- The **High contrast** theme draws white on black with yellow focus outlines.
- Tab moves the keyboard focus through the panels' buttons and fields; Enter or Space presses the
  focused button. The arrow keys turn the view.
- 3BCAD exposes its interface to screen readers through the operating system's accessibility API
  (AccessKit).
