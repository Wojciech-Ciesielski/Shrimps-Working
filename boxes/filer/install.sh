#!/bin/bash
# Buduje Filera i instaluje go do DESTDIR (pakiet, obraz KIWI albo test).
#   boxes/filer/install.sh DESTDIR
# Wymaga: cargo, pakiety deweloperskie gtk4 (4.14+) i libadwaita-1 (1.5+).
# SHRIMP_PREBUILT=1: program zbudowany wcześniej (krok chroot shrimp-box).
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/filer" "$D/usr/bin/filer"
install -Dm644 "$HERE/data/org.shrimp.Filer.desktop" "$D/usr/share/applications/org.shrimp.Filer.desktop"
install -Dm644 "$HERE/data/org.shrimp.Filer.svg" "$D/usr/share/icons/hicolor/scalable/apps/org.shrimp.Filer.svg"
echo "Filer zainstalowany w $D"
