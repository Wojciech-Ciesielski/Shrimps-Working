# Wtyczki zsh dla Cherry

Leap 16.1 nie ma pakietów `zsh-autosuggestions` ani `zsh-syntax-highlighting`, więc Cherry ma ich kopie
(install.sh → `/usr/share/cherry/zsh-plugins/`). Pakiet systemowy, jeśli się pojawi, ma pierwszeństwo
(kolejność w src/store.rs).

| Wtyczka | Wersja | Źródło | Licencja |
|---|---|---|---|
| zsh-autosuggestions | 0.7.1 | https://github.com/zsh-users/zsh-autosuggestions | MIT (`LICENSE`) |
| zsh-syntax-highlighting | 0.8.0 | https://github.com/zsh-users/zsh-syntax-highlighting | BSD-3-Clause (`COPYING.md`) |

Skopiowane bez zmian: główny plik `.zsh`, licencja, plik wersji i (dla podświetlania) katalog
`highlighters/` bez testów.
