#!/bin/bash
# Atrapy zypper/rpm/lspci/lsusb do uruchomienia Cocktail poza openSUSE (zrzuty ekranu, testy).
#   . tests/fake-system.sh KATALOG   → ustawia PATH i zmienne COCKTAIL_*
# Dziennik wywołań zyppera: KATALOG/log.
set -euo pipefail
FAKE=$1
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$FAKE/bin"
: > "$FAKE/log"
cat > "$FAKE/bin/zypper" <<Z
#!/bin/bash
echo "zypper \$*" >> "$FAKE/log"
case " \$* " in
  *" list-updates "*) cat <<'X'
<?xml version='1.0'?>
<stream><update-status version="0.6"><update-list>
<update kind="package" name="kernel-default" edition="6.12.2-1.1" arch="x86_64" edition-old="6.12.1-1.1"><summary>Kernel</summary></update>
<update kind="package" name="Mesa" edition="25.2.2-1.1" arch="x86_64" edition-old="25.2.1-1.1"><summary>Mesa</summary></update>
</update-list></update-status></stream>
X
  ;;
  *" search "*) cat <<'X'
<?xml version='1.0'?>
<stream><search-result version="0.0"><solvable-list>
<solvable status="installed" name="vlc" summary="Graphical media player" kind="package"/>
<solvable status="not-installed" name="vlc-codec-gstreamer" summary="GStreamer codec for VLC" kind="package"/>
</solvable-list></search-result></stream>
X
  ;;
  *" --dry-run "*) cat <<'X'
<?xml version='1.0'?>
<stream><install-summary packages-to-change="2"><to-remove>
<solvable type="package" name="vlc" edition="3.0.21" arch="x86_64" summary="x"/>
<solvable type="package" name="vlc-qt" edition="3.0.21" arch="x86_64" summary="y"/>
</to-remove></install-summary></stream>
X
  ;;
esac
exit 0
Z
cat > "$FAKE/bin/rpm" <<'Z'
#!/bin/bash
for p in "$@"; do
  case "$p" in -q|--qf|%*) continue ;; esac
  case "$p" in vlc|git|python3|noto-sans-fonts|google-noto-coloremoji-fonts|cups|cups-filters|fwupd|cryptsetup|mokutil|rsync|gnome-disk-utility|openssh|avahi|gstreamer-plugins-base|gstreamer-plugins-good)
    echo "$p" ;;
  *) echo "package $p is not installed" ;; esac
done
Z
cat > "$FAKE/bin/lspci" <<'Z'
#!/bin/bash
printf 'Slot:\t00:02.0\nClass:\tVGA compatible controller\nVendor:\tIntel Corporation\nDevice:\tUHD Graphics\nDriver:\ti915\n\nSlot:\t01:00.0\nClass:\t3D controller\nVendor:\tNVIDIA Corporation\nDevice:\tGeForce RTX 4060 Laptop GPU\nDriver:\tnvidia\n\nSlot:\t00:1f.3\nClass:\tAudio device\nVendor:\tIntel Corporation\nDevice:\tHigh Definition Audio\nDriver:\tsnd_hda_intel\n\nSlot:\t02:00.0\nClass:\tNetwork controller\nVendor:\tIntel Corporation\nDevice:\tWi-Fi 6 AX201 160MHz\n\nSlot:\t03:00.0\nClass:\tEthernet controller\nVendor:\tRealtek Semiconductor Co., Ltd.\nDevice:\tRTL8111 PCIe GbE\n\nSlot:\t00:14.0\nClass:\tUSB controller\nVendor:\tIntel Corporation\nDevice:\tUSB 3.0 xHCI\n'
Z
cat > "$FAKE/bin/lsusb" <<'Z'
#!/bin/bash
echo "Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub"
echo "Bus 001 Device 003: ID 0bda:58e6 Realtek Semiconductor Corp. Integrated Webcam HD"
echo "Bus 001 Device 004: ID 8087:0026 Intel Corp. AX201 Bluetooth"
Z
cat > "$FAKE/history" <<Z
$(date -d '-8 days' '+%F') 09:00:00|install|ninja|1.12.1-1.1|x86_64|root@pc|oss|a|
$(date -d '-3 days' '+%F') 09:00:00|install|python3|3.13.1-1.1|x86_64|root@pc|oss|a|
$(date -d '-2 days' '+%F') 09:00:00|install|python3|3.13.2-1.1|x86_64|root@pc|oss|a|
$(date -d '-1 days' '+%F') 18:20:00|remove |kernel-default|6.11.0-1.1|x86_64|root@pc|
$(date -d '-1 days' '+%F') 18:21:00|install|Mesa|25.2.1-1.1|x86_64|root@pc|oss|a|
$(date -d '-1 days' '+%F') 18:22:00|install|Mesa|25.2.2-1.1|x86_64|root@pc|oss|a|
$(date '+%F') 08:10:00|install|vlc|3.0.21-1.1|x86_64|root@pc|oss|a|
Z
cat > "$FAKE/bin/transactional-update" <<Z
#!/bin/bash
echo "transactional-update \$*" >> "$FAKE/log"
Z
chmod +x "$FAKE/bin/"*
export PATH="$FAKE/bin:$PATH" COCKTAIL_NO_PKEXEC=1 COCKTAIL_LIBEXEC="$HERE/../system/libexec" \
  COCKTAIL_MODE=zypper COCKTAIL_HISTORY="$FAKE/history"
