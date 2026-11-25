//! Finding the two things the engine needs: its jar file and Java.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// File name of the engine.
pub const JAR_NAME: &str = "dbpadu-engine.jar";
/// Set this to use an engine jar from somewhere else.
pub const JAR_ENV: &str = "DBPADU_ENGINE_JAR";

/// The engine jar: the one named in `DBPADU_ENGINE_JAR`, the one next to
/// the program, or, when run from the source folder, the one Maven built.
pub fn jar() -> Option<PathBuf> {
    if let Some(path) = env::var_os(JAR_ENV).map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let mut candidates = Vec::new();
    if let Some(dir) = &exe_dir {
        candidates.push(dir.join(JAR_NAME));
        // macOS: dbpadu.app/Contents/MacOS/dbpadu and Contents/Resources.
        candidates.push(dir.join("../Resources").join(JAR_NAME));
        // `cargo run`: target/debug/dbpadu, with the repository above it.
        candidates.extend(
            dir.ancestors()
                .skip(1)
                .take(4)
                .map(|up| up.join("engine/target").join(JAR_NAME)),
        );
    }
    if let Ok(dir) = env::current_dir() {
        candidates.push(dir.join(JAR_NAME));
        candidates.push(dir.join("engine/target").join(JAR_NAME));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// The `java` program to use: the one from the settings, else the one in
/// `JAVA_HOME`, else whichever `java` is on `PATH`.
pub fn java(setting: &str) -> OsString {
    let setting = setting.trim();
    if !setting.is_empty() {
        return setting.into();
    }
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(home) = env::var_os("JAVA_HOME") {
        let path = Path::new(&home).join("bin").join(name);
        if path.is_file() {
            return path.into();
        }
    }
    "java".into()
}
