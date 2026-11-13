//! Dane Cherry: notatki, historia i ustawienia w jednym pliku JSON
//! (`~/.local/share/cherry/cherry.json`) oraz pliki startowe zsh (`~/.config/cherry/zsh/`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub cmd: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistEntry {
    pub cmd: String,
    /// Sekundy od 1970-01-01 (UTC).
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Settings {
    /// Po ilu dniach usuwać historię; `None` = nigdy.
    pub history_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Data {
    pub notes: Vec<Note>,
    pub history: Vec<HistEntry>,
    pub settings: Settings,
}

/// Wybór czasu przechowywania historii w ustawieniach.
pub const HISTORY_CHOICES: [Option<u32>; 4] = [None, Some(1), Some(7), Some(30)];
const HISTORY_MAX: usize = 1000;

impl Data {
    /// Startowe notatki, żeby panel nie był pusty przy pierwszym uruchomieniu.
    fn first_run() -> Data {
        let n = |c: &str, t: &'static str| Note {
            cmd: c.into(),
            text: crate::i18n::tr(t).into(),
        };
        Data {
            notes: vec![
                n("sudo zypper refresh", "note.refresh"),
                n("ls -la", "note.lsla"),
                n("cd ..", "note.cdup"),
            ],
            ..Data::default()
        }
    }

    /// Dopisuje komendę do historii (bez powtórzenia ostatniej) i przycina ją.
    pub fn record(&mut self, cmd: &str, now: i64) {
        let cmd = cmd.trim();
        if cmd.is_empty() || self.history.first().is_some_and(|h| h.cmd == cmd) {
            return;
        }
        self.history.insert(
            0,
            HistEntry {
                cmd: cmd.into(),
                at: now,
            },
        );
        self.prune(now);
    }

    /// Wpisy z pliku komend zsh: `sekundy⇥komenda` w linii (starsze najpierw). Linia bez czasu dostaje `now`.
    pub fn ingest(&mut self, text: &str, now: i64) {
        for line in text.lines() {
            let (at, cmd) = match line.split_once('\t') {
                Some((t, c)) => (t.trim().parse().unwrap_or(now), c),
                None => (now, line),
            };
            self.record(cmd, at);
        }
        self.prune(now);
    }

    /// Usuwa wpisy starsze niż ustawienie i nadmiar ponad limit.
    pub fn prune(&mut self, now: i64) {
        if let Some(d) = self.settings.history_days {
            let limit = now - i64::from(d) * 86_400;
            self.history.retain(|h| h.at >= limit);
        }
        self.history.truncate(HISTORY_MAX);
    }
}

fn data_file() -> PathBuf {
    gtk::glib::user_data_dir()
        .join("cherry")
        .join("cherry.json")
}

/// Plik, do którego zsh w Cherry dopisuje wykonane komendy (zmienna CHERRY_HISTORY).
pub fn commands_file() -> PathBuf {
    let p = data_file().with_file_name("commands.log");
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    p
}

/// Zabiera zawartość pliku komend (przeniesienie, potem odczyt: zsh w tym czasie zacznie nowy plik).
pub fn take_commands() -> String {
    let p = commands_file();
    let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::rename(&p, &tmp).is_err() {
        return String::new();
    }
    let s = std::fs::read_to_string(&tmp).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);
    s
}

pub fn load() -> Data {
    load_from(&data_file())
}

pub fn load_from(p: &Path) -> Data {
    match std::fs::read_to_string(p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Data::first_run(),
    }
}

pub fn save(d: &Data) {
    let p = data_file();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(d) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }
}

/// Katalog ZDOTDIR Cherry: `.zshenv` i `.zshrc` wczytują pliki użytkownika, potem dokładają narzędzia
/// (starship, zoxide, fzf, atuin, eza, yazi), skróty jak w Windows, wtyczki zsh i zgłaszanie folderu (OSC 7).
pub fn write_zsh() -> PathBuf {
    let dir = gtk::glib::user_config_dir().join("cherry").join("zsh");
    let _ = std::fs::create_dir_all(&dir);
    // Skróty w zapisie (aliasy) z poprzednich wersji Cherry nie są już wczytywane.
    let _ = std::fs::remove_file(dir.join("aliases.zsh"));
    let _ = std::fs::write(dir.join(".zshenv"), ZSHENV);
    let _ = std::fs::write(dir.join(".zshrc"), ZSHRC.replace("@KEYS@", KEYS));
    let _ = std::fs::write(dir.join("starship.toml"), STARSHIP);
    dir
}

const ZSHENV: &str = r#"# Cherry: najpierw ustawienia użytkownika
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"
"#;

const ZSHRC: &str = r#"# Cherry: plik użytkownika, potem dodatki Cherry
CHERRY_DIR="$ZDOTDIR"
ZDOTDIR="$HOME"
[[ -f "$HOME/.zshrc" ]] && source "$HOME/.zshrc"
: ${HISTFILE:=$HOME/.zsh_history}
: ${HISTSIZE:=5000}
: ${SAVEHIST:=5000}
setopt INC_APPEND_HISTORY HIST_IGNORE_DUPS 2>/dev/null
autoload -Uz compinit && compinit -u 2>/dev/null
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list 'm:{a-z}={A-Za-z}'

# Znak zachęty: starship albo prosty folder i ❯.
if (( $+commands[starship] )); then
  [[ -f "${XDG_CONFIG_HOME:-$HOME/.config}/starship.toml" ]] || export STARSHIP_CONFIG="$CHERRY_DIR/starship.toml"
  eval "$(starship init zsh)"
else
  PROMPT='%F{blue}%~%f %F{magenta}❯%f '
fi

# Narzędzia, jeśli są zainstalowane: z (zoxide), Ctrl+T i Alt+C (fzf), Ctrl+R (atuin), ls (eza), y (yazi).
(( $+commands[zoxide] )) && eval "$(zoxide init zsh)"
if (( $+commands[fzf] )); then
  if fzf --zsh >/dev/null 2>&1; then
    source <(fzf --zsh)
  else
    for f in /usr/share/fzf/shell/key-bindings.zsh /usr/share/fzf/key-bindings.zsh; do
      [[ -f $f ]] && { source $f; break }
    done
  fi
fi
(( $+commands[atuin] )) && eval "$(atuin init zsh --disable-up-arrow)"
if (( $+commands[eza] )); then
  alias ls='eza --group-directories-first'
  alias ll='eza -l --group-directories-first --git'
  alias la='eza -la --group-directories-first --git'
fi
(( $+commands[micro] )) && export EDITOR=${EDITOR:-micro}
if (( $+commands[yazi] )); then
  y() {
    local tmp cwd
    tmp=$(mktemp -t yazi-cwd.XXXXXX)
    yazi "$@" --cwd-file="$tmp"
    IFS= read -r -d '' cwd < "$tmp"
    [[ -n $cwd && $cwd != $PWD ]] && builtin cd -- "$cwd"
    rm -f -- "$tmp"
  }
fi
if (( $+commands[fastfetch] )) && [[ -f /usr/share/cherry/fastfetch.jsonc && ! -f ${XDG_CONFIG_HOME:-$HOME/.config}/fastfetch/config.jsonc ]]; then
  alias fastfetch='fastfetch -c /usr/share/cherry/fastfetch.jsonc'
fi

# Skróty jak w Windows przy wpisywaniu komendy (programy uruchomione w terminalu dostają klawisze bez zmian):
# Shift+strzałki/Home/End zaznaczają, pisanie, wklejanie, Backspace i Delete zastępują zaznaczenie,
# Ctrl+A zaznacza całość, Ctrl+C kopiuje zaznaczenie (bez zaznaczenia przerywa jak zwykle), Ctrl+X wycina,
# Ctrl+Z cofa, Ctrl+Y ponawia, Ctrl+strzałki skaczą o słowo, Ctrl+Backspace i Ctrl+Delete usuwają słowo.
# Ctrl+V wkleja okno Cherry.
@KEYS@
# Wtyczki zsh (pakiety openSUSE i inne typowe miejsca); podświetlanie składni musi być ostatnie.
for f in /usr/share/{zsh/plugins/,zsh-plugins/,}zsh-autosuggestions/zsh-autosuggestions.zsh(N); do
  source $f
  ZSH_AUTOSUGGEST_ACCEPT_WIDGETS+=(cherry-move-forward-char cherry-move-end-of-line)
  ZSH_AUTOSUGGEST_PARTIAL_ACCEPT_WIDGETS+=(cherry-move-forward-word)
  break
done
for f in /usr/share/{zsh/plugins/,zsh-plugins/,}zsh-syntax-highlighting/zsh-syntax-highlighting.zsh(N); do
  source $f
  break
done

# Historia Cherry: zsh dopisuje każdą wykonaną komendę (czas⇥komenda), okno ją pokazuje.
zmodload zsh/datetime 2>/dev/null
_cherry_history() {
  local cmd=${1%%$'\n'}
  [[ -n $CHERRY_HISTORY && -n ${cmd//[[:space:]]/} ]] && print -r -- "${EPOCHSECONDS:-0}"$'\t'"${cmd//$'\n'/ }" >> "$CHERRY_HISTORY"
  return 0
}
_cherry_osc7() { printf '\e]7;file://%s%s\e\\' "${HOST}" "${PWD// /%20}" }
autoload -Uz add-zsh-hook
add-zsh-hook zshaddhistory _cherry_history
add-zsh-hook chpwd _cherry_osc7
add-zsh-hook precmd _cherry_osc7
"#;

const KEYS: &str = r#"_cherry_clip() { (( $+commands[wl-copy] )) && print -rn -- "$1" | wl-copy 2>/dev/null }
_cherry_kill() { (( REGION_ACTIVE )) || return 1; zle .kill-region; REGION_ACTIVE=0 }
_cherry_select() { (( REGION_ACTIVE )) || { MARK=$CURSOR; REGION_ACTIVE=1 }; zle .${WIDGET#cherry-select-} }
_cherry_move() { REGION_ACTIVE=0; zle .${WIDGET#cherry-move-} }
_cherry_erase() { _cherry_kill || zle .${WIDGET#cherry-erase-} }
_cherry_insert() { _cherry_kill; zle .self-insert }
_cherry_paste() { _cherry_kill; zle .bracketed-paste }
_cherry_all() { MARK=0; CURSOR=$#BUFFER; REGION_ACTIVE=1 }
_cherry_cut() { (( REGION_ACTIVE )) || return 0; zle .copy-region-as-kill; _cherry_clip "$CUTBUFFER"; _cherry_kill }
_cherry_copy() { (( REGION_ACTIVE )) || return 1; zle .copy-region-as-kill; _cherry_clip "$CUTBUFFER"; REGION_ACTIVE=0 }
zle -N self-insert _cherry_insert
zle -N bracketed-paste _cherry_paste
zle -N cherry-select-all _cherry_all
zle -N cherry-cut _cherry_cut
bindkey -rp '^X'
bindkey '^A' cherry-select-all '^X' cherry-cut '^Z' undo '^Y' redo
() {
  local k w
  for k w in \
    '^[[1;2D' select-backward-char '^[[1;2C' select-forward-char \
    '^[[1;2H' select-beginning-of-line '^[[1;2F' select-end-of-line \
    '^[[1;6D' select-backward-word '^[[1;6C' select-forward-word \
    '^[[D' move-backward-char '^[OD' move-backward-char '^[[C' move-forward-char '^[OC' move-forward-char \
    '^[[H' move-beginning-of-line '^[OH' move-beginning-of-line '^[[1~' move-beginning-of-line \
    '^[[F' move-end-of-line '^[OF' move-end-of-line '^[[4~' move-end-of-line \
    '^[[1;5D' move-backward-word '^[[1;5C' move-forward-word \
    '^?' erase-backward-delete-char '^[[3~' erase-delete-char \
    '^H' erase-backward-kill-word '^[[3;5~' erase-kill-word
  do
    zle -N cherry-$w _cherry_${w%%-*}
    bindkey -- $k cherry-$w
  done
}
# Ctrl+C przy zaznaczonym tekście komendy kopiuje go zamiast przerywać.
TRAPINT() {
  if zle && _cherry_copy 2>/dev/null; then zle -R; return 0; fi
  return $(( 128 + $1 ))
}"#;

/// starship w Cherry: jedna linia (folder, gałąź i stan gita, czas długiej komendy, ❯).
/// Własny `~/.config/starship.toml` ma pierwszeństwo.
const STARSHIP: &str = r#"add_newline = false
format = "$directory$git_branch$git_status$container$cmd_duration$character"

[directory]
style = "bold blue"
truncation_length = 4

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""

[cmd_duration]
format = "[$duration]($style) "

[character]
success_symbol = "[❯](bold purple)"
error_symbol = "[❯](bold red)"
"#;

/// Gałąź gita dla folderu: szuka `.git/HEAD` w górę drzewa.
pub fn git_branch(dir: &Path) -> Option<String> {
    let mut d = Some(dir);
    while let Some(p) = d {
        let head = p.join(".git").join("HEAD");
        if let Ok(s) = std::fs::read_to_string(&head) {
            let s = s.trim();
            return Some(match s.strip_prefix("ref: refs/heads/") {
                Some(b) => b.to_owned(),
                None => s.chars().take(7).collect(),
            });
        }
        d = p.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_dedup_and_retention() {
        let mut d = Data::default();
        d.record("ls", 1000);
        d.record("ls", 1001);
        d.record("pwd", 90_000);
        assert_eq!(d.history.len(), 2);
        d.settings.history_days = Some(1);
        d.prune(1000 + 86_400 + 10);
        assert_eq!(
            d.history.iter().map(|h| h.cmd.as_str()).collect::<Vec<_>>(),
            vec!["pwd"]
        );
    }

    #[test]
    fn commands_from_zsh() {
        let mut d = Data::default();
        d.ingest("100\tls -la\n101\tls -la\n102\tcd /etc\nbez czasu\n", 200);
        let cmds: Vec<(&str, i64)> = d.history.iter().map(|h| (h.cmd.as_str(), h.at)).collect();
        assert_eq!(cmds, vec![("bez czasu", 200), ("cd /etc", 102), ("ls -la", 100)]);
    }

    #[test]
    fn json_roundtrip_and_defaults() {
        // Pola starszych wersji (skróty, położenie kart) są pomijane.
        let d: Data = serde_json::from_str(
            r#"{"notes":[{"cmd":"ls","text":"x"}],"keys":[{"accel":"<Alt>l","cmd":"ls"}],"settings":{"tabs":"right"}}"#,
        )
        .unwrap();
        assert_eq!(d.settings.history_days, None);
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Data>(&s).unwrap(), d);
    }

    #[test]
    fn branch_from_head() {
        let t = std::env::temp_dir().join(format!("cherry-git-{}", std::process::id()));
        std::fs::create_dir_all(t.join(".git")).unwrap();
        std::fs::create_dir_all(t.join("a/b")).unwrap();
        std::fs::write(t.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(git_branch(&t.join("a/b")).as_deref(), Some("main"));
        let _ = std::fs::remove_dir_all(t);
    }
}
