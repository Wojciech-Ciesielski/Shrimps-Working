# Shrimp Center

Systemy na komputerze i wygląd pulpitu. Zastępuje menu GRUB-a (w Shrimp jest systemd-boot bez menu).
GTK4 + libadwaita (Rust), wyglądem jak Cocktail i Panko.

## Co działa

- **Systemy**: „Uruchom ponownie w Windows” (BootNext przez `efibootmgr`, tylko przy następnym
  starcie), „Ustawienia UEFI” (`systemctl reboot --firmware-setup`) i domyślny system (pierwszy
  w BootOrder). Zmiany UEFI robi `pkexec /usr/bin/shrimp-center --boot-next|--default ID`
  (reguła `org.shrimp.center.boot`); program przyjmuje tylko ID systemu z listy `efibootmgr`.
- **Wygląd** (GSettings, zmiany od razu): styl jasny/ciemny, kolor akcentu (GNOME 47+), animacje,
  wielkość tekstu, skrót do skalowania ekranu, dok (położenie, wielkość ikon, ukrywanie, kosz),
  przyciski okna, czcionki. „Przywróć domyślne” wraca do wartości Shrimp (99_shrimp.gschema.override).
- Przycisk panelu bocznego, „O Shrimp Center” otwiera stronę programu (`help-links`, temat `center`).

## Uruchomienie

```sh
cd boxes/shrimp-center
cargo run -- --demo                  # przykładowe wpisy UEFI, ustawienia w pamięci
SHRIMP_CENTER_PAGE=wyglad cargo run -- --demo
cargo test
boxes/shrimp-center/install.sh DESTDIR
```
