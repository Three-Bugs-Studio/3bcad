# Open questions

Decisions the maintainer (@DongDuong2001) needs to make. Answer inline or in PR
[#120](https://github.com/Three-Bugs-Studio/3bcad/pull/120); resolved questions move to the bottom.

## For v0.2.0

1. **Plane orientation.** 3BCAD is Z-up (good for 3D printing): Top = XY, Front = XZ, Right = YZ. SolidWorks
   is Y-up, where Front = XY. Keep Z-up naming, or match SolidWorks exactly?
2. **Pattern instance count (#15).** To make the count editable after Apply, the pattern has to become a
   regenerating "sketch pattern" object instead of plain copies. Do it in v0.2.x, or move #15 to v0.3.0?
3. **Release timing.** Tag v0.2.0 as soon as PR #120 is merged and manually tested, or wait to bundle it with
   early v0.3.0 work (Extruded Cut needs booleans)?
4. **Manual testing on macOS / Linux.** Who can try the sketcher there before tagging?

## Project / community

5. **Code of Conduct contact.** `CODE_OF_CONDUCT.md` points to GitHub private reports or "the contact address
   on the organization profile". Add a dedicated email (e.g. a `conduct@` address)?
6. **Second maintainer.** With one code owner, every PR you open is merged by admin bypass. Add another
   maintainer/reviewer to CODEOWNERS so reviews are real?
7. **Sketch on face follows the face.** Persistent face naming (#42) is in v0.3.0. Is a "static plane" sketch
   on a face acceptable until then?

## Resolved

- Repository visibility: public (2026-10-26), to allow the "Protect main" ruleset.
- Licence: MIT OR Apache-2.0.
