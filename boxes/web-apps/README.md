# Aplikacje internetowe (Shrimp Apps)

Biblioteka `shrimp-webapps`: katalog 150 popularnych aplikacji webowych i tworzenie ich
jednym kliknięciem. Shrimp Apps wstawia ją jako stronę „Aplikacje internetowe”:

```rust
let content = shrimp_webapps::page(); // gtk::Widget, bez własnego paska tytułu
```

- `data/webapps.json`: 150 pozycji (`id`, `name`, `url`, `category`, opcjonalnie `names.pl`, `icon`, `icon_url` = ręcznie wskazana ikona).
  Kategorie jak w pasku bocznym Shrimp Apps. Kolejność w kategorii = popularność.
- `data/webapps-reserve.json`: 49 kolejnych pozycji do podmiany, gdy któraś z głównych przestanie działać.
- `data/icons/<id>.png`: ikony 128×128, wbudowywane w bibliotekę przez `build.rs`. Źródła w kolejności:
  manifest PWA, apple-touch-icon, usługa ikon Google, DuckDuckGo, favicon, na końcu znak marki z Simple Icons.
  Jakość: źródło ≥96 px = ostra ikona (rogi zaokrąglone, gdy jest pełne tło); 48–95 px = ikona na jasnym
  kafelku, bez rozciągania ponad 1,5×; poniżej 48 px (rozmyłaby się) = litera na kolorze wziętym z ikony.
  Brak źródła = litera rysowana przez aplikację.
- `data/VERIFY.md`, `data/verify-report.json`: wynik ostatniego sprawdzenia adresów
  (`tools/verify.py`, uruchamia go workflow **Web Apps** przy każdej zmianie).

## Jak działa „Utwórz”

`launcher::create` zapisuje `~/.local/share/applications/shrimp-webapp-<id>.desktop` i ikonę
w `~/.local/share/shrimp/webapps/`. GNOME od razu pokazuje aplikację w siatce. Okno otwiera
przeglądarka z rodziny Chromium w trybie aplikacji (`--app=URL`, kolejność: Chromium, Chrome,
Brave, Edge, Vivaldi; najpierw z Flathuba, potem z systemu). Bez takiej przeglądarki adres
otwiera domyślna przeglądarka (`xdg-open`), w zwykłej karcie. „Utwórz własną” robi to samo
dla dowolnego adresu.

Ograniczenia: Firefox i GNOME Web nie mają trybu `--app`; z piaskownicy Flatpak tworzenie
zwraca błąd (wtedy potrzebny byłby portal `org.freedesktop.portal.DynamicLauncher`).

## Uruchomienie

```sh
cargo test
cargo run --example demo            # podgląd strony w oknie
```
