//! The log file and crash reports. Release builds have no console on Windows, so everything goes
//! to `logs/3bcad.log` in the user's config folder (Help > Open Log Folder), started afresh each
//! run with the last three runs kept. A panic writes `logs/crash.txt`; when 3BCAD did not get to
//! close normally, the next start keeps it as `crash-<time>.txt` and says so.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::VERSION;

const KEEP: usize = 3;
/// A runaway loop must not fill the disk.
const MAX_BYTES: u64 = 10 << 20;

pub fn log_dir() -> Option<PathBuf> {
    crate::files::config_dir().map(|d| d.join("logs"))
}

/// `3bcad.log` becomes `3bcad.1.log`, and so on; the oldest goes.
fn rotate(dir: &Path) {
    let name = |i: usize| match i {
        0 => dir.join("3bcad.log"),
        i => dir.join(format!("3bcad.{i}.log")),
    };
    let _ = fs::remove_file(name(KEEP));
    for i in (0..KEEP).rev() {
        let _ = fs::rename(name(i), name(i + 1));
    }
}

/// UTC "2026-11-10 13:05:09" (no time zone database needed).
fn timestamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's days-to-civil.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

struct FileLog {
    file: Mutex<Option<(File, u64)>>,
    level: log::LevelFilter,
}

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        // Our own crates in detail; the libraries' chatter only when it is a warning or worse.
        m.level() <= self.level
            && (m.target().starts_with("threebcad") || m.target().starts_with("3bcad") || m.level() <= log::Level::Warn)
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            timestamp(SystemTime::now()),
            r.level(),
            r.target(),
            r.args()
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut f) = self.file.lock() {
            if let Some((file, written)) = f.as_mut() {
                if *written < MAX_BYTES {
                    *written += line.len() as u64;
                    let _ = file.write_all(line.as_bytes());
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            if let Some((file, _)) = f.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// The text of a crash report.
fn crash_report(message: &str, location: Option<String>, backtrace: &str) -> String {
    format!(
        "3BCAD {VERSION} crashed at {} UTC\nOS: {} {}\n\n{message}\n{}\n\nBacktrace:\n{backtrace}\n\
         Please attach this file (and 3bcad.log next to it) to a bug report:\n\
         https://github.com/Three-Bugs-Studio/3bcad/issues/new?template=bug_report.yml\n",
        timestamp(SystemTime::now()),
        std::env::consts::OS,
        std::env::consts::ARCH,
        location.map_or_else(String::new, |l| format!("at {l}")),
    )
}

/// Starts the log, installs the crash reporter, and returns the crash report the last run left
/// when it did not close normally.
pub fn init() -> Option<PathBuf> {
    let dir = log_dir();
    let mut file = None;
    let mut crashed = None;
    if let Some(dir) = &dir {
        let _ = fs::create_dir_all(dir);
        let (running, crash) = (dir.join("running"), dir.join("crash.txt"));
        // The marker outlived its run: a crash report written since belongs to it.
        if let (Ok(marker), Ok(report)) = (fs::metadata(&running), fs::metadata(&crash)) {
            if report.modified().ok() >= marker.modified().ok() {
                let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let kept = dir.join(format!("crash-{secs}.txt"));
                if fs::rename(&crash, &kept).is_ok() {
                    crashed = Some(kept);
                }
            }
        }
        let _ = fs::write(&running, VERSION);
        rotate(dir);
        file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("3bcad.log"))
            .ok()
            .map(|f| (f, 0));
    }
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let logger = Box::new(FileLog {
        file: Mutex::new(file),
        level,
    });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(level);
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        // A panic the program catches (truck on a bad STEP file) also lands here: it is logged, and
        // only reported if 3BCAD then fails to close normally.
        log::error!("panic: {message} {}", location.clone().unwrap_or_default());
        if let Some(dir) = log_dir() {
            let _ = fs::write(dir.join("crash.txt"), crash_report(&message, location, &backtrace));
        }
        log::logger().flush();
        default(info);
    }));
    log::info!("3BCAD {VERSION} on {} {}", std::env::consts::OS, std::env::consts::ARCH);
    crashed
}

/// A normal exit: no crash to report next time.
pub fn closed_normally() {
    if let Some(dir) = log_dir() {
        let _ = fs::remove_file(dir.join("running"));
        let _ = fs::remove_file(dir.join("crash.txt"));
    }
    log::info!("closed normally");
    log::logger().flush();
}

/// Help > Open Log Folder.
pub fn open_folder() -> std::io::Result<()> {
    let dir = log_dir().ok_or_else(|| std::io::Error::other("no user folder"))?;
    fs::create_dir_all(&dir)?;
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(&dir).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_rotate_keeping_three_runs() {
        let dir = std::env::temp_dir().join(format!("3bcad-logs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for run in 0..5 {
            rotate(&dir);
            fs::write(dir.join("3bcad.log"), format!("run {run}")).unwrap();
        }
        let read = |n: &str| fs::read_to_string(dir.join(n)).unwrap();
        assert_eq!(read("3bcad.log"), "run 4");
        assert_eq!(read("3bcad.1.log"), "run 3");
        assert_eq!(read("3bcad.3.log"), "run 1");
        assert!(!dir.join("3bcad.4.log").exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn timestamps_and_crash_reports_read_well() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01 00:00:00");
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_794_315_909);
        assert_eq!(timestamp(t), "2026-11-10 13:05:09");
        let r = crash_report("index out of bounds", Some("src/app.rs:10".into()), "frames");
        assert!(r.contains(VERSION) && r.contains("index out of bounds") && r.contains("at src/app.rs:10"));
        assert!(r.contains("Backtrace:\nframes") && r.contains("issues/new"));
    }
}
