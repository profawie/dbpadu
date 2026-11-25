#!/usr/bin/env bash
# Build and run dbpadu on macOS or Linux.
#
#   ./run.sh            debug build (compiles faster)
#   ./run.sh --release  optimized build
#
# The Java engine is built first when it is missing or its code changed.
# On Windows, use run.ps1 instead.

set -euo pipefail
cd "$(dirname "$0")"

JAR=engine/target/dbpadu-engine.jar

usage() {
    sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
}

build_engine() {
    if [ -f "$JAR" ] && [ -z "$(find engine/pom.xml engine/src -newer "$JAR" -print -quit)" ]; then
        return
    fi
    command -v java >/dev/null 2>&1 || [ -n "${JAVA_HOME:-}" ] || {
        echo "error: Java is not installed. Install a JDK, version 17 or newer (https://adoptium.net)." >&2
        exit 1
    }
    echo "Building the engine ..."
    (cd engine && ./mvnw -q -B -DskipTests package)
}

case "${1:-}" in
"")
    build_engine
    exec cargo run --locked
    ;;
--release)
    build_engine
    exec cargo run --locked --release
    ;;
-h | --help) usage ;;
*)
    usage >&2
    exit 2
    ;;
esac
