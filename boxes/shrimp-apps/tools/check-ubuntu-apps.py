#!/usr/bin/env python3
"""Sprawdza data/ubuntu-apps.json na żywo (CI, z internetem):

- liczba instalacji z Flathub (api/v2/stats/<flathub>) >= min_downloads,
- aplikacja NIE jest sprawdzona na Flathub (sprawdzone są w zwykłym katalogu sklepu),
- pakiet jest opublikowany w Ubuntu 24.04 (noble, amd64) według Launchpad.

Wypisuje tabelę z liczbami i źródłem; kod wyjścia 1, gdy któraś aplikacja nie spełnia zasad.
"""
import json
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

DATA = Path(__file__).resolve().parent.parent / "data" / "ubuntu-apps.json"
UA = {"User-Agent": "shrimp-apps-ci", "Accept": "application/json"}


def get(url, tries=3):
    for i in range(tries):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            err = e
        except Exception as e:  # sieć: ponów
            err = e
        time.sleep(2 * (i + 1))
    raise RuntimeError(f"{url}: {err}")


def installs(app_id):
    s = get(f"https://flathub.org/api/v2/stats/{app_id}")
    return (s or {}).get("installs_total")


def verified(app_id):
    v = get(f"https://flathub.org/api/v2/verification/{app_id}/status")
    if v is not None and "verified" in v:
        return bool(v["verified"])
    a = get(f"https://flathub.org/api/v2/appstream/{app_id}") or {}
    return str((a.get("metadata") or {}).get("flathub::verification::verified", "")).lower() == "true"


def in_noble(pkg):
    q = urllib.parse.urlencode({
        "ws.op": "getPublishedBinaries",
        "binary_name": pkg,
        "exact_match": "true",
        "status": "Published",
        "distro_arch_series": "https://api.launchpad.net/1.0/ubuntu/noble/amd64",
    })
    r = get(f"https://api.launchpad.net/1.0/ubuntu/+archive/primary?{q}") or {}
    return bool(r.get("entries"))


def main():
    d = json.loads(DATA.read_text())
    need = int(d["min_downloads"])
    bad = []
    seen = set()
    print(f"{'pakiet':20} {'Flathub':36} {'instalacje':>12}  sprawdzona  noble")
    for a in d["apps"]:
        for k in ("package", "exec", "name", "flathub", "category"):
            if not a.get(k):
                bad.append(f"{a}: brak pola {k}")
        if a["package"] in seen:
            bad.append(f"{a['package']}: powtórzony")
        seen.add(a["package"])
        n = installs(a["flathub"])
        v = verified(a["flathub"])
        u = in_noble(a["package"])
        print(f"{a['package']:20} {a['flathub']:36} {n if n is not None else '?':>12}  {'tak' if v else 'nie':10}  {'tak' if u else 'nie'}")
        if n is None or n < need:
            bad.append(f"{a['package']}: {n} instalacji na Flathub ({a['flathub']}), potrzeba {need}")
        if v:
            bad.append(f"{a['package']}: {a['flathub']} jest sprawdzona na Flathub, należy do zwykłego katalogu")
        if not u:
            bad.append(f"{a['package']}: brak pakietu w Ubuntu 24.04 (noble)")
    if bad:
        print("\n".join(["", "Nie spełniają zasad:"] + [f"  - {b}" for b in bad]))
        return 1
    print(f"\nok: {len(d['apps'])} aplikacji")
    return 0


if __name__ == "__main__":
    sys.exit(main())
