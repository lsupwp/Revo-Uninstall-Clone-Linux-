#!/usr/bin/env bash
# Install RevoLinux.
#   sudo packaging/install.sh                 -> /usr/local (system menu)
#   PREFIX="$HOME/.local" packaging/install.sh -> per-user menu, no root
set -euo pipefail

cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-/usr/local}"
BIN="${BIN:-target/release/revolinux}"

if [[ ! -x "$BIN" ]]; then
    echo "error: $BIN not found. Run: cargo build --release" >&2
    exit 1
fi

# Only ask for sudo when writing outside the user's home.
SUDO=""
if [[ "$(id -u)" -ne 0 && "$PREFIX" != "$HOME"/* ]]; then
    SUDO="sudo"
fi

$SUDO install -Dm755 "$BIN" "$PREFIX/bin/revolinux"
$SUDO install -Dm644 assets/revolinux.svg \
    "$PREFIX/share/icons/hicolor/scalable/apps/revolinux.svg"

# Use an absolute Exec path so the menu entry works even when
# $PREFIX/bin is not in the session PATH.
DESKTOP_SRC="$(mktemp)"
trap 'rm -f "$DESKTOP_SRC"' EXIT
sed "s|^Exec=.*|Exec=$PREFIX/bin/revolinux|" assets/revolinux.desktop > "$DESKTOP_SRC"
$SUDO install -Dm644 "$DESKTOP_SRC" "$PREFIX/share/applications/revolinux.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    $SUDO update-desktop-database "$PREFIX/share/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    $SUDO gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    $SUDO kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi

echo "Installed to $PREFIX/bin/revolinux"
