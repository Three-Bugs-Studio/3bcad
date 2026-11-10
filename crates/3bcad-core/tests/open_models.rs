//! Opens every model in `MODELS_DIR` the way File > Open does (read, rebuild, mesh for display) and
//! prints one line per file (`MODELS_DIR` may also be one file). For checking downloaded third-party
//! files by hand:
//! `MODELS_DIR=/path/to/models cargo test --release -p threebcad-core --test open_models -- --ignored --nocapture`

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use threebcad_core::{FeatureKind, PartDocument, Shared};
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::{print_mesh, render_mesh};

fn open(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let file = path.file_name().unwrap().to_string_lossy().into_owned();
    let t = Instant::now();
    let mut doc = PartDocument::new("open");
    let mut note = String::new();
    match ext.as_str() {
        "step" | "stp" => {
            let import =
                threebcad_io::read_step(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if !import.skipped.is_empty() {
                note = format!(" skipped={} ({})", import.skipped.len(), import.skipped[0]);
            }
            let read = t.elapsed();
            doc.edit(|h| {
                h.add(FeatureKind::Imported {
                    file,
                    solids: Shared::new(import.solids),
                })
            })
            .map_err(|e| e.to_string())?;
            if !doc.errors().is_empty() {
                return Err(format!("rebuild: {:?}", doc.errors()));
            }
            let built = t.elapsed();
            let tris: usize = doc
                .bodies()
                .iter()
                .map(|b| render_mesh(&b.solid, 0.05).positions.len() / 3)
                .sum();
            let meshed = t.elapsed();
            let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.05)).collect();
            let m = mass_properties(&meshes);
            let (lo, hi) = bounds(meshes.iter().flat_map(|m| m.vertices.iter().copied()));
            Ok(format!(
                "bodies={} faces={} tris={tris} volume={:.1} size={:.1}x{:.1}x{:.1} read={:.1}s rebuild={:.1}s mesh={:.1}s{note}",
                doc.bodies().len(),
                doc.bodies().iter().map(|b| b.topo.faces.len()).sum::<usize>(),
                m.volume,
                hi[0] - lo[0],
                hi[1] - lo[1],
                hi[2] - lo[2],
                read.as_secs_f64(),
                (built - read).as_secs_f64(),
                (meshed - built).as_secs_f64(),
            ))
        }
        "stl" | "3mf" | "obj" => {
            let mesh = threebcad_io::read_mesh(path).map_err(|e| e.to_string())?;
            let read = t.elapsed();
            let (lo, hi) = bounds(mesh.vertices.iter().copied());
            let tris = mesh.triangles.len();
            let volume = mass_properties(std::slice::from_ref(&mesh)).volume;
            doc.edit(|h| {
                h.add(FeatureKind::ImportedMesh {
                    file,
                    mesh: Shared::new(mesh),
                })
            })
            .map_err(|e| e.to_string())?;
            Ok(format!(
                "tris={tris} volume={volume:.1} size={:.1}x{:.1}x{:.1} read={:.1}s rebuild={:.1}s",
                hi[0] - lo[0],
                hi[1] - lo[1],
                hi[2] - lo[2],
                read.as_secs_f64(),
                (t.elapsed() - read).as_secs_f64()
            ))
        }
        "dxf" => {
            let sketch = threebcad_io::read_dxf(&std::fs::read(path).map_err(|e| e.to_string())?, None)
                .map_err(|e| e.to_string())?;
            Ok(format!("entities={}", sketch.entity_ids().count()))
        }
        _ => Err("not a model".into()),
    }
}

fn bounds(points: impl Iterator<Item = [f64; 3]>) -> ([f64; 3], [f64; 3]) {
    points.fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), p| {
        (
            std::array::from_fn(|i| lo[i].min(p[i])),
            std::array::from_fn(|i| hi[i].max(p[i])),
        )
    })
}

#[test]
#[ignore = "needs MODELS_DIR with downloaded models"]
fn open_downloaded_models() {
    let Some(dir) = std::env::var_os("MODELS_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut files: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries.filter_map(|e| Some(e.ok()?.path())).collect(),
        Err(_) => vec![dir],
    };
    files.sort();
    let mut hung = false;
    for path in files {
        let (tx, rx) = mpsc::channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(|| open(&p)).unwrap_or_else(|_| Err("PANIC".into()));
            let _ = tx.send(r);
        });
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        match rx.recv_timeout(Duration::from_secs(180)) {
            Ok(Ok(line)) => println!("OK   {name}: {line}"),
            Ok(Err(e)) if e == "not a model" => {}
            Ok(Err(e)) => println!("FAIL {name}: {e}"),
            Err(_) => {
                println!("HANG {name}: over 180 s");
                hung = true;
            }
        }
    }
    if hung {
        std::process::exit(0);
    }
}
