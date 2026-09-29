#!/bin/sh
# Build and install Quran for the current user (or system-wide with PREFIX=/usr/local and sudo).
#   ./scripts/install.sh              # installs to ~/.local
#   ./scripts/install.sh --uninstall
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/quran"
APPS="$PREFIX/share/applications/com.quran.Desktop.desktop"
ICON="$PREFIX/share/icons/hicolor/scalable/apps/com.quran.Desktop.svg"
cd "$(dirname "$0")/.."

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN" "$APPS" "$ICON"
    echo "Removed Quran from $PREFIX (your data in ~/.local/share/quran-desktop is kept)."
    exit 0
fi

cargo build --release
install -Dm755 target/release/quran "$BIN"
install -Dm644 data/com.quran.Desktop.desktop "$APPS"
install -Dm644 data/icons/com.quran.Desktop.svg "$ICON"
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo "Installed $BIN"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not on your PATH." ;;
esac
