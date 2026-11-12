#!/bin/bash
# Buduje Shrimp Apps i instaluje do DESTDIR (pakiet, KIWI).
#   boxes/shrimp-apps/install.sh DESTDIR [PREFIX=/usr]
# SHRIMP_PREBUILT=1: użyj gotowego target/release/shrimp-apps (krok chroot shrimp-box).
set -euo pipefail
DEST=${1:?użycie: install.sh DESTDIR [PREFIX]}
PREFIX=${2:-/usr}
cd "$(dirname "$0")"
if [ "${SHRIMP_PREBUILT:-0}" != 1 ]; then
  cargo build --release --locked
fi
install -Dm755 target/release/shrimp-apps "$DEST$PREFIX/bin/shrimp-apps"
install -Dm644 data/org.shrimp.Apps.desktop "$DEST$PREFIX/share/applications/org.shrimp.Apps.desktop"
install -Dm644 data/org.shrimp.Apps.metainfo.xml "$DEST$PREFIX/share/metainfo/org.shrimp.Apps.metainfo.xml"
install -Dm644 data/base-apps.json "$DEST$PREFIX/share/shrimp/base-apps.json"
