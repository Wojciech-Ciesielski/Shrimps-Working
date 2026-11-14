//! Nazwa programu z pliku .exe (zasób wersji: ProductName, potem FileDescription), do wyszukania
//! w Wine AppDB. Czyta tylko nagłówki i sekcję zasobów, nie cały plik.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_RSRC: u32 = 16 * 1024 * 1024;

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn read_at(f: &mut File, off: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut v = vec![0u8; len];
    f.read_exact(&mut v).ok()?;
    Some(v)
}

/// Sekcja zasobów: (RVA katalogu zasobów, bajty sekcji od tego RVA).
fn resource_section(f: &mut File) -> Option<(u32, Vec<u8>)> {
    let dos = read_at(f, 0, 64)?;
    if &dos[0..2] != b"MZ" {
        return None;
    }
    let pe = u32le(&dos, 0x3C)? as u64;
    let hdr = read_at(f, pe, 24)?;
    if &hdr[0..4] != b"PE\0\0" {
        return None;
    }
    let nsect = u16le(&hdr, 6)? as usize;
    let opt_size = u16le(&hdr, 20)? as usize;
    let opt = read_at(f, pe + 24, opt_size)?;
    let dd = match u16le(&opt, 0)? {
        0x10b => 96,
        0x20b => 112,
        _ => return None,
    };
    let rva = u32le(&opt, dd + 2 * 8)?;
    let size = u32le(&opt, dd + 2 * 8 + 4)?;
    if rva == 0 || size == 0 || size > MAX_RSRC {
        return None;
    }
    let sects = read_at(f, pe + 24 + opt_size as u64, nsect.min(96) * 40)?;
    for s in sects.chunks_exact(40) {
        let (vsize, vaddr, raw_size, raw_ptr) = (u32le(s, 8)?, u32le(s, 12)?, u32le(s, 16)?, u32le(s, 20)?);
        let span = vsize.max(raw_size);
        if rva >= vaddr && rva < vaddr.saturating_add(span) {
            let off = raw_ptr as u64 + (rva - vaddr) as u64;
            let len = raw_size.saturating_sub(rva - vaddr).min(MAX_RSRC) as usize;
            return Some((rva, read_at(f, off, len)?));
        }
    }
    None
}

/// Pierwszy wpis katalogu zasobów (opcjonalnie o danym identyfikatorze): (czy podkatalog, przesunięcie).
fn dir_entry(b: &[u8], dir: usize, id: Option<u32>) -> Option<(bool, usize)> {
    let named = u16le(b, dir + 12)? as usize;
    let ids = u16le(b, dir + 14)? as usize;
    for i in 0..(named + ids).min(4096) {
        let e = dir + 16 + i * 8;
        let name = u32le(b, e)?;
        let off = u32le(b, e + 4)?;
        if id.is_none_or(|want| name == want) {
            return Some((off & 0x8000_0000 != 0, (off & 0x7FFF_FFFF) as usize));
        }
    }
    None
}

/// Bajty zasobu RT_VERSION (typ 16), pierwszy wpis nazwy i języka.
fn version_resource(rva: u32, sec: &[u8]) -> Option<&[u8]> {
    let (sub, d1) = dir_entry(sec, 0, Some(16))?;
    if !sub {
        return None;
    }
    let (sub, d2) = dir_entry(sec, d1, None)?;
    if !sub {
        return None;
    }
    let (sub, data) = dir_entry(sec, d2, None)?;
    if sub {
        return None;
    }
    let drva = u32le(sec, data)?;
    let dsize = u32le(sec, data + 4)? as usize;
    let start = drva.checked_sub(rva)? as usize;
    sec.get(start..start.checked_add(dsize)?)
}

fn utf16(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

/// Wartość klucza z tabeli tekstów VS_VERSIONINFO (wyszukanie „Klucz\0”, wyrównanie do 4 bajtów, tekst).
pub fn version_string(ver: &[u8], key: &str) -> Option<String> {
    let pat: Vec<u8> = key.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
    let pos = ver.windows(pat.len()).enumerate().find(|(i, w)| i % 2 == 0 && *w == pat.as_slice()).map(|(i, _)| i)?;
    let mut v = pos + pat.len();
    v = (v + 3) & !3;
    let rest = ver.get(v..)?;
    let units: Vec<u16> = utf16(rest).into_iter().take_while(|u| *u != 0).take(256).collect();
    let s = String::from_utf16_lossy(&units).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Nazwa produktu z pliku .exe (ProductName, potem FileDescription), bez dopisków „Setup”/„Installer”.
pub fn product_name(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let (rva, sec) = resource_section(&mut f)?;
    let ver = version_resource(rva, &sec)?;
    let name = version_string(ver, "ProductName").or_else(|| version_string(ver, "FileDescription"))?;
    Some(clean(&name))
}

fn clean(name: &str) -> String {
    let mut s = name.trim().to_string();
    for suffix in [" Setup", " Installer", " Installation", " Install", " (x64)", " (64-bit)", " x64"] {
        if s.len() > suffix.len() && s.to_lowercase().ends_with(&suffix.to_lowercase()) {
            s.truncate(s.len() - suffix.len());
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
    }

    /// Minimalny PE32+ z jedną sekcją .rsrc zawierającą RT_VERSION z ProductName.
    fn tiny_pe(product: &str) -> Vec<u8> {
        // tabela tekstów: …"ProductName\0" + wyrównanie + wartość
        let mut ver = vec![0u8; 6];
        ver.extend(w("VS_VERSION_INFO"));
        while !ver.len().is_multiple_of(4) {
            ver.push(0);
        }
        ver.extend([0u8; 6]);
        ver.extend(w("ProductName"));
        while !ver.len().is_multiple_of(4) {
            ver.push(0);
        }
        ver.extend(w(product));
        let rva = 0x1000u32;
        let mut rs = vec![0u8; 0];
        // katalog typów: 1 wpis id=16 → podkatalog 0x18
        let dir = |entries: &[(u32, u32)]| {
            let mut d = vec![0u8; 12];
            d.extend(0u16.to_le_bytes());
            d.extend((entries.len() as u16).to_le_bytes());
            for (n, o) in entries {
                d.extend(n.to_le_bytes());
                d.extend(o.to_le_bytes());
            }
            d
        };
        rs.extend(dir(&[(16, 0x8000_0018)])); // 0x00..0x18
        rs.extend(dir(&[(1, 0x8000_0030)])); // 0x18..0x30
        rs.extend(dir(&[(0x415, 0x48)])); // 0x30..0x48
        let data_off = 0x58u32;
        rs.extend((rva + data_off).to_le_bytes()); // 0x48 wpis danych
        rs.extend((ver.len() as u32).to_le_bytes());
        rs.extend([0u8; 8]);
        assert_eq!(rs.len(), data_off as usize);
        rs.extend(&ver);
        let raw_ptr = 0x200u32;
        let mut f = vec![0u8; 0x40];
        f[0] = b'M';
        f[1] = b'Z';
        f[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        f.extend(b"PE\0\0");
        let mut coff = vec![0u8; 20];
        coff[2..4].copy_from_slice(&1u16.to_le_bytes());
        coff[16..18].copy_from_slice(&240u16.to_le_bytes());
        f.extend(coff);
        let mut opt = vec![0u8; 240];
        opt[0..2].copy_from_slice(&0x20bu16.to_le_bytes());
        opt[112 + 16..112 + 20].copy_from_slice(&rva.to_le_bytes());
        opt[112 + 20..112 + 24].copy_from_slice(&(rs.len() as u32).to_le_bytes());
        f.extend(opt);
        let mut sect = vec![0u8; 40];
        sect[0..5].copy_from_slice(b".rsrc");
        sect[8..12].copy_from_slice(&(rs.len() as u32).to_le_bytes());
        sect[12..16].copy_from_slice(&rva.to_le_bytes());
        sect[16..20].copy_from_slice(&(rs.len() as u32).to_le_bytes());
        sect[20..24].copy_from_slice(&raw_ptr.to_le_bytes());
        f.extend(sect);
        f.resize(raw_ptr as usize, 0);
        f.extend(rs);
        f
    }

    #[test]
    fn reads_product_name() {
        let t = std::env::temp_dir().join(format!("shrimp-pe-{}.exe", std::process::id()));
        std::fs::write(&t, tiny_pe("Notepad++ Installer")).unwrap();
        assert_eq!(product_name(&t).as_deref(), Some("Notepad++"));
        std::fs::write(&t, b"MZ not really").unwrap();
        assert_eq!(product_name(&t), None);
        let _ = std::fs::remove_file(&t);
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean("7-Zip Setup"), "7-Zip");
        assert_eq!(clean("IrfanView (64-bit)"), "IrfanView");
        assert_eq!(clean("Setup"), "Setup");
    }
}
