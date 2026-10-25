use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slicer {
    OrcaSlicer,
    BambuStudio,
    PrusaSlicer,
}

impl Slicer {
    pub const ALL: [Slicer; 3] = [Slicer::OrcaSlicer, Slicer::BambuStudio, Slicer::PrusaSlicer];

    pub fn name(self) -> &'static str {
        match self {
            Slicer::OrcaSlicer => "OrcaSlicer",
            Slicer::BambuStudio => "Bambu Studio",
            Slicer::PrusaSlicer => "PrusaSlicer",
        }
    }

    /// Opens `file` in the slicer: default install location first, then `PATH`.
    pub fn launch(self, file: &Path) -> io::Result<Child> {
        if cfg!(target_os = "macos") {
            return Command::new("open").args(["-a", self.name()]).arg(file).spawn();
        }
        let mut last_err = io::Error::new(io::ErrorKind::NotFound, format!("{} not found", self.name()));
        for exe in self.candidates() {
            if exe.is_absolute() && !exe.exists() {
                continue;
            }
            match Command::new(&exe).arg(file).spawn() {
                Ok(child) => return Ok(child),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    fn candidates(self) -> Vec<PathBuf> {
        let (dir, exe) = match self {
            Slicer::OrcaSlicer => ("OrcaSlicer", "orca-slicer"),
            Slicer::BambuStudio => ("Bambu Studio", "bambu-studio"),
            Slicer::PrusaSlicer => ("Prusa3D\\PrusaSlicer", "prusa-slicer"),
        };
        let mut out = Vec::new();
        if cfg!(windows) {
            let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
            out.push(Path::new(&program_files).join(dir).join(format!("{exe}.exe")));
        }
        out.push(PathBuf::from(exe));
        out
    }
}
