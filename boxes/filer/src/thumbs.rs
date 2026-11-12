//! Miniatury zgodne z freedesktop (~/.cache/thumbnails/large, wspólne z Nemo i GNOME).
//! Obrazy skaluje gdk-pixbuf; inne typy (wideo, PDF…) robią programy z /usr/share/thumbnailers.
//! Jeden wątek w tle, kolejka bez powtórzeń; wynik wraca do wątku interfejsu.
use gtk::gdk_pixbuf::Pixbuf;
use gtk::{gio, glib, prelude::*};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const SIZE: i32 = 256;

struct Job {
    uri: String,
    path: PathBuf,
    mime: String,
    mtime: u64,
}

type Done = Box<dyn FnOnce(Option<PathBuf>)>;

thread_local! {
    static WAITING: RefCell<HashMap<String, Vec<Done>>> = RefCell::new(HashMap::new());
    static FAILED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static QUEUE: RefCell<Option<mpsc::Sender<Job>>> = const { RefCell::new(None) };
}

fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join("thumbnails").join("large")
}

pub fn target(uri: &str) -> PathBuf {
    let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri).unwrap_or_default();
    cache_dir().join(format!("{md5}.png"))
}

/// Gotowa miniatura (z GIO albo z naszej pamięci podręcznej).
pub fn existing(info: &gio::FileInfo) -> Option<PathBuf> {
    if info.boolean("thumbnail::failed") {
        return None;
    }
    if let Some(p) = info.attribute_byte_string("thumbnail::path") {
        return Some(PathBuf::from(p.as_str()));
    }
    let f = crate::util::file_of(info)?;
    let t = target(&f.uri());
    let fresh = std::fs::metadata(&t).ok()?.modified().ok()? >= std::time::UNIX_EPOCH + std::time::Duration::from_secs(info.attribute_uint64("time::modified"));
    fresh.then_some(t)
}

fn thumbnailers() -> &'static Vec<(Vec<String>, String)> {
    static T: std::sync::OnceLock<Vec<(Vec<String>, String)>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut v = vec![];
        let mut dirs = vec![glib::user_data_dir()];
        dirs.extend(glib::system_data_dirs());
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(d.join("thumbnailers")) else { continue };
            for e in rd.flatten() {
                let Ok(s) = std::fs::read_to_string(e.path()) else { continue };
                let get = |k: &str| s.lines().find_map(|l| l.strip_prefix(k).map(|r| r.trim_start_matches([' ', '=']).trim().to_string()));
                if let (Some(exec), Some(mimes)) = (get("Exec"), get("MimeType")) {
                    v.push((mimes.split(';').filter(|m| !m.is_empty()).map(String::from).collect(), exec));
                }
            }
        }
        v
    })
}

fn image_mime(m: &str) -> bool {
    Pixbuf::formats().iter().any(|f| f.mime_types().iter().any(|t| t == m))
}

/// Czy warto robić miniaturę (typ obsługiwany, plik lokalny, nie za duży).
pub fn wanted(info: &gio::FileInfo, limit_mb: u64) -> bool {
    let Some(mime) = info.content_type() else { return false };
    let Some(f) = crate::util::file_of(info) else { return false };
    if f.path().is_none() || info.boolean("thumbnail::failed") || (info.size() as u64) > limit_mb * 1_000_000 {
        return false;
    }
    if FAILED.with(|x| x.borrow().contains(f.uri().as_str())) {
        return false;
    }
    let m = mime.as_str();
    image_mime(m) || thumbnailers().iter().any(|(ms, _)| ms.iter().any(|x| x == m))
}

pub fn request(f: &gio::File, info: &gio::FileInfo, done: impl FnOnce(Option<PathBuf>) + 'static) {
    let Some(path) = f.path() else { return done(None) };
    let uri = f.uri().to_string();
    let first = WAITING.with(|w| {
        let mut w = w.borrow_mut();
        let e = w.entry(uri.clone()).or_default();
        e.push(Box::new(done));
        e.len() == 1
    });
    if !first {
        return;
    }
    let job = Job { uri, path, mime: info.content_type().map(|s| s.to_string()).unwrap_or_default(), mtime: info.attribute_uint64("time::modified") };
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        let tx = q.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            let (back_tx, back_rx) = async_channel::unbounded::<(String, Option<PathBuf>)>();
            std::thread::spawn(move || {
                while let Ok(j) = rx.recv() {
                    let r = make(&j);
                    if back_tx.send_blocking((j.uri, r)).is_err() {
                        break;
                    }
                }
            });
            glib::spawn_future_local(async move {
                while let Ok((uri, r)) = back_rx.recv().await {
                    if r.is_none() {
                        FAILED.with(|x| x.borrow_mut().insert(uri.clone()));
                    }
                    let cbs = WAITING.with(|w| w.borrow_mut().remove(&uri)).unwrap_or_default();
                    for cb in cbs {
                        cb(r.clone());
                    }
                }
            });
            tx
        });
        let _ = tx.send(job);
    });
}

fn make(j: &Job) -> Option<PathBuf> {
    let out = target(&j.uri);
    if out.exists() && std::fs::metadata(&out).and_then(|m| m.modified()).ok()? >= std::time::UNIX_EPOCH + std::time::Duration::from_secs(j.mtime) {
        return Some(out);
    }
    std::fs::create_dir_all(cache_dir()).ok()?;
    let tmp = out.with_extension(format!("{}.tmp.png", std::process::id()));
    if image_mime(&j.mime) {
        let pb = Pixbuf::from_file_at_scale(&j.path, SIZE, SIZE, true).ok()?;
        let pb = pb.apply_embedded_orientation().unwrap_or(pb);
        save(&pb, &tmp, j)?;
    } else {
        let exec = thumbnailers().iter().find(|(ms, _)| ms.contains(&j.mime))?.1.clone();
        let args: Vec<String> = exec
            .split_whitespace()
            .map(|a| a.replace("%s", &SIZE.to_string()).replace("%u", &j.uri).replace("%i", &j.path.to_string_lossy()).replace("%o", &tmp.to_string_lossy()).replace("%%", "%"))
            .collect();
        let ok = std::process::Command::new(&args[0]).args(&args[1..]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().ok()?.success();
        if !ok {
            return None;
        }
        // Dopisujemy znaczniki URI i MTime, żeby miniatura była ważna dla innych programów.
        let pb = Pixbuf::from_file(&tmp).ok()?;
        save(&pb, &tmp, j)?;
    }
    std::fs::rename(&tmp, &out).ok()?;
    Some(out)
}

fn save(pb: &Pixbuf, to: &Path, j: &Job) -> Option<()> {
    pb.savev(to, "png", &[("tEXt::Thumb::URI", &j.uri), ("tEXt::Thumb::MTime", &j.mtime.to_string())]).ok()
}
