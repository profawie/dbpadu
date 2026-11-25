//! For Windows, puts the icon and version details into the `.exe`.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/dbpadu.ico");
    // The build script runs on the building computer; ask what it builds for.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Icon and file details (name, version, description) of the Windows
    // program, as shown by Explorer and the taskbar. Needs `windres`, which
    // comes with the MinGW-w64 compiler the Windows build already uses.
    let mut resources = winresource::WindowsResource::new();
    resources
        .set_icon("assets/icon/dbpadu.ico")
        .set("ProductName", "dbpadu")
        .set("FileDescription", "dbpadu");
    resources
        .compile()
        .expect("windres (part of MinGW-w64) embeds the Windows icon");
}
