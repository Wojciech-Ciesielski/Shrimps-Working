# Panko

Cofanie zmian w systemie Shrimp: przeglądanie migawek, porównanie z obecnym stanem,
przywracanie plików i całego systemu. Nakładka GTK4 + libadwaita (Rust) na migawki
systemu (usługa `org.opensuse.Snapper`). Projekt: `shrimp-handoff/mockups/panko.html`,
spec: `shrimp-handoff/specs/gnome-apps.md` › Panko.

## Uruchomienie

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev     # Ubuntu 24.04
sudo zypper in 'pkgconfig(gtk4)' 'pkgconfig(libadwaita-1)'   # Leap 16
cd boxes/panko
cargo run -- --demo        # przykładowe dane z makiety, bez usługi migawek
cargo run                  # prawdziwe migawki
cargo test                 # testy (bez usługi)
boxes/panko/install.sh DESTDIR   # program, plik .desktop, reguła polkit
```

Do podglądu i testów: `PANKO_PAGE=zmiany|zaawansowane`, `PANKO_SELECT=<numer migawki>`,
`ADW_DEBUG_COLOR_SCHEME=prefer-dark`. Workflow **Panko** w GitHub Actions buduje program,
uruchamia testy.

## Co działa

- **Migawki**: grupy Dziś / Wczoraj / Ostatni tydzień / miesiące (zwijane), paski,
  ikona i nazwa według rodzaju (po/przed aktualizacją, cykliczna, przed przywróceniem,
  instalacja, ręczna z opisem). Klik zaznacza migawkę, drugi klik odznacza.
  - „Utwórz migawkę” (okno z opisem) – D-Bus `CreateSingleSnapshot`, znacznik `panko=yes`.
  - „Usuń” – `DeleteSnapshots`; para przed/po aktualizacji znika razem.
  - „Pokaż zmiany” – przejście do Zmian z tą migawką.
  - „Przywróć system” – `transactional-update rollback N` (albo `snapper rollback N`
    bez transactional-update) przez pkexec, potem restart przez logind.
- **Zmiany**: „Porównaj z” (lista migawek), pliki zmienione od migawki do teraz, grupy
  katalogów (/etc, /usr/bin, /var/lib…), znaczniki Dodany / Usunięty / Zmieniony, pola wyboru,
  „Przywróć wybrane” – `snapper undochange N..0 pliki` przez pkexec. Lista to GtkListView,
  więc znosi też dziesiątki tysięcy plików po dużej aktualizacji (duże porównania startują zwinięte).
- **Zaawansowane**: woluminy (system zawsze włączony, /home włącz/wyłącz =
  `create-config`/`delete-config`), czyszczenie (TIMELINE_LIMIT_*, NUMBER_LIMIT,
  EMPTY_PRE_POST_CLEANUP, SPACE_LIMIT w %), dostęp (ALLOW_USERS). Zmiany czekają 0,9 s
  i idą jednym `snapper set-config` (jedno okno hasła).
- **Menu główne**: migawki cykliczne (TIMELINE_CREATE), „Usuwaj stare migawki” + „Starsze niż N dni”,
  O aplikacji Panko. Usługa migawek nie ma limitu wieku, więc Panko robi to sam: ustawienie leży
  w `/etc/panko/panko.conf` (`MAX_AGE_DAYS`, zapis przez `pkexec panko --set-max-age N|off`),
  a `panko-cleanup.timer` raz dziennie uruchamia `panko --cleanup` jako root. Czyszczenie nigdy nie
  usuwa najnowszej migawki, pary „przed/po aktualizacji” usuwa razem (i tylko gdy obie są starsze
  niż limit), a bieżącej, domyślnej i aktywnej migawki nie ruszy usługa migawek. Domyślnie wyłączone.
  Sprawdzone na prawdziwym snapperd: migawki z datą sprzed 100 i 50 dni znikają, najnowsze zostają.
- **Wyszukiwanie** na dole paska bocznego filtruje listę otwartej strony;
  „Nic nie pasuje do tego wyszukiwania.”
- **Stany**: wczytywanie, „Brak dostępu do migawek” z przyciskiem „Zezwól” (dopisuje konto do
  ALLOW_USERS z SYNC_ACL), „Migawki są niedostępne” (brak usługi), „Migawki są wyłączone”
  („Włącz” tworzy konfigurację dla /). Błędy jako krótki komunikat na dole okna; zamknięcie
  okna hasła nie jest błędem.
- **Pomoc**: przycisk ? otwiera `boxes/help-links/help.json` › base + `/panko?lang=..&from=panko`.
  Dopóki `base` jest puste, przycisk jest nieaktywny.
- Polskie znaki w opisach i nazwach plików: kodowanie `\x??` usługi (`src/escape.rs`).

Sprawdzone: `cargo test` (13 testów: nazwy migawek, grupy dni, grupy plików, ustawienia,
kodowanie, adres pomocy, wiek migawek) i test na prawdziwej usłudze `cargo test -- --ignored dbus`
(`PANKO_TEST_CONFIG=<konfiguracja>`): Btrfs w pliku, snapperd, tworzenie z polskim opisem,
lista, porównanie (zmieniony + dodany plik), usuwanie – jako root i jako zwykłe konto
z ALLOW_USERS. Okno uruchomione na tej usłudze jako root (lista) i jako konto bez dostępu
(stan „Brak dostępu”).

## Uprawnienia (ważne dla obrazu)

Usługa migawek wpuszcza do odczytu i tworzenia/usuwania tylko roota i konta z `ALLOW_USERS` /
`ALLOW_GROUPS` w konfiguracji. Zmiana ustawień, przywracanie plików i systemu idą zawsze przez
pkexec (`data/org.shrimp.Panko.policy`, `auth_admin_keep` dla `/usr/bin/snapper` i
`/usr/sbin/transactional-update`). Jeśli w usłudze brakuje uprawnień do utworzenia lub usunięcia
migawki, Panko robi to samo przez pkexec.

## Czego brakuje

1. **Na ISO od tej zmiany:** box panko w kontenerze desktop: shrimp-box buduje go w chroocie Leap, `configure.sh` boxa
   włącza `panko-cleanup.timer`, a `boxes/setup/system/share/post-install.sh` kopiuje go do zainstalowanego
   systemu (dopóki nie ma pakietu). Menu Shrimp otwiera go jako „Panko”.
2. **Dostęp bez pytania:** post-install ustawia `ALLOW_GROUPS="wheel"` i `SYNC_ACL="yes"` w konfiguracji
   `root`, więc administrator nie widzi stanu „Brak dostępu”. Do sprawdzenia na zainstalowanym systemie.
3. **Ikona aplikacji** `org.shrimp.Panko` – nie ma jej ani w Adwaita, ani w paczce projektu.
   Do wybrania z MoreWaita albo do narysowania (zasada: tylko prawdziwe ikony).
4. **„Migawka przed aktualizacją”** w menu jest zawsze włączona i nieaktywna: na systemie
   niezmiennym transactional-update zawsze robi migawkę i nie ma ustawienia, żeby tego nie robić.
   Do decyzji: zostawić jako informację, ukryć albo powiązać z wtyczką zyppera.
5. **Timer czyszczenia** jest włączany w obrazie (`boxes/panko/configure.sh`) i w zainstalowanym systemie (post-install).
6. **Tylko migawki systemu (/)** na listach Migawki i Zmiany. Migawki /home (po włączeniu
   w Zaawansowanych) są tworzone, ale nie ma jeszcze ich przeglądania (np. przełącznik woluminu).
7. **Przywracanie plików z /usr** na systemie niezmiennym nie przejdzie (/usr tylko do odczytu);
   działa dla /etc, /var, /home. Dla plików systemowych właściwe jest „Przywróć system”.
   Panko pokazuje wtedy komunikat o błędzie; warto ukryć pola wyboru w /usr.
8. **Tłumaczenia** – teksty są po polsku w kodzie. Do przeniesienia do formatu `i18n/`
   (jak Setup) albo gettext.
9. **Sprawdzenie na Leap 16.1 w sesji GNOME** (Wayland, polkit, prawdziwy transactional-update
   rollback i restart). W kontenerze sprawdzone były tylko X11 + usługa migawek bez polkit.
10. Restart po „Przywróć system” jest natychmiastowy. Do rozważenia okno „Uruchom ponownie teraz”.

## Pliki

- `src/model.rs` – migawki, nazwy, grupy dni, zmiany plików, ustawienia (czyste, z testami)
- `src/backend.rs` – usługa D-Bus + pkexec; `src/demo.rs` – dane z makiety
- `src/ui/` – okno (`mod.rs`), strony `snapshots.rs`, `changes.rs`, `advanced.rs`, lista `tree.rs`
- `src/retention.rs` – czyszczenie starych migawek (ustawienie, wybór migawek do usunięcia)
- `data/` – plik .desktop, reguła polkit, jednostki panko-cleanup; `install.sh` – instalacja do DESTDIR
