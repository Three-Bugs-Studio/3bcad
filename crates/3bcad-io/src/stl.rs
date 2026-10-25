use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use threebcad_kernel::mesher::PrintMesh;

/// Binary STL, all bodies in one file.
pub fn write_stl(path: &Path, meshes: &[PrintMesh]) -> io::Result<()> {
    let count: usize = meshes.iter().map(|m| m.triangles.len()).sum();
    let count = u32::try_from(count).map_err(|_| io::Error::other("too many triangles for STL"))?;
    let mut w = BufWriter::new(File::create(path)?);
    let mut header = [0u8; 80];
    header[..21].copy_from_slice(b"3BCAD binary STL (mm)");
    w.write_all(&header)?;
    w.write_all(&count.to_le_bytes())?;
    for m in meshes {
        for t in &m.triangles {
            let [a, b, c] = t.map(|i| m.vertices[i]);
            let (u, v) = (sub(b, a), sub(c, a));
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(f64::MIN_POSITIVE);
            for p in [n.map(|c| c / len), a, b, c] {
                for c in p {
                    w.write_all(&(c as f32).to_le_bytes())?;
                }
            }
            w.write_all(&[0, 0])?;
        }
    }
    w.flush()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
