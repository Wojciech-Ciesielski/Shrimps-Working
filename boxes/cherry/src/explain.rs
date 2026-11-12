//! „Jak to działa?”: rozbiór komendy na części z prostym opisem każdej z nich.
//! Logika bez GTK (testowalna); opisy są kluczami tłumaczeń z `i18n/`.

/// Rodzaj części: zwykły opis, ostrzeżenie (np. `rm -rf`, `sudo`) albo podpowiedź programu Shrimp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Warn,
    Hint,
}

/// Jedna część komendy: token z wiersza i klucz opisu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub tok: String,
    pub key: &'static str,
    pub kind: Kind,
}

/// Opis komendy: klucz krótkiego opisu, opisy liter opcji, podkomend i argumentu.
pub struct Cmd {
    pub name: &'static str,
    pub key: &'static str,
    pub flags: &'static [(char, &'static str)],
    pub long: &'static [(&'static str, &'static str)],
    pub subs: &'static [(&'static str, &'static str)],
    pub arg: &'static str,
}

const ARG: &str = "arg.generic";

macro_rules! cmd {
    ($n:literal, $k:literal) => {
        Cmd {
            name: $n,
            key: $k,
            flags: &[],
            long: &[],
            subs: &[],
            arg: ARG,
        }
    };
    ($n:literal, $k:literal, flags: $f:expr) => {
        Cmd {
            name: $n,
            key: $k,
            flags: $f,
            long: &[],
            subs: &[],
            arg: ARG,
        }
    };
    ($n:literal, $k:literal, flags: $f:expr, arg: $a:literal) => {
        Cmd {
            name: $n,
            key: $k,
            flags: $f,
            long: &[],
            subs: &[],
            arg: $a,
        }
    };
    ($n:literal, $k:literal, subs: $s:expr) => {
        Cmd {
            name: $n,
            key: $k,
            flags: &[],
            long: &[],
            subs: $s,
            arg: ARG,
        }
    };
    ($n:literal, $k:literal, arg: $a:literal) => {
        Cmd {
            name: $n,
            key: $k,
            flags: &[],
            long: &[],
            subs: &[],
            arg: $a,
        }
    };
}

/// Znane komendy. Kolejność nie ma znaczenia; nazwa to pierwszy token (po `sudo`).
pub static COMMANDS: &[Cmd] = &[
    cmd!("ls", "cmd.ls", flags: &[('l', "flag.ls.l"), ('a', "flag.ls.a"), ('h', "flag.ls.h"), ('t', "flag.ls.t"), ('R', "flag.ls.R")], arg: "arg.path"),
    cmd!("cd", "cmd.cd", arg: "arg.dir"),
    cmd!("pwd", "cmd.pwd"),
    cmd!("cp", "cmd.cp", flags: &[('r', "flag.recursive"), ('R', "flag.recursive"), ('i', "flag.ask"), ('v', "flag.verbose")], arg: "arg.path"),
    cmd!("mv", "cmd.mv", flags: &[('i', "flag.ask"), ('v', "flag.verbose"), ('f', "flag.force")], arg: "arg.path"),
    cmd!("rm", "cmd.rm", flags: &[('r', "flag.recursive"), ('R', "flag.recursive"), ('f', "flag.force"), ('i', "flag.ask"), ('v', "flag.verbose")], arg: "arg.path"),
    cmd!("mkdir", "cmd.mkdir", flags: &[('p', "flag.mkdir.p")], arg: "arg.dir"),
    cmd!("rmdir", "cmd.rmdir", arg: "arg.dir"),
    cmd!("touch", "cmd.touch", arg: "arg.file"),
    cmd!("cat", "cmd.cat", arg: "arg.file"),
    cmd!("less", "cmd.less", arg: "arg.file"),
    cmd!("head", "cmd.head", flags: &[('n', "flag.lines")], arg: "arg.file"),
    cmd!("tail", "cmd.tail", flags: &[('n', "flag.lines"), ('f', "flag.tail.f")], arg: "arg.file"),
    cmd!("nano", "cmd.nano", arg: "arg.file"),
    cmd!("grep", "cmd.grep", flags: &[('i', "flag.grep.i"), ('r', "flag.recursive"), ('n', "flag.grep.n"), ('v', "flag.grep.v")], arg: "arg.pattern"),
    cmd!("find", "cmd.find", arg: "arg.path"),
    cmd!("echo", "cmd.echo", arg: "arg.text"),
    cmd!("man", "cmd.man", arg: "arg.cmd"),
    cmd!("which", "cmd.which", arg: "arg.cmd"),
    cmd!("whoami", "cmd.whoami"),
    cmd!("uname", "cmd.uname", flags: &[('a', "flag.uname.a"), ('r', "flag.uname.r")]),
    cmd!("clear", "cmd.clear"),
    cmd!("exit", "cmd.exit"),
    cmd!("history", "cmd.history"),
    cmd!("chmod", "cmd.chmod", flags: &[('R', "flag.recursive")], arg: "arg.mode"),
    cmd!("chown", "cmd.chown", flags: &[('R', "flag.recursive")], arg: "arg.owner"),
    cmd!("df", "cmd.df", flags: &[('h', "flag.human")]),
    cmd!("du", "cmd.du", flags: &[('h', "flag.human"), ('s', "flag.du.s")], arg: "arg.path"),
    cmd!("free", "cmd.free", flags: &[('h', "flag.human")]),
    cmd!("top", "cmd.top"),
    cmd!("htop", "cmd.top"),
    cmd!("ps", "cmd.ps", flags: &[('a', "flag.ps.a"), ('u', "flag.ps.u"), ('x', "flag.ps.x")]),
    cmd!("kill", "cmd.kill", flags: &[('9', "flag.kill.9")], arg: "arg.pid"),
    cmd!("tar", "cmd.tar", flags: &[('x', "flag.tar.x"), ('c', "flag.tar.c"), ('z', "flag.tar.z"), ('v', "flag.verbose"), ('f', "flag.tar.f")], arg: "arg.file"),
    cmd!("unzip", "cmd.unzip", arg: "arg.file"),
    cmd!("curl", "cmd.curl", flags: &[('L', "flag.curl.L"), ('O', "flag.curl.O"), ('o', "flag.curl.o")], arg: "arg.url"),
    cmd!("wget", "cmd.wget", arg: "arg.url"),
    cmd!("ping", "cmd.ping", flags: &[('c', "flag.ping.c")], arg: "arg.host"),
    cmd!("ssh", "cmd.ssh", arg: "arg.host"),
    cmd!("ip", "cmd.ip", subs: &[("a", "sub.ip.addr"), ("addr", "sub.ip.addr"), ("link", "sub.ip.link"), ("route", "sub.ip.route")]),
    cmd!("journalctl", "cmd.journalctl", flags: &[('b', "flag.journal.b"), ('f', "flag.tail.f"), ('u', "flag.journal.u")]),
    cmd!("systemctl", "cmd.systemctl", subs: &[
        ("start", "sub.systemctl.start"), ("stop", "sub.systemctl.stop"), ("restart", "sub.systemctl.restart"),
        ("status", "sub.systemctl.status"), ("enable", "sub.systemctl.enable"), ("disable", "sub.systemctl.disable"),
        ("reboot", "sub.systemctl.reboot"), ("poweroff", "sub.systemctl.poweroff"),
    ]),
    cmd!("zypper", "cmd.zypper", subs: &[
        ("refresh", "sub.zypper.refresh"), ("ref", "sub.zypper.refresh"), ("update", "sub.zypper.update"), ("up", "sub.zypper.update"),
        ("install", "sub.zypper.install"), ("in", "sub.zypper.install"), ("remove", "sub.zypper.remove"), ("rm", "sub.zypper.remove"),
        ("search", "sub.zypper.search"), ("se", "sub.zypper.search"), ("dup", "sub.zypper.dup"), ("info", "sub.zypper.info"),
    ]),
    cmd!("transactional-update", "cmd.tu", subs: &[("up", "sub.tu.up"), ("pkg", "sub.tu.pkg"), ("rollback", "sub.tu.rollback")]),
    cmd!("flatpak", "cmd.flatpak", subs: &[
        ("install", "sub.flatpak.install"), ("update", "sub.flatpak.update"), ("uninstall", "sub.flatpak.uninstall"),
        ("list", "sub.flatpak.list"), ("run", "sub.flatpak.run"), ("search", "sub.flatpak.search"),
    ]),
    cmd!("snapper", "cmd.snapper", subs: &[("list", "sub.snapper.list"), ("create", "sub.snapper.create"), ("undochange", "sub.snapper.undochange")]),
    cmd!("git", "cmd.git", subs: &[
        ("status", "sub.git.status"), ("add", "sub.git.add"), ("commit", "sub.git.commit"), ("push", "sub.git.push"),
        ("pull", "sub.git.pull"), ("clone", "sub.git.clone"), ("log", "sub.git.log"), ("diff", "sub.git.diff"),
        ("checkout", "sub.git.checkout"), ("switch", "sub.git.checkout"), ("branch", "sub.git.branch"), ("init", "sub.git.init"),
    ]),
];

/// Długie opcje wspólne dla wielu komend.
const COMMON_LONG: &[(&str, &str)] = &[("--help", "long.help"), ("--version", "long.version")];

pub fn find(name: &str) -> Option<&'static Cmd> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// Nazwy znanych komend: podpowiedzi przy tworzeniu notatki.
pub fn names() -> impl Iterator<Item = &'static str> {
    COMMANDS.iter().map(|c| c.name)
}

/// Dzieli wiersz na tokeny jak powłoka w prostym przypadku: spacje, cudzysłowy, operatory `| && || ; > >>`.
pub fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                cur.push(c);
            }
            ' ' | '\t' => flush(&mut cur, &mut out),
            '|' | '&' | ';' | '>' => {
                flush(&mut cur, &mut out);
                let mut op = c.to_string();
                if let Some(&n) = chars.peek() {
                    if (c == '|' && n == '|') || (c == '&' && n == '&') || (c == '>' && n == '>') {
                        op.push(n);
                        chars.next();
                    }
                }
                out.push(op);
            }
            _ => cur.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn operator(tok: &str) -> Option<&'static str> {
    Some(match tok {
        "|" => "op.pipe",
        "&&" => "op.and",
        "||" => "op.or",
        ";" => "op.seq",
        ">" => "op.write",
        ">>" => "op.append",
        "&" => "op.background",
        _ => return None,
    })
}

/// Rozbiór całego wiersza. Pusty wiersz = brak opisu.
pub fn analyze(line: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let toks = tokens(line.trim());
    let mut i = 0;
    while i < toks.len() {
        // jedna komenda do następnego operatora
        let mut j = i;
        while j < toks.len() && operator(&toks[j]).is_none() {
            j += 1;
        }
        simple(&toks[i..j], &mut parts);
        if let Some(op) = toks.get(j) {
            parts.push(Part {
                tok: op.clone(),
                key: operator(op).unwrap(),
                kind: Kind::Normal,
            });
        }
        i = j + 1;
    }
    parts
}

fn simple(toks: &[String], parts: &mut Vec<Part>) {
    let mut k = 0;
    if toks.first().map(String::as_str) == Some("sudo") {
        parts.push(Part {
            tok: "sudo".into(),
            key: "cmd.sudo",
            kind: Kind::Warn,
        });
        k = 1;
    }
    let Some(name) = toks.get(k) else { return };
    let Some(cmd) = find(name) else {
        parts.push(Part {
            tok: name.clone(),
            key: "cmd.unknown",
            kind: Kind::Normal,
        });
        for t in &toks[k + 1..] {
            parts.push(Part {
                tok: t.clone(),
                key: ARG,
                kind: Kind::Normal,
            });
        }
        return;
    };
    parts.push(Part {
        tok: name.clone(),
        key: cmd.key,
        kind: Kind::Normal,
    });
    let mut sub_seen = false;
    let mut rec = false;
    let mut force = false;
    for t in &toks[k + 1..] {
        if let Some(stripped) = t.strip_prefix("--") {
            let key = cmd
                .long
                .iter()
                .chain(COMMON_LONG)
                .find(|(l, _)| *l == t.as_str())
                .map(|(_, k)| *k)
                .unwrap_or("flag.unknown");
            if stripped == "recursive" {
                rec = true;
            }
            if stripped == "force" {
                force = true;
            }
            parts.push(Part {
                tok: t.clone(),
                key,
                kind: Kind::Normal,
            });
        } else if t.len() > 1 && t.starts_with('-') {
            for l in t.chars().skip(1) {
                match l {
                    'r' | 'R' => rec = true,
                    'f' => force = true,
                    _ => {}
                }
                let key = cmd
                    .flags
                    .iter()
                    .find(|(c, _)| *c == l)
                    .map(|(_, k)| *k)
                    .unwrap_or("flag.unknown");
                parts.push(Part {
                    tok: format!("-{l}"),
                    key,
                    kind: Kind::Normal,
                });
            }
        } else if !sub_seen && !cmd.subs.is_empty() {
            sub_seen = true;
            let key = cmd
                .subs
                .iter()
                .find(|(s, _)| *s == t.as_str())
                .map(|(_, k)| *k)
                .unwrap_or("sub.unknown");
            parts.push(Part {
                tok: t.clone(),
                key,
                kind: Kind::Normal,
            });
        } else {
            let key = match (cmd.name, t.as_str()) {
                ("cd", "..") => "arg.up",
                ("cd", "~") => "arg.home",
                ("cd", "-") => "arg.back",
                _ => cmd.arg,
            };
            parts.push(Part {
                tok: t.clone(),
                key,
                kind: Kind::Normal,
            });
        }
    }
    if cmd.name == "rm" && rec && force {
        parts.push(Part {
            tok: "rm -rf".into(),
            key: "warn.rmrf",
            kind: Kind::Warn,
        });
    }
    match cmd.name {
        "zypper" | "transactional-update" => parts.push(Part {
            tok: "Cocktail".into(),
            key: "hint.cocktail",
            kind: Kind::Hint,
        }),
        "flatpak" => parts.push(Part {
            tok: "Shrimp Apps".into(),
            key: "hint.apps",
            kind: Kind::Hint,
        }),
        "snapper" => parts.push(Part {
            tok: "Panko".into(),
            key: "hint.panko",
            kind: Kind::Hint,
        }),
        _ => {}
    }
}

/// Wszystkie klucze używane przez rozbiór (test: każdy ma tekst w `_keys.tsv`).
#[cfg(test)]
pub fn all_keys() -> Vec<&'static str> {
    let mut v = vec![
        ARG,
        "cmd.sudo",
        "cmd.unknown",
        "flag.unknown",
        "sub.unknown",
        "arg.up",
        "arg.home",
        "arg.back",
        "warn.rmrf",
        "hint.cocktail",
        "hint.apps",
        "hint.panko",
        "op.pipe",
        "op.and",
        "op.or",
        "op.seq",
        "op.write",
        "op.append",
        "op.background",
    ];
    for c in COMMANDS {
        v.push(c.key);
        v.push(c.arg);
        v.extend(c.flags.iter().map(|(_, k)| *k));
        v.extend(c.long.iter().map(|(_, k)| *k));
        v.extend(c.subs.iter().map(|(_, k)| *k));
    }
    v.extend(COMMON_LONG.iter().map(|(_, k)| *k));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_text() {
        let tsv = include_str!("../i18n/_keys.tsv");
        let known: Vec<&str> = tsv
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split('\t').nth(1))
            .collect();
        let missing: Vec<_> = all_keys()
            .into_iter()
            .filter(|k| !known.contains(k))
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    fn keys(line: &str) -> Vec<(String, &'static str)> {
        analyze(line).into_iter().map(|p| (p.tok, p.key)).collect()
    }

    #[test]
    fn splits_flags_and_args() {
        assert_eq!(
            keys("ls -la Dokumenty"),
            vec![
                ("ls".into(), "cmd.ls"),
                ("-l".into(), "flag.ls.l"),
                ("-a".into(), "flag.ls.a"),
                ("Dokumenty".into(), "arg.path"),
            ]
        );
    }

    #[test]
    fn sudo_and_subcommand_with_hint() {
        let p = analyze("sudo zypper refresh");
        assert_eq!(p[0].kind, Kind::Warn);
        assert_eq!(p[1].key, "cmd.zypper");
        assert_eq!(p[2].key, "sub.zypper.refresh");
        assert_eq!(p.last().unwrap().kind, Kind::Hint);
    }

    #[test]
    fn rm_rf_warns() {
        assert!(analyze("rm -rf stary").iter().any(|p| p.key == "warn.rmrf"));
        assert!(analyze("rm -r -f stary")
            .iter()
            .any(|p| p.key == "warn.rmrf"));
        assert!(!analyze("rm plik").iter().any(|p| p.key == "warn.rmrf"));
    }

    #[test]
    fn operators_and_quotes() {
        assert_eq!(
            tokens("echo 'a b' | grep a && ls >> x"),
            vec!["echo", "'a b'", "|", "grep", "a", "&&", "ls", ">>", "x"]
        );
        let k: Vec<_> = analyze("cat a | grep -i b")
            .into_iter()
            .map(|p| p.key)
            .collect();
        assert_eq!(
            k,
            vec![
                "cmd.cat",
                "arg.file",
                "op.pipe",
                "cmd.grep",
                "flag.grep.i",
                "arg.pattern"
            ]
        );
    }

    #[test]
    fn unknown_and_empty() {
        assert!(analyze("   ").is_empty());
        assert_eq!(analyze("foo bar")[0].key, "cmd.unknown");
        assert_eq!(keys("cd ..")[1].1, "arg.up");
    }
}
