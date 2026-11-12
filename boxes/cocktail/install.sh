#!/bin/bash
# Buduje Cocktail i instaluje go do DESTDIR (pakiet, obraz KIWI albo test).
#   boxes/cocktail/install.sh DESTDIR
# Wymaga: cargo, pakiety deweloperskie gtk4 (4.14+) i libadwaita (1.5+).
# SHRIMP_PREBUILT=1: plik target/release/cocktail zbudowany wcześniej.
set -euo pipefail
D=${1:?użycie: install.sh DESTDIR}
HERE=$(cd "$(dirname "$0")" && pwd)
if [ -z "${SHRIMP_PREBUILT:-}" ]; then
  (cd "$HERE" && cargo build --release --locked)
fi
install -Dm755 "$HERE/target/release/cocktail" "$D/usr/bin/cocktail"
install -Dm755 "$HERE/system/libexec/cocktail-helper" "$D/usr/libexec/cocktail-helper"
install -Dm755 "$HERE/system/libexec/cocktail-query" "$D/usr/libexec/cocktail-query"
install -Dm644 "$HERE/system/polkit/org.shrimp.cocktail.policy" "$D/usr/share/polkit-1/actions/org.shrimp.cocktail.policy"
install -Dm644 "$HERE/system/applications/org.shrimp.Cocktail.desktop" "$D/usr/share/applications/org.shrimp.Cocktail.desktop"
echo "Cocktail zainstalowany w $D"
