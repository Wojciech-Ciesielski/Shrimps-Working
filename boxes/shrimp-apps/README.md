# Shrimp Apps

Sklep z aplikacjami Shrimp: Flathub i aplikacje internetowe. GTK4 + libadwaita + Rust, backend libflatpak. Projekt: `specs/gnome-apps.md` i `mockups/shrimp-apps.html` z `shrimp-handoff.zip`.

## Co działa

- **Katalog Flathub.** Przy starcie Shrimp Apps sprawdza, czy jest repozytorium `flathub` w instalacji systemowej (na Shrimp dodaje je box `flathub`; gdy go nie ma, dodaje je z `flathub.flatpakrepo`). Metadane AppStream pobiera libflatpak (`update_appstream_full_sync`), odświeżane, gdy są starsze niż 6 godzin albo po „Odśwież katalog” (Ctrl+R). Bez sieci pokazuje ostatni zapisany katalog, a gdy go nie ma, ekran „Brak połączenia z internetem” z „Spróbuj ponownie”.
- **Odkrywaj** (jak w makiecie): karuzela „Wybór tygodnia” (popularne aplikacje z kolorem marki z AppStream i zrzutami, co tydzień inny zestaw, przewija się co 7 s), pod nią sekcje 8 kategorii z siatką 3 kolumny × 3 wiersze, strzałkami i przejściem do kategorii. Kolejność według popularności z API Flathub (`/api/v2/collection/popular`, zapisywane na dobę), bez sieci alfabetycznie.
- **Kategorie** z paska bocznego: Grafika, Wideo, Muzyka, Gry, Biuro, Internet, Programowanie, Narzędzia (mapowanie kategorii freedesktop w `src/categories.rs`). Pełna lista w siatce (GtkGridView, działa płynnie przy tysiącach aplikacji).
- **Wyszukiwanie** na dole paska bocznego (Ctrl+F): nazwa, słowa kluczowe, identyfikator, opis; bez polskich znaków też trafia („zolw” → „Żółw”). Najpierw dopasowanie nazwy, potem popularność.
- **Strona aplikacji**: ikona, nazwa, twórca (z ✓ dla sprawdzonych na Flathub), zrzuty ekranu w karuzeli, opis, a na dole wersja, rozmiar (do pobrania albo zajęty), kategoria, licencja, twórca, strona WWW. Teksty w języku systemu, gdy Flathub je ma (inaczej angielskie).
- **Instalacja, aktualizacja, usuwanie** przez `FlatpakTransaction` w osobnym wątku, z postępem na przycisku („37 %”) i pasku na stronie aplikacji, „Anuluj”, kolejką (kilka „Pobierz” naraz czeka na swoją kolej) i potwierdzeniem usuwania. Instalacja dla całego systemu; hasło (polkit) pyta flatpak-system-helper. Po instalacji powiadomienie z „Otwórz”.
- **Zainstalowane**: lista aplikacji (także spoza katalogu), na górze „Aktualizacje (N)” i „Zaktualizuj wszystko” (jedna transakcja, jedno pytanie o hasło). Aktualizacje aplikacji są tutaj, nie w Cocktail.
- **Otwórz** uruchamia aplikację przez libflatpak.
- Linki `appstream://ID` otwierają stronę aplikacji (plik `.desktop` obsługuje `x-scheme-handler/appstream`).
- Jasny i ciemny motyw według systemu, akcent z ustawień GNOME, wąskie okno zwija pasek boczny.
- **Aplikacje internetowe**: strona z biblioteki `boxes/web-apps` (`shrimp_webapps::page()`): katalog popularnych aplikacji i tworzenie własnych.
- Słowo „flatpak” nie pojawia się w interfejsie.

## Czego brakuje

- **Na nośniku ISO**: box shrimp-apps w kontenerze desktop: shrimp-box buduje program w chroocie Leap i instaluje go do obrazu, a post-install Setup kopiuje go do zainstalowanego systemu. Menu Shrimp otwiera go jako „Oprogramowanie”.
- **Ikona programu**: na razie `system-software-install` z motywu; własnej ikony w paczce nie ma.
- **Tłumaczenia**: interfejs tylko po polsku (format `i18n/` do podpięcia).
- **Strona pomocy**: temat `apps` w `boxes/help-links/help.json`; dopóki `base` jest puste, przycisk i pozycja „Pomoc” są nieaktywne.
- **Aktualizacje w tle i powiadomienia** o nowych wersjach (dziś sprawdzane przy otwarciu i odświeżeniu).
- Uprawnienia aplikacji, oceny i recenzje, dodatki (np. wtyczki), wybór gałęzi (beta) i zmiana repozytorium: świadomie pominięte.
- Ikony z GNOME Icon Library (`compass2`, `library`, `globe`) użyte są, gdy motyw je ma (np. MoreWaita); inaczej zapasowe z Adwaita.

## Budowa i testy

```
sudo apt install libgtk-4-dev libadwaita-1-dev libflatpak-dev   # Ubuntu 24.04 (CI)
sudo zypper in 'pkgconfig(gtk4)' 'pkgconfig(libadwaita-1)' 'pkgconfig(flatpak)'   # Leap 16.1
cargo test && cargo run
boxes/shrimp-apps/install.sh DESTDIR            # bin, .desktop, metainfo
```

Wymagania: GTK 4.14, libadwaita 1.5, flatpak 1.14 (Leap 16.1 ma nowsze).

- `shrimp-apps --self-test [ID]`: bez okna wczytuje katalog, szuka, a z ID instaluje, aktualizuje i usuwa aplikację.
- `tests/test-repo.py build DIR`: lokalne repozytorium udające Flathub (33 aplikacje testowe z ikonami, zrzutami i AppStream), `bump DIR ID` wydaje nową wersję. Z `SHRIMP_APPS_INSTALLATION=user` program używa instalacji użytkownika (tylko do testów).
- Workflow **Shrimp Apps** robi to wszystko w CI: testy, repozytorium testowe (instalacja, aktualizacja 1.0 → 1.1, usuwanie), prawdziwy Flathub (katalog, wyszukiwanie, instalacja i usunięcie Flatseal).

## Budowa kodu

| Plik | Zadanie |
|---|---|
| `src/backend.rs` | wątek libflatpak: repozytorium, katalog, zainstalowane, transakcje, uruchamianie |
| `src/catalog.rs` | parser AppStream (oba formaty tłumaczeń), wyszukiwanie |
| `src/categories.rs` | kategorie paska bocznego |
| `src/net.rs` | obrazy z pamięcią podręczną (`~/.cache/shrimp-apps`), popularność z API Flathub |
| `src/store.rs` | stan wspólny: katalog, operacje, obserwatorzy |
| `src/window.rs` | okno, pasek boczny, Odkrywaj, kategorie, wyszukiwanie, Zainstalowane |
| `src/details.rs` | strona aplikacji |
| `src/selftest.rs` | `--self-test` |
