//! Embeds the app icon and version details in the Windows program: Explorer, the taskbar, the
//! installer's shortcut and "Open with" take the icon and the name "3BCAD" from here.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/icon.ico")
            .set("ProductName", "3BCAD")
            .set("FileDescription", "3BCAD")
            .compile()
            .expect("cannot embed the icon in 3bcad.exe");
    }
}
