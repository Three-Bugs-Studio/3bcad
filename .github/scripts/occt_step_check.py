"""Reads 3BCAD's STEP exports with OpenCASCADE and checks each solid's volume, units and name, and
for an assembly (a line with a bounding box after the volume) that its components are placed.

Usage: python occt_step_check.py <folder with *.step and volumes.txt>
"""
import pathlib
import sys

from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader

folder = pathlib.Path(sys.argv[1])
failed = False
for line in (folder / "volumes.txt").read_text().splitlines():
    name, expected, *bounds = line.split()
    expected = float(expected)
    path = folder / f"{name}.step"
    reader = STEPControl_Reader()
    if reader.ReadFile(str(path)) != IFSelect_RetDone:
        print(f"FAIL {name}: OpenCASCADE could not read the file")
        failed = True
        continue
    reader.TransferRoots()
    props = GProp_GProps()
    BRepGProp.VolumeProperties_s(reader.OneShape(), props)
    volume = props.Mass()
    # Our volume comes from a fine mesh, OCCT's is exact: they agree to a fraction of a percent.
    ok = abs(volume - expected) < 2e-3 * expected and f"PRODUCT('{name}'" in path.read_text()
    print(f"{'ok  ' if ok else 'FAIL'} {name}: OCCT volume {volume:.3f}, 3BCAD {expected:.3f}")
    if bounds:
        box = Bnd_Box()
        BRepBndLib.AddOptimal_s(reader.OneShape(), box, False, False)
        lo, hi = box.CornerMin(), box.CornerMax()
        found = (lo.X(), lo.Y(), lo.Z(), hi.X(), hi.Y(), hi.Z())
        placed = all(abs(a - float(b)) < 0.05 for a, b in zip(found, bounds))
        print(f"{'ok  ' if placed else 'FAIL'} {name}: OCCT box {[round(v, 3) for v in found]}, 3BCAD {bounds}")
        ok &= placed
    failed |= not ok
sys.exit(1 if failed else 0)
