# Cherry

Terminal Shrimp z zsh, który tłumaczy komendy. GTK4 + libadwaita + VTE, Rust.

- **Jak to działa?**: rozbiór wpisywanej komendy na części z prostym opisem (`src/explain.rs`).
- **Notatki**: zapisane komendy z opisem, klik wpisuje komendę do terminala.
- **Skróty**: klawisze (np. Alt+L) i skróty w zapisie (aliasy zsh, np. `gs`).
- **Historia**: z wyszukiwaniem; usuwanie po 1, 7 albo 30 dniach (ustawienia).
- Karty u góry albo pionowo po prawej, pasek stanu: folder, gałąź gita, użytkownik.

Dane: `~/.local/share/cherry/cherry.json`. Cherry uruchamia zsh z własnym `ZDOTDIR`
(`~/.config/cherry/zsh/`), który najpierw wczytuje `~/.zshrc` użytkownika.

```
cargo run              # okno
cargo test             # testy (po polsku)
./install.sh DESTDIR   # instalacja do obrazu
```
