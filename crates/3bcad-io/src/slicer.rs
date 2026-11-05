use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    pub fn from_name(name: &str) -> Option<Slicer> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Opens `file` in the slicer: at `exe` when the user located it, else from the default install
    /// location, then `PATH`. A slicer that is not there is an `ErrorKind::NotFound` error.
    pub fn launch(self, file: &Path, exe: Option<&Path>) -> io::Result<()> {
        let missing = |what: String| io::Error::new(io::ErrorKind::NotFound, what);
        if let Some(exe) = exe {
            if !exe.exists() {
                return Err(missing(format!("{} is not at {}", self.name(), exe.display())));
            }
            if cfg!(target_os = "macos") && exe.extension().is_some_and(|e| e == "app") {
                return open_app(exe.as_os_str(), file).map_err(|_| missing(format!("{} did not open", self.name())));
            }
            return Command::new(exe).arg(file).spawn().map(drop);
        }
        if cfg!(target_os = "macos") {
            return open_app(std::ffi::OsStr::new(self.name()), file)
                .map_err(|_| missing(format!("{} is not installed", self.name())));
        }
        for exe in self.candidates() {
            if exe.is_absolute() && !exe.exists() {
                continue;
            }
            match Command::new(&exe).arg(file).spawn() {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Err(missing(format!("{} was not found", self.name())))
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

/// macOS `open -a`: it starts even when the app is missing, so its exit status is the answer.
fn open_app(app: &std::ffi::OsStr, file: &Path) -> io::Result<()> {
    let status = Command::new("open").arg("-a").arg(app).arg(file).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("open failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_a_missing_override_is_not_found() {
        for s in Slicer::ALL {
            assert_eq!(Slicer::from_name(s.name()), Some(s));
        }
        assert_eq!(Slicer::from_name("Cura"), None);
        let nowhere = std::env::temp_dir().join("3bcad-no-such-slicer.exe");
        let err = Slicer::OrcaSlicer
            .launch(Path::new("part.3mf"), Some(&nowhere))
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
