#!/bin/bash
# Buduje Shrimp Center i instaluje go do DESTDIR (pakiet, obraz KIWI albo test).
#   boxes/shrimp-center/install.sh DESTDIR
# Wymaga: cargo, pakiety deweloperskie gtk4 (4.14+) i libadwaita-1 (1.5+).
# SHRIMP_PREBUILT=1: program zbudowany wcześniej (krok chroot shrimp-box).
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/shrimp-center" "$D/usr/bin/shrimp-center"
install -Dm644 "$HERE/data/org.shrimp.Center.desktop" "$D/usr/share/applications/org.shrimp.Center.desktop"
install -Dm644 "$HERE/data/org.shrimp.Center.svg" "$D/usr/share/icons/hicolor/scalable/apps/org.shrimp.Center.svg"
install -Dm644 "$HERE/data/org.shrimp.Center.policy" "$D/usr/share/polkit-1/actions/org.shrimp.Center.policy"
echo "Shrimp Center zainstalowany w $D"
