#!/usr/bin/env bash
# Install a released RevoLinux tarball (binary + icon + menu entry).
#
#   ./install.sh                     -> /usr/local (system menu)
#   PREFIX="$HOME/.local" ./install.sh  -> per-user menu, no root
set -euo pipefail

cd "$(dirname "$0")"
PREFIX="${PREFIX:-/usr/local}"

SUDO=""
if [[ "$(id -u)" -ne 0 && "$PREFIX" != "$HOME"/* ]]; then
    SUDO="sudo"
fi

$SUDO install -Dm755 revolinux "$PREFIX/bin/revolinux"
$SUDO install -Dm644 revolinux.svg \
    "$PREFIX/share/icons/hicolor/scalable/apps/revolinux.svg"

# Absolute Exec path so the menu entry works without $PREFIX/bin in PATH.
DESKTOP_SRC="$(mktemp)"
trap 'rm -f "$DESKTOP_SRC"' EXIT
sed "s|^Exec=.*|Exec=$PREFIX/bin/revolinux|" revolinux.desktop > "$DESKTOP_SRC"
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
