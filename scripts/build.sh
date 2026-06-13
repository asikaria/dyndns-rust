#!/usr/bin/env bash
set -euo pipefail

log_info() {
    echo -e "\033[1;32m[INFO]\033[0m $1"
}

log_warn() {
    echo -e "\033[1;33m[WARN]\033[0m $1"
}

log_error() {
    echo -e "\033[1;31m[ERROR]\033[0m $1"
}

# Navigate to project root directory (parent of scripts/)
cd "$(dirname "$0")/.."

BIN_NAME="dyndns"
RELEASE_DIR="release"

# Linux cross targets to build, formatted as "rust-triple:release-subdir".
CROSS_TARGETS=(
    "aarch64-unknown-linux-gnu:arm64-linux"
    "x86_64-unknown-linux-gnu:amd64-linux"
)

# cross handles the Linux toolchains inside a container (needs Docker/Podman)
if ! command -v cross &> /dev/null; then
    log_error "cross is not installed. Install it with: cargo install cross --git https://github.com/cross-rs/cross"
    exit 1
fi

# Host target triple drives the native build's output path and binary suffix
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
NATIVE_BIN="$BIN_NAME"
case "$HOST_TRIPLE" in
    *windows*) NATIVE_BIN="$BIN_NAME.exe" ;;
esac

log_info "1. Native release build ($HOST_TRIPLE)..."
cargo build --release

step=2
for entry in "${CROSS_TARGETS[@]}"; do
    triple="${entry%%:*}"
    log_info "$step. Cross release build ($triple)..."
    cross build --target "$triple" --release
    step=$((step + 1))
done

# Copies the binary plus the files we ship to a target machine into a staging dir,
# preferring the real secret/config files and falling back to the templates.
stage_payload() {
    local dest="$1"
    local bin_src="$2"
    mkdir -p "$dest"
    cp "$bin_src" "$dest/"
    cp README.md "$dest/README.md"
    cp scripts/install.sh "$dest/install.sh"

    if [[ -f .env ]]; then
        cp .env "$dest/.env"
    else
        log_warn "No .env found; staging .env.example as .env in $dest"
        cp .env.example "$dest/.env"
    fi

    if [[ -f config.toml ]]; then
        cp config.toml "$dest/config.toml"
    else
        log_warn "No config.toml found; staging config.toml.example as config.toml in $dest"
        cp config.toml.example "$dest/config.toml"
    fi
}

log_info "$step. Staging release artifacts in $RELEASE_DIR/..."
rm -rf "$RELEASE_DIR"
stage_payload "$RELEASE_DIR/native" "target/release/$NATIVE_BIN"
for entry in "${CROSS_TARGETS[@]}"; do
    triple="${entry%%:*}"
    subdir="${entry##*:}"
    stage_payload "$RELEASE_DIR/$subdir" "target/$triple/release/$BIN_NAME"
done

log_info "Build complete. Release artifacts:"
echo "  - $RELEASE_DIR/native/ (native: $HOST_TRIPLE)"
for entry in "${CROSS_TARGETS[@]}"; do
    triple="${entry%%:*}"
    subdir="${entry##*:}"
    echo "  - $RELEASE_DIR/$subdir/ ($triple)"
done
