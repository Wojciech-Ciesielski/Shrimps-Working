#!/bin/bash
# Buduje okno aplikacji internetowych i instaluje je do DESTDIR.
#   boxes/webapp-window/install.sh DESTDIR
# SHRIMP_PREBUILT=1: program zbudowany wcześniej (krok chroot shrimp-box).
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)
if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/shrimp-webapp" "$D/usr/bin/shrimp-webapp"
echo "shrimp-webapp zainstalowany w $D"
