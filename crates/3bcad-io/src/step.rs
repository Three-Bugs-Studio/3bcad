use std::io;
use std::path::Path;

use threebcad_kernel::Solid;
use truck_stepio::out::{CompleteStepDisplay, StepHeaderDescriptor, StepModels};

/// Exact B-Rep export (truck writes the AP203/AP214-compatible `ISO-10303-042` schema).
// shortcut: export only; STEP import needs a truck_stepio::r#in -> Solid conversion layer.
pub fn write_step<'a>(path: &Path, solids: impl IntoIterator<Item = &'a Solid>) -> io::Result<()> {
    let compressed: Vec<_> = solids.into_iter().map(Solid::compress).collect();
    let models: StepModels<'_, _, _, _> = compressed.iter().collect();
    let header = StepHeaderDescriptor {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        organization_system: "3BCAD".into(),
        ..Default::default()
    };
    std::fs::write(path, CompleteStepDisplay::new(models, header).to_string())
}
