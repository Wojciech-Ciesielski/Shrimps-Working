# Filer

Menedżer plików Shrimp. Odwzorowuje funkcje i układ Nemo jeden do jednego, ale jest napisany od nowa
w Rust na GTK4 + libadwaita. Ikony bierze z motywu, w Shrimp z MoreWaity.

Dlaczego nie port kodu Nemo: Nemo to GTK3 w C. Przeniesienie go na GTK4 oznacza przepisanie widoków
(GtkIconView/GtkTreeView nie istnieją w GTK4 tak samo), panelu bocznego, okien i rozszerzeń.
Własny kod w Rust wychodzi taniej i pasuje do reszty aplikacji Shrimps (Cocktail, Panko, Cherry).

## Etap 1 (Beta 5): rdzeń

| Nemo | Filer |
|---|---|
| Widok ikon, listy i kompaktowy (Ctrl+1/2/3), powiększenie (Ctrl +/−/0, Ctrl + kółko, suwak) | ✓ |
| Kolumny listy: nazwa, rozmiar, typ, data modyfikacji, dostęp, właściciel, uprawnienia, położenie (menu nagłówka) | ✓ |
| Sortowanie (nazwa, rozmiar, typ, data), odwrotna kolejność, foldery najpierw | ✓ |
| Karty (Ctrl+T, Ctrl+W, środkowy przycisk), dodatkowy panel (F3) | ✓ |
| Pasek ścieżki z przyciskami i pole adresu (Ctrl+L) | ✓ |
| Panel boczny: Mój komputer, zakładki, urządzenia z zajętością i wysuwaniem, sieć (F9) | ✓ |
| Pasek menu (Plik, Edycja, Widok, Przejdź, Zakładki, Pomoc), pasek stanu | ✓ |
| Kopiowanie, przenoszenie, dowiązania, kosz, usuwanie, przywracanie: w tle, z oknem postępu | ✓ |
| Konflikty: zastąp, scal, pomiń, zachowaj oba, „dla wszystkich” | ✓ |
| Cofnij (Ctrl+Z) | ✓ |
| Schowek zgodny z GNOME (Wytnij/Kopiuj/Wklej, Wklej do folderu), przeciąganie i upuszczanie | ✓ |
| Zmiana nazwy w miejscu (F2), nowy folder, nowy dokument z Szablonów | ✓ |
| Otwórz za pomocą, Inny program…, Kopiuj do / Przenieś do (w tym drugi panel) | ✓ |
| Zaznacz wszystko, odwróć, według wzorca (Ctrl+S), szukanie przez pisanie | ✓ |
| Szukanie w folderze i podfolderach (Ctrl+F) | ✓ |
| Właściwości: podstawowe, uprawnienia, otwórz za pomocą | ✓ |
| Kosz: pasek z „Opróżnij kosz”, przywracanie | ✓ |
| Miniatury freedesktop (obrazy, wideo i PDF przez /usr/share/thumbnailers) | ✓ |
| Skrypty z ~/.local/share/nemo/scripts (zmienne NEMO_SCRIPT_*) | ✓ |
| Otwórz w terminalu (Cherry), Połącz z serwerem (smb, sftp, ftp przez GVfs) | ✓ |
| Zakładki wspólne z GTK (~/.config/gtk-3.0/bookmarks), edycja zakładek | ✓ |
| Preferencje: widoki, zachowanie, wyświetlanie, podgląd | ✓ |

## Etap 2

- Akcje Nemo (`.nemo_action` z ~/.local/share/nemo/actions), menu „Kompresuj / Rozpakuj”
- Ponów (Ctrl+Y), historia przejść w menu przycisków Wstecz/Dalej
- Drzewo folderów w panelu bocznym, emblematy, notatki
- Przenoszenie kart między panelami, „Otwórz jako administrator” (jeśli zostanie w Shrimp)
- Integracja z CoreOS: domyślny menedżer plików w obrazie (`inode/directory`)

nemo-python i pulpit (nemo-desktop) nie będą przenoszone: GNOME w Shrimp nie ma ikon na pulpicie.

## Budowa

```sh
cargo build --release          # wymaga gtk4 4.14+ i libadwaita 1.5+
./install.sh /tmp/root          # instalacja do katalogu
```

Ustawienia: `~/.config/shrimp/filer.json`. Teksty: `i18n/` (wspólny format Shrimp).
