//! Kodowanie napisów w D-Bus usługi migawek: znaki spoza ASCII i bajty niebędące
//! UTF-8 przechodzą jako "\x??", a sam "\" jako "\\" (doc/dbus-protocol.txt w snapperze).

/// Zamienia napis z usługi na zwykły tekst. Niepoprawne sekwencje zostają bez zmian.
pub fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            if b[i + 1] == b'\\' {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if b[i + 1] == b'x' && i + 3 < b.len() {
                if let Ok(v) = u8::from_str_radix(&s[i + 2..i + 4], 16) {
                    out.push(v);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Zamienia tekst na postać, którą przyjmuje usługa.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &c in s.as_bytes() {
        match c {
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(c as char),
            _ => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_polish() {
        let t = "Przed zmianą ustawień sieci \\ ok";
        assert_eq!(
            escape(t),
            "Przed zmian\\xc4\\x85 ustawie\\xc5\\x84 sieci \\\\ ok"
        );
        assert_eq!(unescape(&escape(t)), t);
    }

    #[test]
    fn broken_sequences_stay() {
        assert_eq!(unescape("a\\xZZ\\"), "a\\xZZ\\");
        assert_eq!(unescape("/etc/x\\x20y"), "/etc/x y");
    }
}
