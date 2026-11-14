#!/usr/bin/env python3
"""Maintain data/known-releases.toml: the official GoodbyeDPI releases the
tray app is willing to launch.

GoodbyeDPI is NOT bundled with the app. Users unpack the official release
zip themselves and drop goodbyedpi-toggle.exe next to it. Before every start
the app hashes x86_64/goodbyedpi.exe, WinDivert.dll and WinDivert64.sys and
only runs them if all three match one entry recorded here.

Only the Python standard library is used, so this runs the same way on
Windows, Linux and GitHub Actions runners.

Usage:
  fetch-goodbyedpi.py --add 0.2.3rc3   download a release and record it
  fetch-goodbyedpi.py --verify         re-download every recorded release and
                                       check that nothing differs
  fetch-goodbyedpi.py --list 0.2.3rc3  print the members of a release zip

The zip is never extracted to disk: members are read in memory by name.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import sys
import tomllib
import urllib.request
import zipfile
from pathlib import Path

REPO = "ValdikSS/GoodbyeDPI"
API = f"https://api.github.com/repos/{REPO}"
ROOT = Path(__file__).resolve().parent.parent
KNOWN_PATH = ROOT / "data" / "known-releases.toml"
USER_AGENT = "goodbyedpi-toggle-fetch-script"
ARCH_DIRS = ("x86_64", "amd64", "x64")
PAYLOAD = (("exe", "goodbyedpi.exe"), ("dll", "WinDivert.dll"), ("sys", "WinDivert64.sys"))


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def http_get(url: str, accept: str | None = None) -> bytes:
    headers = {"User-Agent": USER_AGENT}
    if accept:
        headers["Accept"] = accept
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read()


def release_info(tag: str) -> dict:
    return json.loads(http_get(f"{API}/releases/tags/{tag}", "application/vnd.github+json"))


def pick_zip_asset(release: dict) -> dict:
    zips = [a for a in release["assets"] if a["name"].lower().endswith(".zip")]
    if len(zips) != 1:
        names = ", ".join(a["name"] for a in release["assets"])
        sys.exit(f"error: expected exactly one .zip asset in {release['tag_name']}, got: {names}")
    return zips[0]


def members(zf: zipfile.ZipFile) -> list[str]:
    return [i.filename for i in zf.infolist() if not i.is_dir()]


def find_payload(names: list[str], wanted: str) -> str:
    hits = []
    for m in names:
        parts = m.replace("\\", "/").split("/")
        if parts[-1].lower() == wanted.lower() and len(parts) >= 2 and parts[-2].lower() in ARCH_DIRS:
            hits.append(m)
    if len(hits) != 1:
        sys.exit(f"error: expected exactly one x86_64 '{wanted}' in the zip, found {hits}")
    return hits[0]


def find_dns_script(names: list[str]) -> str:
    """The launcher the GoodbyeDPI README recommends outside Russia."""
    hits = []
    for m in names:
        base = m.replace("\\", "/").split("/")[-1].lower()
        if (base.endswith(".cmd") and "any_country" in base and "dnsredir" in base
                and "service" not in base):
            hits.append(m)
    if len(hits) != 1:
        sys.exit(f"error: expected exactly one *any_country*dnsredir*.cmd in the zip, found {hits}")
    return hits[0]


def describe(tag: str) -> dict:
    rel = release_info(tag)
    asset = pick_zip_asset(rel)
    data = http_get(asset["browser_download_url"])
    zsha = sha256(data)
    digest = asset.get("digest")  # "sha256:<hex>" on newer GitHub uploads
    if digest and digest.startswith("sha256:") and digest[7:] != zsha:
        sys.exit(f"error: downloaded zip sha256 {zsha} != GitHub digest {digest}")
    zf = zipfile.ZipFile(io.BytesIO(data))
    names = members(zf)
    entry = {
        "version": rel["tag_name"],
        "prerelease": rel["prerelease"],
        "release_url": rel["html_url"],
        "zip_name": asset["name"],
        "zip_url": asset["browser_download_url"],
        "zip_sha256": zsha,
    }
    for role, wanted in PAYLOAD:
        src = find_payload(names, wanted)
        entry[f"{role}_source"] = src
        entry[f"{role}_sha256"] = sha256(zf.read(src))
    script = find_dns_script(names)
    entry["dns_script_source"] = script
    raw = zf.read(script)
    entry["dns_script_sha256"] = sha256(raw)
    entry["dns_script"] = raw.decode("utf-8")  # upstream scripts are ASCII
    return entry


FIELDS = [
    "version", "prerelease", "release_url", "zip_name", "zip_url", "zip_sha256",
    "exe_source", "exe_sha256", "dll_source", "dll_sha256", "sys_source", "sys_sha256",
    "dns_script_source", "dns_script_sha256", "dns_script",
]


def toml_value(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    return json.dumps(v, ensure_ascii=True)  # JSON strings are valid TOML basic strings


def load_known() -> list[dict]:
    if not KNOWN_PATH.exists():
        return []
    doc = tomllib.loads(KNOWN_PATH.read_text(encoding="utf-8"))
    return doc.get("release", [])


def save_known(releases: list[dict]) -> None:
    lines = [
        "# Official GoodbyeDPI releases goodbyedpi-toggle will launch.",
        "# Generated by scripts/fetch-goodbyedpi.py. Do not edit by hand.",
        "# build.rs embeds these hashes; the app refuses files that match none of them.",
        "schema = 1",
        "",
    ]
    for r in releases:
        lines.append("[[release]]")
        lines += [f"{k} = {toml_value(r[k])}" for k in FIELDS]
        lines.append("")
    KNOWN_PATH.parent.mkdir(parents=True, exist_ok=True)
    KNOWN_PATH.write_text("\n".join(lines), encoding="utf-8", newline="\n")


def add(tag: str) -> None:
    entry = describe(tag)
    releases = [r for r in load_known() if r["version"] != entry["version"]]
    releases.append(entry)
    releases.sort(key=lambda r: r["version"])
    save_known(releases)
    for role, _ in PAYLOAD:
        print(f"{entry['version']} {role}: {entry[role + '_sha256']}  ({entry[role + '_source']})")
    print(f"recorded {entry['version']} in {KNOWN_PATH.relative_to(ROOT)}")


def verify() -> int:
    releases = load_known()
    if not releases:
        print(f"{KNOWN_PATH.relative_to(ROOT)} has no releases")
        return 1
    bad = 0
    for r in releases:
        fresh = describe(r["version"])
        for k in FIELDS:
            if fresh[k] != r[k]:
                print(f"MISMATCH {r['version']} {k}")
                bad += 1
        if not bad:
            print(f"ok {r['version']}")
    return bad


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--add", metavar="TAG", action="append", help="record a release tag (repeatable)")
    g.add_argument("--verify", action="store_true", help="re-download recorded releases and compare")
    g.add_argument("--list", metavar="TAG", help="list the members of a release zip")
    args = ap.parse_args()
    if args.verify:
        sys.exit(1 if verify() else 0)
    if args.list:
        asset = pick_zip_asset(release_info(args.list))
        zf = zipfile.ZipFile(io.BytesIO(http_get(asset["browser_download_url"])))
        for i in zf.infolist():
            print(f"{i.file_size:>10}  {i.filename}")
        return
    for tag in args.add:
        add(tag)


if __name__ == "__main__":
    main()
