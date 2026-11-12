#!/bin/bash
# Buduje Cherry i instaluje go do DESTDIR (pakiet, obraz KIWI albo test).
#   boxes/cherry/install.sh DESTDIR
# Wymaga: cargo, pakiety deweloperskie gtk4 (4.14+), libadwaita-1 (1.5+) i vte-2.91-gtk4 (0.76+).
# SHRIMP_PREBUILT=1: program zbudowany wcześniej (krok chroot shrimp-box).
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/cherry" "$D/usr/bin/cherry"
install -Dm644 "$HERE/data/org.shrimp.Cherry.desktop" "$D/usr/share/applications/org.shrimp.Cherry.desktop"
install -Dm644 "$HERE/data/org.shrimp.Cherry.svg" "$D/usr/share/icons/hicolor/scalable/apps/org.shrimp.Cherry.svg"
echo "Cherry zainstalowany w $D"
