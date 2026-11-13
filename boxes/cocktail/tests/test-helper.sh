#!/bin/bash
# Test pomocników cocktail-helper i cocktail-query na atrapach zypper/transactional-update.
#   boxes/cocktail/tests/test-helper.sh
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
# shellcheck source=fake-system.sh
. "$HERE/fake-system.sh" "$T"
H=$COCKTAIL_LIBEXEC/cocktail-helper
Q=$COCKTAIL_LIBEXEC/cocktail-query
fail() { echo "FAIL: $*"; cat "$T/log"; exit 1; }

# Zwykły system: zypper wprost, bez restartu.
: > "$T/log"; OUT=$(COCKTAIL_MODE=zypper "$H" install vlc git)
grep -q '^zypper --non-interactive install --auto-agree-with-licenses vlc git$' "$T/log" || fail "install zypper"
[ -z "$OUT" ] || fail "install zypper: nie powinno być reboot"

# Niezmienny system: transactional-update --continue, potem restart.
: > "$T/log"; OUT=$(COCKTAIL_MODE=transactional "$H" install vlc)
grep -q '^transactional-update --non-interactive --continue pkg install --auto-agree-with-licenses vlc$' "$T/log" || fail "install tu"
[ "$OUT" = reboot ] || fail "install tu: brak reboot"
: > "$T/log"; COCKTAIL_MODE=transactional "$H" remove vlc >/dev/null
grep -q 'pkg remove vlc$' "$T/log" || fail "remove tu"
: > "$T/log"; COCKTAIL_MODE=transactional "$H" update >/dev/null
grep -q -- '--continue up$' "$T/log" || fail "update tu"

: > "$T/log"; COCKTAIL_MODE=transactional "$H" update-pkg vlc >/dev/null
grep -q -- '--continue pkg update --auto-agree-with-licenses vlc$' "$T/log" || fail "update-pkg tu"

# Aktualizacje w tle: przez shrimp-auto-updates z CoreOS.
printf '#!/bin/bash\necho "auto $*" >> "%s/log"\n' "$T" > "$T/bin/au"; chmod +x "$T/bin/au"
: > "$T/log"; COCKTAIL_AUTO_UPDATES=$T/bin/au "$H" auto-updates on; grep -q '^auto on$' "$T/log" || fail "auto-updates on"
if COCKTAIL_AUTO_UPDATES=$T/bin/au "$H" auto-updates maybe 2>/dev/null; then fail "auto-updates maybe"; fi
# Sterowniki: atrapa nie ma aktualizacji sterowników ani fwupdmgr, więc nic nie zmienia.
: > "$T/log"; OUT=$(COCKTAIL_MODE=transactional "$H" drivers); [ -z "$OUT" ] || fail "drivers bez aktualizacji"
grep -q 'transactional-update' "$T/log" && fail "drivers: niepotrzebna migawka"

# Odmowy: chronione pakiety, dziwne nazwy, nieznane polecenia.
for bad in "remove glibc" "remove kernel-default" "install -rf" "install a;b" "install" "frobnicate x" "update x" "drivers x" "auto-updates" "update-pkg" "update-pkg -x"; do
  : > "$T/log"
  # shellcheck disable=SC2086
  if "$H" $bad 2>/dev/null; then fail "przyjęto: $bad"; fi
  [ ! -s "$T/log" ] || fail "wywołano zyppera dla: $bad"
done

# cocktail-query
: > "$T/log"; "$Q" refresh; grep -q 'refresh$' "$T/log" || fail "refresh"
"$Q" history | grep -q '|install|vlc|' || fail "history"
"$Q" remove-preview vlc | grep -q 'name="vlc-qt"' || fail "remove-preview"
if "$Q" remove-preview '--foo' 2>/dev/null; then fail "remove-preview --foo"; fi
echo "cocktail: testy pomocników OK"
