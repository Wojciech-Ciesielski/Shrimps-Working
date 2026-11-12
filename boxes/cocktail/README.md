# Cocktail

Aktualizacje systemu, urządzenia, pakiety i historia dla Shrimp. Nakładka na zyppera i
`transactional-update`, GTK4 + libadwaita w Rust (gtk4-rs 0.11, libadwaita 0.9; minimum GTK 4.14 i libadwaita 1.5).
Projekt: `mockups/cocktail.html` i `specs/gnome-apps.md` z paczki shrimp-handoff.

## Co działa

| Zakładka | Co robi | Skąd dane |
|---|---|---|
| Aktualizacje | Stan „System jest aktualny / Shrimp 26” z zielonym ✓, „Sprawdź aktualizacje” → „Dostępne aktualizacje, N pakietów” → „Zainstaluj aktualizacje” → „Uruchom ponownie, aby zakończyć” | `zypper --xmlout list-updates`; odświeżenie repozytoriów przez `cocktail-query refresh` |
| Urządzenia | Drzewo jak w makiecie: procesor, baterie, karty graficzne (z linią „Używasz sterownika: …”), dźwięk, kamery, sieć, Bluetooth, dyski, USB, nieznane | `lspci -vmm -k`, `lsusb`, `/proc/cpuinfo`, `/sys/class/power_supply`, `/sys/block` |
| Pakiety | Wybrane pakiety w kategoriach z makiety (Multimedia, Czcionki, Środowiska, Narzędzia programistyczne, Sieć, Drukowanie, Firmware, Zabezpieczenia, Wirtualizacja, Narzędzia systemowe) ze znacznikiem stanu i przyciskiem Zainstaluj / Usuń. Wyszukiwanie z paska bocznego filtruje kategorie i po chwili szuka w repozytoriach („Wyniki wyszukiwania”) | `rpm -q`, `zypper --xmlout search` |
| Historia | Zmiany pakietów pogrupowane: Dziś, Wczoraj, Ostatni tydzień, Wcześniej („Zainstalowano / Zaktualizowano a → b / Usunięto”) | `/var/log/zypp/history` (przez `cocktail-query history`, bo plik jest tylko dla roota) |

Wspólne: `AdwNavigationSplitView` (zwija się poniżej 560 sp), wyszukiwanie na dole paska bocznego (ukryte w Aktualizacjach),
menu ☰ z „O aplikacji Cocktail”, baner „Uruchom ponownie, aby zakończyć” po każdej zmianie w nowej migawce,
wiersze naprzemiennie jaśniejsze/ciemniejsze, ikony tylko symboliczne, czerwień nigdzie (potwierdzenie usunięcia ma przycisk w kolorze akcentu).

### Zmiany w systemie

- `system/libexec/cocktail-helper` (pkexec, akcja `org.shrimp.cocktail.manage`, hasło administratora z zapamiętaniem na kilka minut):
  `install PAKIET…`, `remove PAKIET…`, `update`. Na systemie tylko do odczytu: `transactional-update --non-interactive --continue pkg install|pkg remove|up`,
  więc kilka zmian przed restartem trafia do jednej migawki; wypisuje `reboot`. Na zwykłym systemie: `zypper --non-interactive …` wprost.
  Sprawdza nazwy pakietów i nie usuwa pakietów chronionych (jądro, glibc, systemd, zypper, snapper, GNOME Shell, GDM…).
- `system/libexec/cocktail-query` (pkexec, akcja `org.shrimp.cocktail.query`, bez hasła w aktywnej sesji): `refresh`, `history`,
  `remove-preview PAKIET…` (`zypper remove --dry-run`). Przed usunięciem Cocktail pokazuje, co jeszcze zniknie, i odmawia, jeśli na liście jest pakiet chroniony.
- Pakiety zmienione w nowej migawce mają w wierszu „Gotowe / Usunięte po ponownym uruchomieniu”.

## Budowa i testy

```sh
cd boxes/cocktail
cargo build --release          # potrzebne: libgtk-4-dev/gtk4-devel, libadwaita-1-dev/libadwaita-devel
cargo test                     # parsery zyppera, historii, lspci/lsusb, odmiana liczebników
tests/test-helper.sh           # pomocnicy na atrapach zypper/transactional-update
./install.sh DESTDIR           # /usr/bin/cocktail, /usr/libexec/cocktail-*, polkit, .desktop
```

Uruchomienie poza openSUSE na atrapach (bez pkexec): `. tests/fake-system.sh /tmp/fake && cargo run`
(`COCKTAIL_MODE=transactional` pokazuje ścieżkę z migawką, `COCKTAIL_PAGE=pakiety` otwiera zakładkę).
Workflow **Cocktail** w GitHub Actions robi to samo.

## Czego brakuje

1. **Nie testowane na prawdziwym Leap 16.1.** Format XML zyppera i dziennika historii jest z dokumentacji i pamięci, sprawdzony tylko na atrapach.
   Pierwsza rzecz do zrobienia: uruchomić na nośniku/VM i porównać z `zypper --xmlout`.
2. **Na ISO od tej zmiany:** box cocktail w kontenerze desktop: shrimp-box buduje go w chroocie Leap, a post-install Setup kopiuje go do zainstalowanego systemu. „Aktualizacje” w menu Shrimp otwierają Cocktail, gdy jest w systemie.
3. **Nazwy pakietów w katalogu** (`src/catalog.rs`) są nazwami openSUSE, ale nie zostały sprawdzone na Leap 16.1 (np. `nodejs-default`, `fetchmsttfonts`, `qemu-kvm`).
   Kodeki są te same co w Setup (bez Packmana). Kategorii „Wygląd” (rozszerzenia GNOME, motywy Plymouth/GRUB) z makiety nie ma.
4. **Sterowniki w Urządzeniach:** lista jest tylko do odczytu. Brakuje „Zainstaluj wszystkie” (sterownik NVIDIA przez logikę z `shrimp-setup-apply`, firmware przez fwupd) i znaczników „do zainstalowania / do aktualizacji”.
5. **Ustawienia z menu ☰** z makiety: „Sprawdzaj aktualizacje automatycznie”, „Usuwaj stare jądra automatycznie”, „Usuwaj historię co N dni” (wymagają timera systemd / konfiguracji zypp przez pomocnika).
6. **Pomoc:** przycisk pomocy czeka na adresy stron pomocy (otwarta decyzja).
7. **Wydania Shrimp** („Dostępne wydanie Shrimp 26.1, pobranie około 2,1 GB”): na razie tylko zwykłe aktualizacje pakietów (`up`); aktualizacja wydania (`dup` + zmiana repozytoriów) do zrobienia.
8. **Tłumaczenia:** teksty są po polsku w kodzie; brakuje podpięcia pod `i18n/` jak w Setup.
9. **Postęp:** w czasie instalacji jest tylko „Instalowanie…”, bez paska postępu i bez anulowania.
10. **Ikona aplikacji:** tymczasowo `system-software-update` (z MoreWaita, gdy będzie głównym motywem ikon).
