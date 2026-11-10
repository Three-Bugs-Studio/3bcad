# Installing 3BCAD

Download 3BCAD from the
[latest release](https://github.com/Three-Bugs-Studio/3bcad/releases/latest) on GitHub. Each release
has a plain archive for every system and, from version 1.0, an installer as well. Pick one file for your
computer:

| System | Installer | Archive |
| --- | --- | --- |
| Windows 10 or 11, x64 | `3BCAD-<version>-windows-x64.msi` | `3BCAD-<version>-windows-x64.zip` |
| macOS 11 or later, Apple silicon or Intel | `3BCAD-<version>-macos-universal.dmg` | `3BCAD-<version>-macos-universal.zip` |
| Linux, x64 | `3BCAD-<version>-linux-x86_64.AppImage` | `3BCAD-<version>-linux-x64.tar.gz` |

You need a graphics card with Vulkan, DirectX 12 or Metal. To check a download, compare its SHA-256 with
the matching line of the release's `SHA256SUMS.txt`, using `sha256sum` (Linux), `shasum -a 256` (macOS)
or `Get-FileHash` (Windows PowerShell).

## Windows

- **Installer (`.msi`)**: double-click it. It installs 3BCAD for your user only, into
  `%LOCALAPPDATA%\Programs\3BCAD`, so it does not ask for administrator rights. Afterwards
  double-clicking a `.3bcad`, `.3bcasm` or `.3bcdrw` file opens it in 3BCAD, and 3BCAD is offered under
  **Open with** for STEP, STL, 3MF, OBJ and DXF files.
- **Archive (`.zip`)**: unzip it anywhere and run `3bcad.exe`.

## macOS

- **Disk image (`.dmg`)**: open it and drag `3BCAD.app` onto the **Applications** shortcut next to it.
- **Archive (`.zip`)**: unzip it and move `3BCAD.app` where you like.

Double-clicking a 3BCAD file in Finder does not open it on macOS. Drag the file onto the 3BCAD window, or
use **File > Open…**.

## Linux

- **AppImage**: make it executable (`chmod +x 3BCAD-<version>-linux-x86_64.AppImage`) and run it. It needs
  no FUSE library. To get a menu entry and file associations, integrate it with a tool such as
  AppImageLauncher or Gear Lever.
- **Archive (`.tar.gz`)**: extract it and run `./3bcad`.

## Unsigned builds

Until the releases are code-signed, your system warns you the first time you start 3BCAD:

- **Windows**: SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**.
  Even signed builds can show this warning for a while, until the certificate has built up a reputation.
- **macOS**: Gatekeeper refuses to open the app. Right-click `3BCAD.app`, choose **Open**, then **Open**
  again in the dialog. You only do this once.

## Updates

When it starts, 3BCAD asks GitHub whether a newer release exists. If one does, an **Update to v…** button
appears at the top of the window. It shows what is new, downloads the release for your computer, checks it
against the release's checksums, installs it in place and restarts. Nothing else is sent.

- To turn the check off, untick **File > Settings… > General > Check for updates at start**.
- To check at any time, use **Help > Check for Updates…**.
- The AppImage cannot update itself, because it runs from a read-only image: download the new AppImage
  instead.
- After an in-app update, the Windows "Installed apps" list still shows the version the `.msi` installed.

## Building from source

You need Rust 1.87 or later from [rustup.rs](https://rustup.rs). On Linux, install the windowing and file
dialog libraries first, for example on Debian or Ubuntu:

```bash
sudo apt-get install libgtk-3-dev libxkbcommon-dev libwayland-dev
```

Then:

```bash
git clone https://github.com/Three-Bugs-Studio/3bcad.git
cd 3bcad
cargo run --release
```
