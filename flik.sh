#!/usr/bin/env bash
# ==============================================================================
# Flik Runner - Log Analyzer (Native or Docker)
# ==============================================================================

set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# If user wants explicit native or Docker
MODE="${FLIK_MODE:-auto}"

if [ "$MODE" = "native" ]; then
    exec "$DIR/target/release/flik" "$@"
fi

if command -v cargo >/dev/null 2>&1 && [ "$MODE" = "auto" ]; then
    RELEASE_BIN="$DIR/target/release/flik"
    if [ ! -x "$RELEASE_BIN" ] \
        || [ "$DIR/Cargo.toml" -nt "$RELEASE_BIN" ] \
        || { [ -f "$DIR/Cargo.lock" ] && [ "$DIR/Cargo.lock" -nt "$RELEASE_BIN" ]; } \
        || find "$DIR/src" -type f -newer "$RELEASE_BIN" -print -quit | grep -q .; then
        echo "==> Compilation du binaire natif Flik..."
        cargo build --release --manifest-path "$DIR/Cargo.toml"
    fi
    exec "$DIR/target/release/flik" "$@"
fi

if [ "$MODE" = "auto" ] && [ -x "$DIR/target/release/flik" ]; then
    exec "$DIR/target/release/flik" "$@"
fi

if command -v docker >/dev/null 2>&1; then
    echo "==> Lancement de Flik via Docker..."
    if ! docker image inspect flik:latest >/dev/null 2>&1; then
        echo "==> Construction de l'image Docker flik:latest..."
        docker build -t flik:latest "$DIR"
    fi
    DOCKER_FLAGS="-it"
    if [ ! -t 0 ]; then
        DOCKER_FLAGS="-i"
    fi
    exec docker run $DOCKER_FLAGS --rm \
        -v "$PWD":/workspace \
        -w /workspace \
        flik:latest "$@"
fi

echo "Erreur : Ni Cargo ni Docker ne sont disponibles."
exit 1
