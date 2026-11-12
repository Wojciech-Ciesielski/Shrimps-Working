//! Dane Cherry: notatki, skróty, historia i ustawienia w jednym pliku JSON
//! (`~/.local/share/cherry/cherry.json`) oraz pliki startowe zsh (`~/.config/cherry/zsh/`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub cmd: String,
    pub text: String,
}

/// Skrót klawiszowy (akcelerator GTK, np. `<Alt>g`) wpisujący komendę.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyShortcut {
    pub accel: String,
    pub cmd: String,
}

/// Skrót w zapisie: alias zsh (`gs` → `git status`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Abbrev {
    pub abbr: String,
    pub cmd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistEntry {
    pub cmd: String,
    /// Sekundy od 1970-01-01 (UTC).
    pub at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TabsPos {
    #[default]
    Top,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Settings {
    pub tabs: TabsPos,
    /// Po ilu dniach usuwać historię; `None` = nigdy.
    pub history_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Data {
    pub notes: Vec<Note>,
    pub keys: Vec<KeyShortcut>,
    pub abbrevs: Vec<Abbrev>,
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
            abbrevs: vec![Abbrev {
                abbr: "gs".into(),
                cmd: "git status".into(),
            }],
            keys: vec![KeyShortcut {
                accel: "<Alt>l".into(),
                cmd: "ls -la".into(),
            }],
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

/// Alias zsh w bezpiecznym cytowaniu: `alias -- 'gs'='git status'`. Nazwa tylko z bezpiecznych znaków.
pub fn alias_line(a: &Abbrev) -> Option<String> {
    let ok = !a.abbr.is_empty()
        && a.abbr
            .chars()
            .all(|c| c.is_alphanumeric() || "_-.".contains(c));
    if !ok || a.cmd.trim().is_empty() {
        return None;
    }
    Some(format!(
        "alias -- '{}'='{}'",
        a.abbr,
        a.cmd.replace('\'', "'\\''")
    ))
}

/// Katalog ZDOTDIR Cherry: `.zshenv` i `.zshrc` wczytują pliki użytkownika, potem dokładają
/// aliasy, znak zachęty i zgłaszanie bieżącego folderu (OSC 7) dla paska stanu.
pub fn write_zsh(d: &Data) -> PathBuf {
    let dir = gtk::glib::user_config_dir().join("cherry").join("zsh");
    let _ = std::fs::create_dir_all(&dir);
    let aliases: Vec<String> = d.abbrevs.iter().filter_map(alias_line).collect();
    let _ = std::fs::write(dir.join("aliases.zsh"), aliases.join("\n") + "\n");
    let _ = std::fs::write(dir.join(".zshenv"), ZSHENV);
    let _ = std::fs::write(dir.join(".zshrc"), ZSHRC);
    dir
}

const ZSHENV: &str = r#"# Cherry: najpierw ustawienia użytkownika
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"
"#;

const ZSHRC: &str = r#"# Cherry: plik użytkownika, potem dodatki Cherry (aliasy, znak zachęty, bieżący folder)
CHERRY_DIR="$ZDOTDIR"
ZDOTDIR="$HOME"
[[ -f "$HOME/.zshrc" ]] && source "$HOME/.zshrc"
: ${HISTFILE:=$HOME/.zsh_history}
: ${HISTSIZE:=5000}
: ${SAVEHIST:=5000}
setopt INC_APPEND_HISTORY 2>/dev/null
[[ -f "$CHERRY_DIR/aliases.zsh" ]] && source "$CHERRY_DIR/aliases.zsh"
autoload -Uz compinit && compinit -u 2>/dev/null
PROMPT='%F{blue}%~%f %F{magenta}❯%f '
_cherry_osc7() { printf '\e]7;file://%s%s\e\\' "${HOST}" "${PWD// /%20}" }
autoload -Uz add-zsh-hook
add-zsh-hook chpwd _cherry_osc7
add-zsh-hook precmd _cherry_osc7
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
    fn alias_quoting() {
        let a = Abbrev {
            abbr: "say".into(),
            cmd: "echo 'hi'".into(),
        };
        assert_eq!(
            alias_line(&a).unwrap(),
            r#"alias -- 'say'='echo '\''hi'\'''"#
        );
        assert!(alias_line(&Abbrev {
            abbr: "a;b".into(),
            cmd: "x".into()
        })
        .is_none());
    }

    #[test]
    fn json_roundtrip_and_defaults() {
        let d: Data = serde_json::from_str(r#"{"notes":[{"cmd":"ls","text":"x"}]}"#).unwrap();
        assert_eq!(d.settings.tabs, TabsPos::Top);
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
