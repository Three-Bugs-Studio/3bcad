//! Release hygiene: catches a version bump without release notes in normal CI, before a tag is pushed.

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn repo_file(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

#[test]
fn changelog_has_notes_for_current_version() {
    let changelog = repo_file("CHANGELOG.md");
    let heading = format!("## [{VERSION}]");
    let start = changelog
        .lines()
        .position(|l| l.starts_with(&heading))
        .unwrap_or_else(|| panic!("CHANGELOG.md has no `{heading}` section"));
    let has_notes = changelog
        .lines()
        .skip(start + 1)
        .take_while(|l| !l.starts_with("## ["))
        .any(|l| l.trim_start().starts_with("- "));
    assert!(has_notes, "CHANGELOG.md section {heading} has no entries");
    assert!(
        changelog.contains(&format!(
            "[{VERSION}]: https://github.com/Three-Bugs-Studio/3bcad/releases/tag/v{VERSION}"
        )),
        "CHANGELOG.md is missing the link for {VERSION}"
    );
}

#[test]
fn macos_bundle_template_takes_the_version() {
    let plist = repo_file("packaging/macos/Info.plist");
    assert_eq!(plist.matches("@VERSION@").count(), 2);
    assert!(
        plist.contains("<string>3bcad</string>"),
        "CFBundleExecutable must match the binary name"
    );
}
