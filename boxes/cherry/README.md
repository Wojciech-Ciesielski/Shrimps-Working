# Cherry

Terminal Shrimp z zsh. GTK4 + libadwaita + VTE, Rust.

- Pionowy pasek po lewej: pomoc u góry, lista kart (folder i gałąź gita), na dole nowa karta (+) po lewej
  i **Historia** po prawej. Ustawienia i przyciski okna są w prawym górnym rogu, nad terminalem.
- **Skróty jak w Windows**: okno (`src/ui.rs`, `key_action`) kopiuje Ctrl+C przy zaznaczeniu i wkleja Ctrl+V,
  menu pod prawym przyciskiem; edycję komendy (Shift+strzałki, Ctrl+A/X/Z/Y, Ctrl+Backspace) robi `.zshrc`
  Cherry (`src/store.rs`), więc programy w terminalu dostają klawisze bez zmian.
- **Historia**: zsh dopisuje wykonane komendy do `commands.log` (hak `zshaddhistory`), okno je zbiera;
  usuwanie po 1, 7 albo 30 dniach (ustawienia).
- **Narzędzia** (`data/tools.txt`): instalowane miękko w obrazie (`configure.sh`) i w zainstalowanym
  systemie (post-install w CoreOS). `.zshrc` Cherry włącza starship, zoxide, fzf, atuin, eza, yazi,
  zsh-autosuggestions i zsh-syntax-highlighting, jeśli są; `fastfetch` z logo Shrimp (`data/fastfetch*`).

Dane: `~/.local/share/cherry/cherry.json` (historia, ustawienia). Cherry uruchamia zsh z własnym `ZDOTDIR`
(`~/.config/cherry/zsh/`), który najpierw wczytuje `~/.zshrc` użytkownika.

```
cargo run              # okno
cargo test             # testy (po polsku)
./install.sh DESTDIR   # instalacja do obrazu
```
