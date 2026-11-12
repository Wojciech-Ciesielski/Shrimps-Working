#!/bin/bash
# Buduje Panko i instaluje go do DESTDIR (pakiet, obraz KIWI albo test).
#   boxes/panko/install.sh DESTDIR
# Wymaga: cargo, pakiety deweloperskie gtk4 (4.14+) i libadwaita-1 (1.5+).
# SHRIMP_PREBUILT=1: program zbudowany wcześniej (krok chroot shrimp-box).
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/panko" "$D/usr/bin/panko"
install -Dm644 "$HERE/data/org.shrimp.Panko.desktop" "$D/usr/share/applications/org.shrimp.Panko.desktop"
install -Dm644 "$HERE/data/org.shrimp.Panko.svg" "$D/usr/share/icons/hicolor/scalable/apps/org.shrimp.Panko.svg"
install -Dm644 "$HERE/data/org.shrimp.Panko.policy" "$D/usr/share/polkit-1/actions/org.shrimp.Panko.policy"
install -Dm644 "$HERE/data/panko-cleanup.service" "$D/usr/lib/systemd/system/panko-cleanup.service"
install -Dm644 "$HERE/data/panko-cleanup.timer" "$D/usr/lib/systemd/system/panko-cleanup.timer"
# panko-cleanup.timer włącza shrimp-box ([image] enable w box.toml). Bez pliku
# /etc/panko/panko.conf (ustawienie wyłączone) usługa nic nie robi.
echo "Panko zainstalowany w $D"
