#!/usr/bin/env python3
"""Lokalne repozytorium testowe udające Flathub (bez sieci).

Buduje repozytorium OSTree z małym środowiskiem uruchomieniowym i kilkudziesięcioma
aplikacjami testowymi (z ikonami, zrzutami, kolorami marki i AppStream), żeby
sprawdzić Shrimp Apps: katalog, wyszukiwanie, instalację, aktualizację i usuwanie.

  tests/test-repo.py build OUT          repozytorium w OUT/repo, obrazy w OUT/media
  tests/test-repo.py bump OUT APP_ID    nowa wersja aplikacji (do testu aktualizacji)

Zrzuty ekranu wskazują na http://127.0.0.1:8765/ (python3 -m http.server -d OUT/media 8765).
Wymaga: flatpak, ostree, ImageMagick (convert).
"""
import gzip
import os
import shutil
import subprocess
import sys
import textwrap

ARCH = subprocess.run(["flatpak", "--default-arch"], capture_output=True, text=True).stdout.strip() or "x86_64"
RUNTIME = "org.shrimp.TestPlatform"
MEDIA = "http://127.0.0.1:8765"

# id, nazwa, kategorie, podsumowanie, kolor
APPS = [
    ("org.shrimp.test.Paint", "Pędzel", "Graphics;2DGraphics", "Rysowanie i malowanie cyfrowe", "#3584e4"),
    ("org.shrimp.test.Vector", "Wektor", "Graphics;VectorGraphics", "Grafika wektorowa i ilustracje", "#26a269"),
    ("org.shrimp.test.Photos", "Ciemnia", "Graphics;Photography", "Wywoływanie zdjęć RAW", "#c64600"),
    ("org.shrimp.test.Model", "Bryła", "Graphics;3DGraphics", "Modelowanie i animacja 3D", "#e66100"),
    ("org.shrimp.test.Pixel", "Piksel", "Graphics;RasterGraphics", "Edytor grafiki rastrowej", "#9141ac"),
    ("org.shrimp.test.Scan", "Skaner", "Graphics;Scanning", "Skanowanie dokumentów", "#1c71d8"),
    ("org.shrimp.test.Player", "Odtwarzacz", "AudioVideo;Video;Player", "Filmy w każdym formacie", "#ff7800"),
    ("org.shrimp.test.Cut", "Montaż", "AudioVideo;Video;AudioVideoEditing", "Edycja wideo dla każdego", "#5e5c64"),
    ("org.shrimp.test.Record", "Nagrywarka", "AudioVideo;Video;Recorder", "Nagrywanie ekranu i kamery", "#c01c28"),
    ("org.shrimp.test.Stream", "Transmisja", "AudioVideo;Video", "Transmisje na żywo", "#613583"),
    ("org.shrimp.test.Music", "Muzyka", "AudioVideo;Audio;Player", "Twoja biblioteka muzyki", "#e5a50a"),
    ("org.shrimp.test.Studio", "Studio", "AudioVideo;Audio;Music", "Nagrywanie i miksowanie dźwięku", "#2ec27e"),
    ("org.shrimp.test.Podcast", "Podcasty", "AudioVideo;Audio", "Słuchaj ulubionych podcastów", "#865e3c"),
    ("org.shrimp.test.Radio", "Radio", "AudioVideo;Audio", "Stacje radiowe z całego świata", "#1a5fb4"),
    ("org.shrimp.test.Chess", "Szachy", "Game;BoardGame", "Klasyczne szachy", "#77767b"),
    ("org.shrimp.test.Kart", "Wyścigi", "Game;ArcadeGame", "Wyścigi gokartów", "#f6d32d"),
    ("org.shrimp.test.Mines", "Saper", "Game;LogicGame", "Znajdź wszystkie miny", "#57e389"),
    ("org.shrimp.test.Cards", "Pasjans", "Game;CardGame", "Kolekcja pasjansów", "#a51d2d"),
    ("org.shrimp.test.Writer", "Pisarz", "Office;WordProcessor", "Edytor tekstu", "#1c71d8"),
    ("org.shrimp.test.Calc", "Arkusz", "Office;Spreadsheet", "Arkusz kalkulacyjny", "#26a269"),
    ("org.shrimp.test.Slides", "Prezentacje", "Office;Presentation", "Pokazy slajdów", "#e66100"),
    ("org.shrimp.test.Notes", "Notatki", "Office", "Szybkie notatki", "#f5c211"),
    ("org.shrimp.test.Browser", "Przeglądarka", "Network;WebBrowser", "Szybka i prywatna przeglądarka", "#ff7800"),
    ("org.shrimp.test.Mail", "Poczta", "Network;Email", "Poczta e-mail i kalendarz", "#3584e4"),
    ("org.shrimp.test.Chat", "Czat", "Network;InstantMessaging", "Rozmowy z przyjaciółmi", "#9141ac"),
    ("org.shrimp.test.Torrent", "Pobieranie", "Network;FileTransfer", "Pobieranie plików", "#2ec27e"),
    ("org.shrimp.test.Code", "Edytor kodu", "Development;IDE", "Edytor kodu z podpowiedziami", "#1a5fb4"),
    ("org.shrimp.test.Git", "Wersje", "Development;RevisionControl", "Historia zmian w projektach", "#c01c28"),
    ("org.shrimp.test.Api", "Zapytania", "Development", "Testowanie interfejsów sieciowych", "#e66100"),
    ("org.shrimp.test.Term", "Konsola", "System;TerminalEmulator", "Terminal z kartami", "#241f31"),
    ("org.shrimp.test.Disk", "Dyski", "Utility;System", "Zajętość dysku", "#c64600"),
    ("org.shrimp.test.Archive", "Archiwa", "Utility;Archiving", "Pakowanie i rozpakowywanie", "#865e3c"),
    ("org.shrimp.test.Clock", "Zegar", "Utility;Clock", "Budzik, stoper i minutnik", "#3584e4"),
]

LONG = (
    "Program testowy Shrimp Apps. Opis składa się z kilku akapitów, żeby sprawdzić "
    "zawijanie tekstu na stronie aplikacji."
)


def run(*cmd, **kw):
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, check=True, **kw)


def icon(path, color, letter_seed):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    # Gradient w kolorze marki, zaokrąglony kwadrat (ikona testowa).
    run("convert", "-size", "128x128", f"radial-gradient:{color}-#222222", "-alpha", "set", "(", "-size", "128x128",
        "xc:none", "-fill", "white", "-draw", "roundrectangle 8,8 119,119 28,28", ")", "-compose", "DstIn",
        "-composite", path)


def screenshot(path, color, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    base = ["convert", "-size", "1248x702", f"gradient:{color}-#f6f5f4"]
    try:
        run(*base, "-gravity", "center", "-pointsize", "64", "-fill", "white", "-annotate", "0", text, path)
    except subprocess.CalledProcessError:
        run(*base, path)  # bez czcionek ImageMagick nie napisze tekstu: sam gradient


def metainfo(app_id, name, cats, summary, color, version, ts):
    cat_xml = "".join(f"<category>{c}</category>" for c in cats.split(";"))
    shots = "".join(
        f'<screenshot{" type=\"default\"" if i == 1 else ""}>'
        f'<caption>Widok {i}</caption>'
        f'<image type="source" width="1248" height="702">{MEDIA}/{app_id}-{i}.png</image></screenshot>'
        for i in (1, 2)
    )
    return textwrap.dedent(f"""\
        <?xml version="1.0" encoding="UTF-8"?>
        <components version="0.14" origin="flathub">
          <component type="desktop-application">
            <id>{app_id}</id>
            <name>{name}</name>
            <summary>{summary}</summary>
            <description>
              <p>{LONG}</p>
              <p>Najważniejsze funkcje:</p>
              <ul><li>pierwsza funkcja</li><li>druga funkcja</li><li>trzecia funkcja</li></ul>
            </description>
            <developer id="org.shrimp"><name>Zespół Shrimp</name></developer>
            <project_license>GPL-3.0-or-later</project_license>
            <launchable type="desktop-id">{app_id}.desktop</launchable>
            <icon type="cached" height="128" width="128">{app_id}.png</icon>
            <categories>{cat_xml}</categories>
            <keywords><keyword>test</keyword></keywords>
            <url type="homepage">https://example.org/{app_id}</url>
            <screenshots>{shots}</screenshots>
            <releases><release version="{version}" timestamp="{ts}"/></releases>
            <branding><color type="primary" scheme_preference="light">{color}</color>
              <color type="primary" scheme_preference="dark">#303030</color></branding>
            <custom><value key="flathub::verification::verified">true</value></custom>
          </component>
        </components>
        """)


def build_runtime(out, repo):
    d = os.path.join(out, "build", "runtime")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(os.path.join(d, "files", "bin"))
    os.makedirs(os.path.join(d, "usr"), exist_ok=True)
    with open(os.path.join(d, "metadata"), "w") as f:
        f.write(f"[Runtime]\nname={RUNTIME}\nruntime={RUNTIME}/{ARCH}/1\nsdk={RUNTIME}/{ARCH}/1\n")
    with open(os.path.join(d, "files", "bin", "true"), "w") as f:
        f.write("#!/bin/sh\nexit 0\n")
    run("flatpak", "build-export", "--runtime", repo, d, "1")


def build_app(out, repo, entry, version="1.0", ts=1760000000):
    app_id, name, cats, summary, color = entry
    d = os.path.join(out, "build", app_id)
    shutil.rmtree(d, ignore_errors=True)
    files = os.path.join(d, "files")
    os.makedirs(os.path.join(files, "bin"))
    with open(os.path.join(d, "metadata"), "w") as f:
        f.write(f"[Application]\nname={app_id}\nruntime={RUNTIME}/{ARCH}/1\nsdk={RUNTIME}/{ARCH}/1\ncommand=run\n")
    with open(os.path.join(files, "bin", "run"), "w") as f:
        f.write(f"#!/bin/sh\necho {app_id} {version}\n")
    os.chmod(os.path.join(files, "bin", "run"), 0o755)
    # Plik .desktop i ikona (eksportowane do systemu)
    apps_dir = os.path.join(files, "share", "applications")
    os.makedirs(apps_dir)
    with open(os.path.join(apps_dir, f"{app_id}.desktop"), "w") as f:
        f.write(f"[Desktop Entry]\nType=Application\nName={name}\nExec=run\nIcon={app_id}\nCategories={cats};\n")
    ic = os.path.join(files, "share", "icons", "hicolor", "128x128", "apps", f"{app_id}.png")
    icon(ic, color, name)
    # AppStream w formacie, który flatpak build-update-repo zbiera do gałęzi appstream
    xmls = os.path.join(files, "share", "app-info", "xmls")
    os.makedirs(xmls)
    with gzip.open(os.path.join(xmls, f"{app_id}.xml.gz"), "wt", encoding="utf-8") as f:
        f.write(metainfo(app_id, name, cats, summary, color, version, ts))
    ai = os.path.join(files, "share", "app-info", "icons", "flatpak", "128x128")
    os.makedirs(ai)
    shutil.copy(ic, os.path.join(ai, f"{app_id}.png"))
    # export/ jak po flatpak build-finish
    exp = os.path.join(d, "export", "share")
    os.makedirs(os.path.join(exp, "applications"))
    shutil.copy(os.path.join(apps_dir, f"{app_id}.desktop"), os.path.join(exp, "applications"))
    os.makedirs(os.path.join(exp, "icons", "hicolor", "128x128", "apps"))
    shutil.copy(ic, os.path.join(exp, "icons", "hicolor", "128x128", "apps"))
    run("flatpak", "build-export", repo, d, "stable", "--subject", f"{name} {version}")


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("build", "bump"):
        print(__doc__)
        sys.exit(2)
    out = os.path.abspath(sys.argv[2])
    repo = os.path.join(out, "repo")
    if sys.argv[1] == "build":
        shutil.rmtree(out, ignore_errors=True)
        os.makedirs(out)
        run("ostree", "init", "--mode=archive-z2", f"--repo={repo}")
        build_runtime(out, repo)
        for e in APPS:
            build_app(out, repo, e)
            for i in (1, 2):
                screenshot(os.path.join(out, "media", f"{e[0]}-{i}.png"), e[4], f"{e[1]} {i}")
    else:
        app_id = sys.argv[3]
        entry = next(e for e in APPS if e[0] == app_id)
        build_app(out, repo, entry, version="1.1", ts=1770000000)
    run("flatpak", "build-update-repo", "--prune", repo)


if __name__ == "__main__":
    main()
