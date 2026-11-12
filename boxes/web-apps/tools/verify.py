#!/usr/bin/env python3
"""Sprawdza katalog aplikacji internetowych i pobiera ich ikony.

Dla każdej pozycji z data/webapps.json (i rezerwy data/webapps-reserve.json):
  1. pobiera adres (z przekierowaniami) i zapisuje kod HTTP oraz adres końcowy,
  2. szuka manifestu PWA (<link rel="manifest">) i największej ikony,
     potem apple-touch-icon, <link rel="icon">, na końcu /favicon.ico,
  3. zapisuje ikonę jako data/icons/<id>.png (128x128, ImageMagick).

Wynik: data/verify-report.json (maszynowo) i data/VERIFY.md (raport po polsku).
Pole "icon" w webapps.json dostaje ścieżkę ikony, gdy udało się ją pobrać.
Tylko biblioteka standardowa + polecenia `convert`/`identify` (ImageMagick).

Uruchamiane w CI (.github/workflows/web-apps.yml), bo sieć w kontenerach
deweloperskich bywa ograniczona.
"""
import concurrent.futures as cf
import datetime as dt
import gzip
import html.parser
import json
import os
import re
import ssl
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "data")
ICONS = os.path.join(DATA, "icons")
UA = ("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
      "Chrome/142.0.0.0 Safari/537.36")
HEADERS = {
    "User-Agent": UA,
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
    "Accept-Language": "pl-PL,pl;q=0.9,en;q=0.8",
    "Accept-Encoding": "gzip, deflate",
}
TIMEOUT = 25
CTX = ssl.create_default_context()


def fetch(url, limit=4_000_000):
    r = _fetch(url, limit)
    if r[0] is None:  # DNS/TLS/limit czasu: jedna powtórka
        r = _fetch(url, limit)
    return r


def _fetch(url, limit):
    req = urllib.request.Request(url, headers=HEADERS)
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT, context=CTX) as r:
            body = r.read(limit)
            enc = (r.headers.get("Content-Encoding") or "").lower()
            if enc == "gzip":
                body = gzip.decompress(body)
            elif enc == "deflate":
                body = zlib.decompress(body)
            return r.status, r.geturl(), r.headers.get("Content-Type", ""), body, None
    except urllib.error.HTTPError as e:
        return e.code, e.geturl() or url, e.headers.get("Content-Type", "") if e.headers else "", b"", None
    except Exception as e:  # DNS, TLS, timeout...
        return None, url, "", b"", f"{type(e).__name__}: {e}"


class Links(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.manifest = None
        self.icons = []  # (rel, href, sizes)

    def handle_starttag(self, tag, attrs):
        if tag != "link":
            return
        a = {k.lower(): (v or "") for k, v in attrs}
        rel = a.get("rel", "").lower().split()
        href = a.get("href")
        if not href:
            return
        if "manifest" in rel and not self.manifest:
            self.manifest = href
        elif "apple-touch-icon" in rel or "apple-touch-icon-precomposed" in rel:
            self.icons.append(("apple", href, a.get("sizes", "")))
        elif "icon" in rel:
            self.icons.append(("icon", href, a.get("sizes", "")))


def size_of(sizes):
    best = 0
    for s in (sizes or "").lower().split():
        if s == "any":
            return 1024
        m = re.match(r"(\d+)x(\d+)", s)
        if m:
            best = max(best, int(m.group(1)))
    return best


def rank(size):
    # Najlepiej 128..512; większe też dobre, mniejsze gorsze.
    if size == 0:
        return 1
    if size >= 128:
        return 1000 - abs(size - 192) / 10
    return size


def candidates(page_url, html_text):
    p = Links()
    try:
        p.feed(html_text)
    except Exception:
        pass
    out = []
    if p.manifest:
        murl = urllib.parse.urljoin(page_url, p.manifest)
        st, final, _, body, _ = fetch(murl)
        if st == 200 and body:
            try:
                man = json.loads(body.decode("utf-8-sig", "replace"))
                for ic in man.get("icons", []) or []:
                    purpose = (ic.get("purpose") or "any").split()
                    if "any" not in purpose:
                        continue
                    src = ic.get("src")
                    if not src:
                        continue
                    sz = size_of(ic.get("sizes"))
                    if (ic.get("type") or "").endswith("svg") or src.split("?")[0].endswith(".svg"):
                        sz = max(sz, 512)
                    out.append(("manifest", urllib.parse.urljoin(final, src), 3000 + rank(sz)))
            except Exception:
                pass
    for kind, href, sizes in p.icons:
        sz = size_of(sizes)
        if kind == "apple":
            sz = sz or 180
        if href.split("?")[0].endswith(".svg"):
            sz = max(sz, 512)
        base = 2000 if kind == "apple" else 1000
        out.append((kind, urllib.parse.urljoin(page_url, href), base + rank(sz)))
    host = urllib.parse.urlsplit(page_url).netloc
    out.append(("apple", urllib.parse.urljoin(page_url, "/apple-touch-icon.png"), 1500))
    # Usługa ikon Google: działa też dla stron, które nie wpuszczają skryptu.
    out.append(("s2", f"https://www.google.com/s2/favicons?domain={host}&sz=256", 1200))
    out.append(("favicon", urllib.parse.urljoin(page_url, "/favicon.ico"), 0))
    out.sort(key=lambda c: -c[2])
    return p.manifest is not None, out


def run(args, timeout=60):
    return subprocess.run(args, capture_output=True, text=True, timeout=timeout)


def decode_icon(data, ctype, dest):
    """Zamienia pobrany plik (png/jpg/webp/ico/svg) na PNG bez zmiany rozmiaru rastrów.
    SVG jest renderowany ostro w 512 px. Zwraca efektywną liczbę pikseli (rozdzielczość
    źródła) albo None, gdy plik nie jest obrazem."""
    with tempfile.TemporaryDirectory() as tmp:
        is_svg = b"<svg" in data[:3000] or "svg" in ctype
        ext = ".svg" if is_svg else ".ico" if data[:4] == b"\0\0\1\0" else ".img"
        src = os.path.join(tmp, "src" + ext)
        with open(src, "wb") as f:
            f.write(data)
        frame = src + "[0]"
        if ext == ".ico":
            ident = run(["identify", "-format", "%[scene] %w\n", src]).stdout.split("\n")
            frames = [tuple(map(int, l.split())) for l in ident if len(l.split()) == 2]
            if frames:
                frame = f"{src}[{max(frames, key=lambda x: x[1])[0]}]"
        if is_svg:
            r = run(["convert", "-background", "none", "-density", "600", src, "-resize", "512x512",
                     "-strip", "PNG32:" + dest])
            px = 512
        else:
            r = run(["convert", "-background", "none", frame, "-strip", "PNG32:" + dest])
            px = 0
        if r.returncode != 0 or not os.path.exists(dest):
            return None
        if not is_svg:
            w = run(["identify", "-format", "%w", dest]).stdout.strip()
            px = int(w[:5] or 0)
        return px


def dominant_color(png):
    """Średni kolor ikony (ważony nasyceniem i kryciem), przyciemniony pod białą literę."""
    raw = subprocess.run(["convert", png, "-resize", "16x16!", "-depth", "8", "rgba:-"],
                         capture_output=True, timeout=30).stdout
    tot = [0.0, 0.0, 0.0]
    wsum = 0.0
    for i in range(0, len(raw) - 3, 4):
        r, g, b, a = raw[i:i + 4]
        sat = (max(r, g, b) - min(r, g, b)) / 255
        w = (a / 255) * (0.1 + sat)
        wsum += w
        for k, v in enumerate((r, g, b)):
            tot[k] += v * w
    if wsum == 0:
        return "#3584e4"
    r, g, b = (t / wsum for t in tot)
    lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255
    if lum > 0.55:  # biała litera musi być czytelna
        f = 0.55 / lum
        r, g, b = r * f, g * f, b * f
    return "#%02x%02x%02x" % (int(r), int(g), int(b))


def corner_opaque(png):
    out = run(["convert", png, "-format", "%[fx:p{0,0}.a]", "info:"]).stdout.strip()
    try:
        return float(out) > 0.9
    except ValueError:
        return False


def finish_icon(work, px, kind, name, dest):
    """Składa końcową ikonę 128x128 z pliku `work`. Zwraca styl: ostra / kafelek / litera."""
    mask = ["(", "-size", "128x128", "xc:none", "-fill", "white", "-draw",
            "roundrectangle 0,0 127,127 26,26", ")"]
    if kind != "simple" and px >= 96:
        args = ["convert", work, "-filter", "Lanczos", "-resize", "128x128", "-background", "none",
                "-gravity", "center", "-extent", "128x128"]
        if corner_opaque(work):  # pełne tło do krawędzi: zaokrąglamy rogi jak w GNOME
            args += mask + ["-compose", "DstIn", "-composite"]
        r = run(args + ["-strip", "PNG32:" + dest])
        return "ostra" if r.returncode == 0 else None
    if kind == "simple" or px >= 48:
        # Mała albo jednokolorowa ikona: na jasnym kafelku, bez rozciągania ponad ~1,5x.
        size = 80 if kind == "simple" else min(80, int(px * 1.5))
        r = run(["convert", "-size", "128x128", "xc:none", "-fill", "#f6f5f4", "-draw",
                 "roundrectangle 0,0 127,127 26,26", "(", work, "-filter", "Lanczos", "-resize",
                 f"{size}x{size}", ")", "-gravity", "center", "-composite", "-strip", "PNG32:" + dest])
        return "kafelek" if r.returncode == 0 else None
    letter = (name.strip()[:1] or "?").upper()
    color = dominant_color(work)
    r = run(["convert", "-size", "128x128", "xc:none", "-fill", color, "-draw",
             "roundrectangle 0,0 127,127 26,26", "-fill", "white", "-font", "DejaVu-Sans-Bold",
             "-pointsize", "76", "-gravity", "center", "-annotate", "+0+6", letter,
             "-strip", "PNG32:" + dest])
    return "litera" if r.returncode == 0 else None


def simple_slugs(app):
    base = re.sub(r"[^a-z0-9]", "", app["name"].lower().split(" (")[0].replace("+", "plus"))
    ids = re.sub(r"[^a-z0-9]", "", app["id"])
    return list(dict.fromkeys([base, ids]))


def check(app):
    st, final, ctype, body, err = fetch(app["url"])
    res = {"id": app["id"], "url": app["url"], "status": st, "final_url": final, "error": err,
           "manifest": False, "icon_source": None, "icon_kind": None, "icon_px": None,
           "icon_style": None}
    if st is None:
        res["result"] = "błąd"
    elif 200 <= st < 300:
        res["result"] = "ok"
    elif st in (400, 401, 403, 405, 429, 451, 503) or 300 <= st < 400:
        # Serwer odpowiada, ale odrzuca automat (ochrona przed botami, 400 dla nieznanej
        # przeglądarki, pętla przekierowań bez ciasteczek).
        res["result"] = "zablokowane"
    else:
        res["result"] = "błąd"
    page = final if st else app["url"]
    text = body.decode("utf-8", "replace") if "html" in ctype or body[:200].lstrip().startswith(b"<") else ""
    has_manifest, cands = candidates(page, text)
    res["manifest"] = has_manifest
    host = urllib.parse.urlsplit(page).netloc
    cands.append(("ddg", f"https://icons.duckduckgo.com/ip3/{host}.ico", 100))
    for slug in simple_slugs(app):  # ostatnia deska ratunku: jednokolorowy znak marki
        cands.append(("simple", f"https://cdn.simpleicons.org/{slug}", 50))
    if app.get("icon_url"):  # ręcznie wskazana ikona ma pierwszeństwo
        cands.insert(0, ("ręczna", app["icon_url"], 9999))
    dest = os.path.join(ICONS, app["id"] + ".png")
    with tempfile.TemporaryDirectory() as tmp:
        best_px, best_work = 0, os.path.join(tmp, "best.png")
        work = os.path.join(tmp, "try.png")
        for kind, url, _ in cands:
            if kind == "simple" and best_px >= 64:
                continue
            ist, _, ictype, data, _ = fetch(url, limit=2_000_000)
            if ist != 200 or len(data) < 100 or "html" in ictype:
                continue
            px = decode_icon(data, ictype, work)
            if px is None:
                continue
            if kind == "simple":
                px = 100  # sztucznie: ostry znak wygrywa z ikonką 16-48 px, nie ze zwykłą ikoną
            if px > best_px:
                best_px = px
                os.replace(work, best_work)
                res.update(icon_source=url, icon_kind=kind, icon_px=px)
                if px >= 128 or kind == "ręczna":
                    break
        if best_px:
            style = finish_icon(best_work, best_px, res["icon_kind"], app["name"], dest)
            res["icon_style"] = style
            if style is None:
                res.update(icon_source=None, icon_kind=None, icon_px=None)
        elif os.path.exists(dest):
            os.remove(dest)  # bez źródła zostaje litera rysowana przez aplikację
    return res


def load(name):
    with open(os.path.join(DATA, name)) as f:
        return json.load(f)


def dump(doc, name):
    with open(os.path.join(DATA, name), "w") as f:
        f.write('{\n "version": %d,\n "apps": [\n' % doc["version"])
        f.write(",\n".join("  " + json.dumps(a, ensure_ascii=False) for a in doc["apps"]))
        f.write("\n ]\n}\n")


def main():
    os.makedirs(ICONS, exist_ok=True)
    main_doc, reserve = load("webapps.json"), load("webapps-reserve.json")
    apps = main_doc["apps"] + reserve["apps"]
    with cf.ThreadPoolExecutor(16) as ex:
        results = dict(zip([a["id"] for a in apps], ex.map(check, apps)))
    for doc in (main_doc, reserve):
        for a in doc["apps"]:
            r = results[a["id"]]
            if r["icon_source"]:
                a["icon"] = f"icons/{a['id']}.png"
            elif os.path.exists(os.path.join(ICONS, a["id"] + ".png")):
                a["icon"] = f"icons/{a['id']}.png"  # z poprzedniego przebiegu
            else:
                a.pop("icon", None)
    dump(main_doc, "webapps.json")
    dump(reserve, "webapps-reserve.json")
    now = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    with open(os.path.join(DATA, "verify-report.json"), "w") as f:
        json.dump({"checked_at": now, "apps": [results[a["id"]] for a in main_doc["apps"]],
                   "reserve": [results[a["id"]] for a in reserve["apps"]]}, f, ensure_ascii=False, indent=1)
    write_md(now, main_doc["apps"], reserve["apps"], results)
    bad = [a["id"] for a in main_doc["apps"] if results[a["id"]]["result"] == "błąd"]
    print(f"Sprawdzono {len(apps)} adresów; błędy w głównej liście: {len(bad)} {bad}")


def write_md(now, apps, reserve, results):
    def row(a):
        r = results[a["id"]]
        st = r["status"] if r["status"] is not None else (r["error"] or "")[:60]
        final = r["final_url"] if r["final_url"] != a["url"] else ""
        icon = f"{r['icon_kind']} {r['icon_px']}px, {r['icon_style']}" if r["icon_source"] else "brak (litera z aplikacji)"
        return f"| {a['name']} | {a['url']} | {r['result']} | {st} | {final} | {'tak' if r['manifest'] else 'nie'} | {icon} |"
    head = "| Nazwa | Adres | Wynik | HTTP | Przekierowanie | Manifest PWA | Ikona |\n|---|---|---|---|---|---|---|"
    cnt = lambda lst, k: sum(1 for a in lst if results[a["id"]]["result"] == k)
    icons = sum(1 for a in apps if results[a["id"]]["icon_source"])
    lines = [
        "# Weryfikacja katalogu aplikacji internetowych", "",
        f"Sprawdzono: {now} (GitHub Actions, `boxes/web-apps/tools/verify.py`).", "",
        f"Główna lista: {len(apps)} pozycji. Odpowiada poprawnie (2xx): {cnt(apps, 'ok')}, "
        f"serwer odrzuca automat (400/401/403/429/503, pętla przekierowań): {cnt(apps, 'zablokowane')}, "
        f"błąd (404, 5xx, DNS, TLS, limit czasu): {cnt(apps, 'błąd')}. Ikony pobrane: {icons}.", "",
        "„zablokowane” znaczy, że serwer istnieje i odpowiada, ale nie wpuszcza skryptu "
        "(ochrona przed botami, kod 400 dla nieznanej przeglądarki albo pętla przekierowań bez "
        "ciasteczek); adresu nie dało się więc potwierdzić w pełni.", "",
        "Przekierowanie na stronę logowania albo stronę produktu jest normalne dla aplikacji, "
        "które wymagają konta: skrypt nie jest zalogowany.", "",
        "## Główna lista", "", head, *[row(a) for a in apps], "",
        "## Rezerwa (do podmiany błędnych pozycji)", "", head, *[row(a) for a in reserve], "",
    ]
    with open(os.path.join(DATA, "VERIFY.md"), "w") as f:
        f.write("\n".join(lines))


if __name__ == "__main__":
    sys.exit(main())
