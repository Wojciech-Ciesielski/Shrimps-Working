# Shrimps-Working

Repozytorium robocze aplikacji **Shrimps**: natywnych programów Shrimp (GTK4 + libadwaita, Rust), które zastępują domyślne odpowiedniki z GNOME i są wbudowane w ISO.

| Box | Co to jest | CI |
|---|---|---|
| `shrimp-apps` | sklep z aplikacjami Flathub i PWA | Shrimp Apps |
| `cocktail` | aktualizacje i pakiety (zypper, transactional-update) | Cocktail |
| `panko` | migawki i cofanie zmian (snapper) | Panko |
| `shrimp-i18n`, `web-apps` | biblioteki tych aplikacji | Web Apps |
| `i18n-core`, `help-links`, `shrimp-box` | kopie wspólnych boxów (patrz niżej) | Boxy |

Kontener: `containers/shrimps.toml`. Zasady boxów: [`boxes/README.md`](boxes/README.md), sprawdzenie `cd boxes/shrimp-box && cargo run -- check` (workflow **Boxy**).

## Jak trafia do ISO

CoreOS-Working przypina wydanie tego repozytorium w `tools/shrimps.ref` (`Wojciech-Ciesielski/Shrimps-Working@<tag>`) i `tools/fetch-shrimps.sh` dokłada stąd boxy przed budową obrazu. Aplikacje budują się tam w chroocie Leap razem z obrazem. Nowe aplikacje w ISO = nowe wydanie tutaj (workflow **Release**) + zmiana `tools/shrimps.ref` w CoreOS.

Zdalne repozytorium Flathub w systemie daje box `flathub` w CoreOS-Working.

## Kopie wspólnych boxów

Repozytoria są niezależne, więc małe wspólne boxy są kopiowane, a nie dzielone. Zmieniaj je najpierw w repozytorium źródłowym, potem przenieś kopię:

| Box | Źródło |
|---|---|
| `shrimp-box`, `i18n-core` | CoreOS-Working |
| `help-links` | Site-Working |
| `install-schema` | Install-Working |

Zasada: żaden PR nie dodaje zrzutów ekranu ani `test-results/` (workflow **Bez zrzutów**).
