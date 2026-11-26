#!/usr/bin/env bash
# Build a release of dbpadu and package it in dist/.
#
#   ./build.sh            build for this computer (macOS or Linux)
#   ./build.sh --windows  build a Windows .exe with the GNU toolchain
#                         (MinGW-w64), from macOS or Linux
#
# Everything that is built ends up in dist/, and stays together:
#
#   dbpadu (dbpadu.exe)   the program, which only opens the window
#   dbpadu_ui, dbpadu_engine, dbpadu_core
#                         the app's own libraries (.so, .dll or .dylib)
#   std-...               Rust's standard library, shared by the three
#   dbpadu-engine.jar     the Java query engine, one file
#
# dist/dbpadu can be run straight away. The same files are packed in a
# .tar.gz (Linux) or .zip (Windows, macOS) next to them.
#
# Needs Rust (https://rustup.rs) and a JDK, version 17 or newer
# (https://adoptium.net). Maven is downloaded by engine/mvnw.
#
# On Windows itself, use build.ps1 instead.

set -euo pipefail
cd "$(dirname "$0")"

APP=dbpadu
JAR=dbpadu-engine.jar
WINDOWS_TARGET=x86_64-pc-windows-gnu

usage() {
    sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
}

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

version() {
    sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1
}

# zip -r <archive> <entries...>, run inside the folder that holds the entries.
make_zip() {
    local archive=$1
    shift
    if command -v zip >/dev/null 2>&1; then
        zip -qr "$archive" "$@"
    elif command -v python3 >/dev/null 2>&1; then
        python3 -m zipfile -c "$archive" "$@"
    else
        fail "need 'zip' or 'python3' to make the .zip file"
    fi
}

need_cargo() {
    command -v cargo >/dev/null 2>&1 ||
        fail "Rust is not installed. Install it from https://rustup.rs and open a new terminal."
}

# The engine is the same file on every system. Its tests run here too.
build_engine() {
    command -v java >/dev/null 2>&1 || [ -n "${JAVA_HOME:-}" ] ||
        fail "Java is not installed. Install a JDK, version 17 or newer (https://adoptium.net)."
    echo "Building the engine ..."
    (cd engine && ./mvnw -q -B package)
    [ -f "engine/target/$JAR" ] || fail "the engine was not built: engine/target/$JAR is missing"
}

# Rust's standard library as a shared library, for the target named (none:
# this computer). The app's libraries share this one copy.
std_library() {
    local dir file
    dir=$(rustc --print target-libdir ${1:+--target "$1"})
    for file in "$dir"/libstd-*.so "$dir"/libstd-*.dylib "$dir"/std-*.dll; do
        if [ -f "$file" ]; then
            printf '%s\n' "$file"
            return
        fi
    done
    fail "Rust's shared standard library was not found in $dir"
}

# Copy what was staged for the archive into dist/ itself, replacing what an
# earlier build left there.
unpack_stage() {
    local stage=$1 entry
    for entry in "$stage"/*; do
        [ "$(basename "$entry")" = README.md ] && continue
        rm -rf "dist/$(basename "$entry")"
        cp -R "$entry" dist/
    done
    rm -rf "$stage"
}

build_windows() {
    need_cargo
    command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || fail "the MinGW-w64 compiler is missing.
  Debian/Ubuntu: sudo apt install gcc-mingw-w64-x86-64
  Fedora:        sudo dnf install mingw64-gcc
  macOS:         brew install mingw-w64"
    # windres puts the icon into the .exe. It comes with the same packages.
    command -v x86_64-w64-mingw32-windres >/dev/null 2>&1 ||
        fail "x86_64-w64-mingw32-windres is missing. It is part of the MinGW-w64 packages above."

    if command -v rustup >/dev/null 2>&1; then
        rustup target list --installed | grep -qx "$WINDOWS_TARGET" ||
            rustup target add "$WINDOWS_TARGET"
    fi

    build_engine
    cargo build --release --locked --target "$WINDOWS_TARGET"

    local name="$APP-$(version)-windows-x86_64"
    local stage="dist/$name"
    rm -rf "$stage" "dist/$name.zip"
    mkdir -p "$stage"
    cp "target/$WINDOWS_TARGET/release/$APP.exe" target/$WINDOWS_TARGET/release/${APP}_*.dll \
        "$(std_library "$WINDOWS_TARGET")" "engine/target/$JAR" README.md "$stage/"
    # The shared standard library needs these two from MinGW-w64.
    local dll path
    for dll in libgcc_s_seh-1.dll libwinpthread-1.dll; do
        path=$(x86_64-w64-mingw32-gcc -print-file-name="$dll")
        [ -f "$path" ] || fail "$dll was not found in the MinGW-w64 installation"
        cp "$path" "$stage/"
    done
    (cd dist && make_zip "$name.zip" "$name")
    unpack_stage "$stage"
    echo "Built dist/$name.zip; the same files are in dist/"
}

build_macos() {
    need_cargo
    build_engine
    cargo build --release --locked

    local arch name stage bundle
    arch=$(uname -m)
    name="$APP-$(version)-macos-$arch"
    stage="dist/$name"
    bundle="$stage/$APP.app"
    rm -rf "$stage" "dist/$name.zip"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    # The libraries sit next to the program, where it looks for them.
    cp "target/release/$APP" target/release/lib${APP}_*.dylib "$(std_library)" \
        "$bundle/Contents/MacOS/"
    # The program looks for the engine in Contents/Resources.
    cp "engine/target/$JAR" "$bundle/Contents/Resources/$JAR"
    cp "assets/icon/$APP.icns" "$bundle/Contents/Resources/$APP.icns"
    cat >"$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$APP</string>
    <key>CFBundleDisplayName</key><string>$APP</string>
    <key>CFBundleIdentifier</key><string>io.github.profawie.$APP</string>
    <key>CFBundleExecutable</key><string>$APP</string>
    <key>CFBundleIconFile</key><string>$APP</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$(version)</string>
    <key>CFBundleShortVersionString</key><string>$(version)</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    cp README.md "$stage/"
    # ditto keeps the bundle's permissions and extended attributes intact.
    (cd dist && ditto -c -k --keepParent "$name" "$name.zip")
    unpack_stage "$stage"
    echo "Built dist/$name.zip; the app is dist/$APP.app"
    echo "The app is not signed. On first launch, right-click it and choose Open."
}

build_linux() {
    need_cargo
    build_engine
    cargo build --release --locked

    local name stage
    name="$APP-$(version)-linux-$(uname -m)"
    stage="dist/$name"
    rm -rf "$stage" "dist/$name.tar.gz"
    mkdir -p "$stage"
    cp "target/release/$APP" target/release/lib${APP}_*.so "$(std_library)" \
        "engine/target/$JAR" README.md "$stage/"
    tar -C dist -czf "dist/$name.tar.gz" "$name"
    unpack_stage "$stage"
    echo "Built dist/$name.tar.gz; the same files are in dist/, run dist/$APP"
}

case "${1:-}" in
"")
    case "$(uname -s)" in
    Darwin) build_macos ;;
    Linux) build_linux ;;
    *) fail "unsupported system $(uname -s); on Windows, run build.ps1" ;;
    esac
    ;;
--windows) build_windows ;;
-h | --help) usage ;;
*)
    usage >&2
    exit 2
    ;;
esac
