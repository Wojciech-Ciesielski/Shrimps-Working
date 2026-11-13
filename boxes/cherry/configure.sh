# Fragment config.sh boxa cherry (chroot obrazu; składa go shrimp-box).
# Narzędzia wiersza poleceń z /usr/share/cherry/tools.txt (install.sh). Nazwy różnią się między wydaniami
# openSUSE, a config.kiwi przerywa budowę na nieznanej nazwie, więc instalujemy je tu i miękko:
# pierwsza dostępna nazwa z każdej linii, brak to WARNING w dzienniku budowy.
TOOLS=/usr/share/cherry/tools.txt
VERSION_ID="$(. /etc/os-release && echo "$VERSION_ID")"
LEAP="https://download.opensuse.org/distribution/leap/${VERSION_ID}/repo/oss"
CHERRY_REPO=""
for candidate in "$LEAP/x86_64" "$LEAP"; do
  if curl -fsSLI "$candidate/repodata/repomd.xml" >/dev/null 2>&1; then CHERRY_REPO="$candidate"; break; fi
done
if [ -f "$TOOLS" ] && [ -n "$CHERRY_REPO" ]; then
  zypper -n --gpg-auto-import-keys addrepo -f "$CHERRY_REPO" shrimp-build-cherry
  zypper -n --gpg-auto-import-keys refresh shrimp-build-cherry
  while read -r -a group; do
    case "${group[0]:-#}" in \#*) continue ;; esac
    ok=""
    for p in "${group[@]}"; do
      if rpm -q --whatprovides "$p" </dev/null >/dev/null 2>&1 \
        || zypper -n --gpg-auto-import-keys install --no-recommends "$p" </dev/null >/dev/null 2>&1; then
        echo "optional package installed: $p"; ok=1; break
      fi
    done
    [ -n "$ok" ] || echo "WARNING: Cherry: none of these packages is available: ${group[*]}"
  done < "$TOOLS"
  zypper -n removerepo shrimp-build-cherry || true
  zypper -n clean -a
else
  echo "WARNING: Cherry: Leap ${VERSION_ID} repository not reachable, command line tools skipped"
fi
