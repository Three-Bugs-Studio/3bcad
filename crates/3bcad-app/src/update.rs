//! Updates from GitHub Releases: a check for a newer release (at start, unless turned off, or from
//! the File menu), and installing it in place: the release's archive for this computer is
//! downloaded next to 3BCAD, checked against the release's `SHA256SUMS.txt`, unpacked, and swapped
//! for the running program, which is kept as `.old` until the next start.
//!
//! Downloads go through the `curl`, and unpacking through the `tar`, that come with Windows 10 and
//! later, macOS and Linux, so 3BCAD carries no HTTP or TLS code. Nothing is sent but the request.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

use anyhow::{ensure, Context};

use crate::app::VERSION;

const LATEST: &str = "https://api.github.com/repos/Three-Bugs-Studio/3bcad/releases/latest";
/// Where downloads are unpacked, next to the program (on the same drive, so it can be swapped in).
const STAGING: &str = ".3bcad-update";

/// The newest release, as GitHub lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    /// Its CHANGELOG section.
    pub notes: String,
    pub page: String,
    /// The archive for this computer (name, download URL), if the release has one.
    pub asset: Option<(String, String)>,
    pub sums: Option<String>,
}

/// The end of the release archive's name for this computer, as the Release workflow names them.
fn platform() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("-windows-x64.zip")
    } else if cfg!(target_os = "macos") {
        Some("-macos-universal.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("-linux-x64.tar.gz")
    } else {
        None
    }
}

/// Reads GitHub's release JSON, picking the archive whose name ends with `suffix`.
fn parse_release(json: &str, suffix: Option<&str>) -> anyhow::Result<Release> {
    let v: serde_json::Value = serde_json::from_str(json).context("GitHub sent something that is not JSON")?;
    let tag = v["tag_name"].as_str().context("the release has no version tag")?;
    let assets = v["assets"].as_array().map(Vec::as_slice).unwrap_or_default();
    let named = |pick: &dyn Fn(&str) -> bool| {
        assets.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            let url = a["browser_download_url"].as_str()?;
            pick(name).then(|| (name.to_string(), url.to_string()))
        })
    };
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        notes: unwrap_lines(&v["body"].as_str().unwrap_or_default().replace("\r\n", "\n")),
        page: v["html_url"].as_str().unwrap_or_default().to_string(),
        asset: suffix.and_then(|s| named(&|n: &str| n.ends_with(s))),
        sums: named(&|n: &str| n == "SHA256SUMS.txt").map(|(_, url)| url),
    })
}

/// Joins the CHANGELOG's hard-wrapped lines back into one per list item, so the window wraps them.
fn unwrap_lines(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim_start();
        let continues = line.starts_with(' ') && !t.is_empty() && !t.starts_with("- ") && !t.starts_with('#');
        if continues && !out.is_empty() && !out.ends_with('\n') {
            out.push(' ');
            out += t;
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out += line;
        }
    }
    out
}

/// Whether version `latest` is newer than `current` (semantic versions, with or without a `v`): a
/// higher number, or the release of a pre-release.
pub fn newer(latest: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<([u64; 3], bool)> {
        let v = v.trim().trim_start_matches('v');
        let (core, pre) = v.split_once('-').map_or((v, false), |(c, _)| (c, true));
        let n: Vec<u64> = core.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
        let [a, b, c] = n[..] else { return None };
        Some(([a, b, c], pre))
    }
    match (parse(latest), parse(current)) {
        (Some((l, lpre)), Some((c, cpre))) => l > c || (l == c && cpre && !lpre),
        _ => false,
    }
}

/// Runs a command-line tool without a console window flashing up behind 3BCAD on Windows.
fn tool(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Fetches `url` (to `out`, or returns the body).
fn fetch(url: &str, out: Option<&Path>) -> anyhow::Result<Vec<u8>> {
    let mut cmd = tool("curl");
    cmd.args(["-fsSL", "--max-time", "600", "-A", &format!("3BCAD/{VERSION}")])
        .args(["-H", "Accept: application/vnd.github+json"]);
    if let Some(out) = out {
        cmd.arg("-o").arg(out);
    }
    let o = cmd.arg(url).output().context("cannot run curl to reach GitHub")?;
    ensure!(
        o.status.success(),
        "could not download {url}: {}",
        String::from_utf8_lossy(&o.stderr).trim()
    );
    Ok(o.stdout)
}

/// Asks GitHub for the newest release.
pub fn check() -> anyhow::Result<Release> {
    let body = fetch(LATEST, None)?;
    parse_release(&String::from_utf8_lossy(&body), platform())
}

/// What an update replaces: the `.app` bundle on macOS, else the program itself.
fn target(exe: &Path) -> PathBuf {
    let bundle = exe.ancestors().nth(3);
    match bundle {
        Some(b) if cfg!(target_os = "macos") && b.extension().is_some_and(|e| e == "app") => b.to_path_buf(),
        _ => exe.to_path_buf(),
    }
}

fn old(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

fn remove(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
    let _ = std::fs::remove_file(path);
}

/// Removes what the last update left behind: the replaced program and the download.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let t = target(&exe);
        remove(&old(&t));
        if let Some(dir) = t.parent() {
            remove(&dir.join(STAGING));
        }
    }
}

/// Puts `new` where `target` is; the old one stays beside it as `.old` (a running program can be
/// renamed, not overwritten, on Windows).
fn swap(target: &Path, new: &Path) -> io::Result<()> {
    let old = old(target);
    remove(&old);
    std::fs::rename(target, &old)?;
    if let Err(e) = std::fs::rename(new, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e);
    }
    Ok(())
}

/// The expected SHA-256 of `name` in a `sha256sum` listing.
fn listed_sum<'a>(sums: &'a str, name: &str) -> Option<&'a str> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.trim().split_once(char::is_whitespace)?;
        (file.trim_start().trim_start_matches('*') == name).then_some(hash)
    })
}

/// The first file or folder called `name` under `dir`.
fn find(dir: &Path, name: &std::ffi::OsStr) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.file_name() == Some(name) {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(found) = find(&p, name) {
                return Some(found);
            }
        }
    }
    None
}

/// Downloads, checks and installs `r` over the running 3BCAD; returns the program to start.
pub fn install(r: &Release) -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    install_over(r, &target(&exe))?;
    Ok(exe)
}

/// Downloads `r`, checks it against the release's checksums and puts it where `target` is.
fn install_over(r: &Release, target: &Path) -> anyhow::Result<()> {
    let (name, url) = r
        .asset
        .clone()
        .context("this release has no download for this computer")?;
    let sums_url = r
        .sums
        .as_deref()
        .context("this release has no SHA256SUMS.txt to check the download")?;
    let dir = target.parent().context("3BCAD's folder is unknown")?.join(STAGING);
    remove(&dir);
    std::fs::create_dir_all(&dir).with_context(|| {
        format!(
            "cannot write to {}: update from the release page instead",
            dir.display()
        )
    })?;
    let archive = dir.join(&name);
    fetch(&url, Some(&archive))?;
    let sums = String::from_utf8_lossy(&fetch(sums_url, None)?).into_owned();
    let want = listed_sum(&sums, &name).context("SHA256SUMS.txt does not list the download")?;
    let got = hex(&sha256(&std::fs::read(&archive)?));
    ensure!(
        got.eq_ignore_ascii_case(want),
        "the download is damaged (its SHA-256 does not match)"
    );
    let o = tool("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&dir)
        .output()
        .context("cannot run tar to unpack the update")?;
    ensure!(
        o.status.success(),
        "could not unpack the update: {}",
        String::from_utf8_lossy(&o.stderr).trim()
    );
    let new = find(&dir, target.file_name().context("no program name")?)
        .filter(|p| p != &archive)
        .context("the download does not hold 3BCAD")?;
    swap(target, &new).with_context(|| format!("cannot replace {}", target.display()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend((data.len() as u64 * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (o, x) in out.chunks_mut(4).zip(h) {
        o.copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// Where the update is.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    Available(Release),
    Installing(Release),
    /// Installed; takes effect on the next start.
    Installed(String),
    Failed(Release, String),
}

enum Msg {
    Checked(anyhow::Result<Release>),
    Installed(anyhow::Result<PathBuf>),
}

/// The update check and install running beside the UI.
pub struct Updater {
    pub state: State,
    /// The update window is open.
    pub open: bool,
    /// The check was asked for in the menu: say so when there is nothing new.
    manual: bool,
    rx: Option<Receiver<Msg>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            open: false,
            manual: false,
            rx: None,
        }
    }

    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }

    /// The newer version found, until it is installed.
    pub fn available(&self) -> Option<&str> {
        match &self.state {
            State::Available(r) | State::Failed(r, _) => Some(&r.version),
            _ => None,
        }
    }

    /// Asks GitHub in the background.
    pub fn check(&mut self, manual: bool) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        // Nobody listens once the window closed; nothing to do then.
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Checked(check()));
        });
        (self.rx, self.manual, self.state) = (Some(rx), manual, State::Checking);
    }

    /// Downloads and installs the release found, in the background.
    pub fn install(&mut self) {
        let (State::Available(r) | State::Failed(r, _)) = self.state.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let job = r.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Installed(install(&job)));
        });
        (self.rx, self.state) = (Some(rx), State::Installing(r));
    }

    /// Takes in what the background work found; returns a status line worth showing.
    pub fn poll(&mut self, skipped: &str) -> Option<String> {
        let msg = self.rx.as_ref()?.try_recv().ok()?;
        self.rx = None;
        match (msg, std::mem::replace(&mut self.state, State::Idle)) {
            (Msg::Checked(Ok(r)), _) if newer(&r.version, VERSION) && (self.manual || r.version != skipped) => {
                self.state = State::Available(r);
                self.open = true;
                None
            }
            (Msg::Checked(Ok(_)), _) => self.manual.then(|| format!("3BCAD {VERSION} is the latest version")),
            (Msg::Checked(Err(e)), _) => self.manual.then(|| format!("Could not check for updates: {e:#}")),
            (Msg::Installed(Ok(_)), State::Installing(r)) => {
                self.state = State::Installed(r.version);
                self.open = true;
                None
            }
            (Msg::Installed(Err(e)), State::Installing(r)) => {
                self.state = State::Failed(r, format!("{e:#}"));
                self.open = true;
                None
            }
            (_, state) => {
                self.state = state;
                None
            }
        }
    }
}

/// Whether this build may replace itself: a debug build is a developer's `cargo run`.
pub fn can_install(r: &Release) -> bool {
    r.asset.is_some() && r.sums.is_some() && !cfg!(debug_assertions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_fips_examples() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let two_blocks = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            hex(&sha256(two_blocks)),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn versions_compare_as_semantic_versions() {
        assert!(newer("v0.8.0", "0.7.0"));
        assert!(newer("0.10.0", "0.9.3"), "numbers, not text");
        assert!(newer("1.0.0", "0.99.9"));
        assert!(newer("0.8.0", "0.8.0-beta.2"), "the release beats its pre-release");
        assert!(!newer("0.8.0", "0.8.0"));
        assert!(!newer("0.7.9", "0.8.0"));
        assert!(!newer("0.8.0-beta.1", "0.8.0"));
        assert!(!newer("garbage", "0.8.0") && !newer("0.8", "0.7.0"));
    }

    #[test]
    fn the_release_json_gives_the_download_for_this_computer() {
        let json = r#"{
            "tag_name": "v0.9.0",
            "html_url": "https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v0.9.0",
            "body": "Added:\r\n\r\n- Things",
            "assets": [
                {"name": "3BCAD-0.9.0-linux-x64.tar.gz", "browser_download_url": "https://x/linux"},
                {"name": "3BCAD-0.9.0-windows-x64.zip", "browser_download_url": "https://x/win"},
                {"name": "SHA256SUMS.txt", "browser_download_url": "https://x/sums"}
            ]
        }"#;
        let r = parse_release(json, Some("-windows-x64.zip")).unwrap();
        assert_eq!(r.version, "0.9.0");
        assert_eq!(r.notes, "Added:\n\n- Things", "Windows line ends are dropped");
        assert_eq!(
            r.asset,
            Some(("3BCAD-0.9.0-windows-x64.zip".into(), "https://x/win".into()))
        );
        assert_eq!(r.sums.as_deref(), Some("https://x/sums"));
        // No build for this computer: the release page is all there is.
        assert_eq!(parse_release(json, Some("-macos-universal.zip")).unwrap().asset, None);
        assert!(parse_release("{}", None).is_err() && parse_release("<html>", None).is_err());
    }

    #[test]
    fn checksums_are_read_from_a_sha256sum_listing() {
        let sums = "aaaa  3BCAD-0.9.0-linux-x64.tar.gz\nBBBB *3BCAD-0.9.0-windows-x64.zip\n";
        assert_eq!(listed_sum(sums, "3BCAD-0.9.0-windows-x64.zip"), Some("BBBB"));
        assert_eq!(listed_sum(sums, "3BCAD-0.9.0-linux-x64.tar.gz"), Some("aaaa"));
        assert_eq!(listed_sum(sums, "3BCAD-0.9.0-macos-universal.zip"), None);
    }

    #[test]
    fn an_update_swaps_in_the_new_program_and_keeps_the_old_one_until_cleanup() {
        let dir = std::env::temp_dir().join(format!("3bcad-update-test-{}", std::process::id()));
        remove(&dir);
        let unpacked = dir.join(STAGING).join("3BCAD-0.9.0-windows-x64");
        std::fs::create_dir_all(&unpacked).unwrap();
        let exe = dir.join("3bcad.exe");
        std::fs::write(&exe, "old").unwrap();
        std::fs::write(unpacked.join("3bcad.exe"), "new").unwrap();
        let new = find(&dir.join(STAGING), exe.file_name().unwrap()).unwrap();
        swap(&exe, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(old(&exe)).unwrap(), "old");
        // A failed swap puts the program back.
        assert!(swap(&exe, &dir.join("missing")).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        remove(&dir);
    }

    #[test]
    fn release_notes_lose_the_changelog_line_wraps() {
        let notes = "Added\n\n- Drawings (v0.8.0):\n  - Sheets in A4 to A0,\n    as many as you need (#85).\n  - Views (#86).\n- File > New Drawing.";
        assert_eq!(
            unwrap_lines(notes),
            "Added\n\n- Drawings (v0.8.0):\n  - Sheets in A4 to A0, as many as you need (#85).\n  - Views (#86).\n- File > New Drawing."
        );
    }

    /// The real thing: GitHub's latest release, checked and installed over a stand-in program.
    #[test]
    #[ignore = "downloads the latest release from GitHub"]
    fn the_latest_release_installs_over_a_stand_in() {
        let r = check().unwrap();
        assert!(r.asset.is_some() && r.sums.is_some(), "{r:?}");
        let dir = std::env::temp_dir().join(format!("3bcad-update-live-{}", std::process::id()));
        remove(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(target_os = "macos") {
            "3BCAD.app"
        } else if cfg!(windows) {
            "3bcad.exe"
        } else {
            "3bcad"
        };
        let stand_in = dir.join(name);
        std::fs::write(&stand_in, "stand-in").unwrap();
        install_over(&r, &stand_in).unwrap();
        assert_eq!(std::fs::read_to_string(old(&stand_in)).unwrap(), "stand-in");
        let program = if cfg!(target_os = "macos") {
            stand_in.join("Contents/MacOS/3bcad")
        } else {
            stand_in.clone()
        };
        let bytes = std::fs::read(&program).unwrap();
        assert!(bytes.len() > 1_000_000, "{} bytes", bytes.len());
        if cfg!(windows) {
            assert_eq!(&bytes[..2], b"MZ", "a Windows program");
        }
        remove(&dir);
    }

    #[test]
    fn a_found_update_opens_the_window_unless_skipped() {
        let release = |v: &str| Release {
            version: v.into(),
            notes: String::new(),
            page: String::new(),
            asset: None,
            sums: None,
        };
        let feed = |u: &mut Updater, msg: Msg| {
            let (tx, rx) = mpsc::channel();
            tx.send(msg).unwrap();
            u.rx = Some(rx);
        };
        let mut u = Updater::new();
        feed(&mut u, Msg::Checked(Ok(release("99.0.0"))));
        assert_eq!(u.poll(""), None);
        assert!(u.open && u.available() == Some("99.0.0"));
        // Skipped at start, but a check from the menu still shows it.
        let mut u = Updater::new();
        feed(&mut u, Msg::Checked(Ok(release("99.0.0"))));
        assert_eq!((u.poll("99.0.0"), u.open), (None, false));
        u.manual = true;
        feed(&mut u, Msg::Checked(Ok(release("99.0.0"))));
        u.poll("99.0.0");
        assert!(u.open);
        // Up to date: quiet at start, said when asked.
        let mut u = Updater::new();
        feed(&mut u, Msg::Checked(Ok(release(VERSION))));
        assert_eq!(u.poll(""), None);
        u.manual = true;
        feed(&mut u, Msg::Checked(Ok(release(VERSION))));
        assert!(u.poll("").unwrap().contains("latest version"));
        feed(&mut u, Msg::Checked(Err(anyhow::anyhow!("offline"))));
        assert!(u.poll("").unwrap().contains("offline"));
    }
}
