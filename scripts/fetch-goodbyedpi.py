#!/usr/bin/env python3
"""Fetch the official GoodbyeDPI release and pin it in assets/goodbyedpi/.

Only the Python standard library is used so the script runs the same way on
Windows, Linux and GitHub Actions runners.

Usage:
  fetch-goodbyedpi.py                 re-fetch the version pinned in the lock file
  fetch-goodbyedpi.py --tag 0.2.3rc3  fetch a specific release tag and re-pin
  fetch-goodbyedpi.py --latest        fetch GitHub's "latest" (non-prerelease) release
  fetch-goodbyedpi.py --verify        download the pinned zip and check that the
                                      committed files are byte-identical to it
  fetch-goodbyedpi.py --check         offline: check committed files against the lock
  fetch-goodbyedpi.py --list          print the members of the release zip

The zip is never extracted with extractall(): each wanted member is read by
name and written to a fixed file name, so a hostile archive cannot write
outside assets/goodbyedpi/.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import sys
import tomllib
import urllib.request
import zipfile
from pathlib import Path

REPO = "ValdikSS/GoodbyeDPI"
API = f"https://api.github.com/repos/{REPO}"
ROOT = Path(__file__).resolve().parent.parent
ASSET_DIR = ROOT / "assets" / "goodbyedpi"
LOCK_PATH = ASSET_DIR / "goodbyedpi.lock.toml"
USER_AGENT = "goodbyedpi-toggle-fetch-script"

# Wanted payload: (role, file name inside the x86_64 directory of the zip).
PAYLOAD = [
    ("exe", "goodbyedpi.exe"),
    ("dll", "WinDivert.dll"),
    ("sys", "WinDivert64.sys"),
]
ARCH_DIRS = ("x86_64", "amd64", "x64")


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


def release_info(tag: str | None, latest: bool) -> dict:
    if latest:
        url = f"{API}/releases/latest"
    else:
        url = f"{API}/releases/tags/{tag}"
    return json.loads(http_get(url, "application/vnd.github+json"))


def pick_zip_asset(release: dict) -> dict:
    zips = [a for a in release["assets"] if a["name"].lower().endswith(".zip")]
    if len(zips) != 1:
        names = ", ".join(a["name"] for a in release["assets"])
        sys.exit(f"error: expected exactly one .zip asset in {release['tag_name']}, got: {names}")
    return zips[0]


def member_index(zf: zipfile.ZipFile) -> list[str]:
    return [i.filename for i in zf.infolist() if not i.is_dir()]


def find_payload(members: list[str], wanted: str) -> str:
    hits = []
    for m in members:
        parts = m.replace("\\", "/").split("/")
        if parts[-1].lower() == wanted.lower() and len(parts) >= 2 and parts[-2].lower() in ARCH_DIRS:
            hits.append(m)
    if len(hits) != 1:
        sys.exit(f"error: expected exactly one x86_64 '{wanted}' in the zip, found {hits}")
    return hits[0]


def is_license(member: str) -> bool:
    parts = member.replace("\\", "/").split("/")
    base = parts[-1].lower()
    if base.startswith("license") or base.startswith("copying"):
        return True
    return any(p.lower() in ("licenses", "license") for p in parts[:-1])


def is_reference_script(member: str) -> bool:
    return member.lower().endswith(".cmd")


SAFE_NAME = re.compile(r"[^A-Za-z0-9._-]+")


def flat_name(member: str, prefix: str) -> str:
    """Turn a zip member path into a single safe file name."""
    parts = member.replace("\\", "/").split("/")
    # Drop the top-level "goodbyedpi-x.y.z" directory if present.
    if len(parts) > 1 and parts[0].lower().startswith("goodbyedpi"):
        parts = parts[1:]
    name = "_".join(SAFE_NAME.sub("_", p) for p in parts if p not in ("", ".", ".."))
    return prefix + name


def plan_files(zf: zipfile.ZipFile) -> list[dict]:
    members = member_index(zf)
    files = []
    for role, wanted in PAYLOAD:
        src = find_payload(members, wanted)
        files.append({"role": role, "name": wanted, "source": src})
    for m in members:
        if is_license(m):
            files.append({"role": "license", "name": flat_name(m, "LICENSE-upstream-"), "source": m})
        elif is_reference_script(m):
            files.append({"role": "script", "name": flat_name(m, "upstream-"), "source": m})
    names = [f["name"] for f in files]
    if len(set(n.lower() for n in names)) != len(names):
        sys.exit(f"error: file name collision while flattening: {names}")
    if not any(f["role"] == "license" for f in files):
        sys.exit("error: no license file found in the release zip")
    return files


def toml_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)  # JSON strings are valid TOML basic strings


def write_lock(meta: dict, files: list[dict]) -> None:
    lines = [
        "# Generated by scripts/fetch-goodbyedpi.py. Do not edit by hand.",
        "# build.rs refuses to build if any file below does not match its sha256.",
        "schema = 1",
        f"version = {toml_str(meta['version'])}",
        f"prerelease = {'true' if meta['prerelease'] else 'false'}",
        f"release_url = {toml_str(meta['release_url'])}",
        f"zip_name = {toml_str(meta['zip_name'])}",
        f"zip_url = {toml_str(meta['zip_url'])}",
        f"zip_sha256 = {toml_str(meta['zip_sha256'])}",
        "",
    ]
    for f in files:
        lines += [
            "[[file]]",
            f"role = {toml_str(f['role'])}",
            f"name = {toml_str(f['name'])}",
            f"source = {toml_str(f['source'])}",
            f"sha256 = {toml_str(f['sha256'])}",
            "",
        ]
    LOCK_PATH.write_text("\n".join(lines), encoding="utf-8", newline="\n")


def read_lock() -> dict:
    if not LOCK_PATH.exists():
        sys.exit(f"error: {LOCK_PATH.relative_to(ROOT)} does not exist; run with --tag first")
    return tomllib.loads(LOCK_PATH.read_text(encoding="utf-8"))


def check_local(lock: dict) -> int:
    bad = 0
    for f in lock["file"]:
        p = ASSET_DIR / f["name"]
        if not p.is_file():
            print(f"MISSING  {f['name']}")
            bad += 1
            continue
        got = sha256(p.read_bytes())
        if got != f["sha256"]:
            print(f"MISMATCH {f['name']}: lock {f['sha256']} != file {got}")
            bad += 1
        else:
            print(f"ok       {f['name']}  {got}")
    return bad


def fetch(tag: str | None, latest: bool) -> None:
    rel = release_info(tag, latest)
    asset = pick_zip_asset(rel)
    print(f"release {rel['tag_name']} (prerelease={rel['prerelease']}), asset {asset['name']}")
    data = http_get(asset["browser_download_url"])
    zsha = sha256(data)
    digest = asset.get("digest")  # GitHub publishes "sha256:<hex>" for newer uploads
    if digest and digest.startswith("sha256:") and digest[7:] != zsha:
        sys.exit(f"error: downloaded zip sha256 {zsha} != GitHub digest {digest}")
    zf = zipfile.ZipFile(io.BytesIO(data))
    files = plan_files(zf)

    ASSET_DIR.mkdir(parents=True, exist_ok=True)
    for old in ASSET_DIR.iterdir():
        if old.is_file() and old != LOCK_PATH:
            old.unlink()
    for f in files:
        blob = zf.read(f["source"])
        f["sha256"] = sha256(blob)
        (ASSET_DIR / f["name"]).write_bytes(blob)
        print(f"wrote {f['name']:<48} {f['sha256']}")
    write_lock(
        {
            "version": rel["tag_name"],
            "prerelease": rel["prerelease"],
            "release_url": rel["html_url"],
            "zip_name": asset["name"],
            "zip_url": asset["browser_download_url"],
            "zip_sha256": zsha,
        },
        files,
    )
    print(f"pinned {rel['tag_name']} in {LOCK_PATH.relative_to(ROOT)}")


def verify_upstream(lock: dict) -> int:
    data = http_get(lock["zip_url"])
    zsha = sha256(data)
    if zsha != lock["zip_sha256"]:
        print(f"MISMATCH upstream zip: lock {lock['zip_sha256']} != downloaded {zsha}")
        return 1
    print(f"ok       upstream zip {lock['zip_name']}  {zsha}")
    zf = zipfile.ZipFile(io.BytesIO(data))
    bad = 0
    for f in lock["file"]:
        up = sha256(zf.read(f["source"]))
        if up != f["sha256"]:
            print(f"MISMATCH {f['name']}: upstream {up} != lock {f['sha256']}")
            bad += 1
    return bad + check_local(lock)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--tag", help="release tag to fetch and pin")
    g.add_argument("--latest", action="store_true", help="fetch GitHub's latest non-prerelease release")
    g.add_argument("--verify", action="store_true", help="compare committed files with the pinned upstream zip")
    g.add_argument("--check", action="store_true", help="offline check of committed files against the lock")
    g.add_argument("--list", metavar="TAG", help="list the members of a release zip")
    args = ap.parse_args()

    if args.check:
        sys.exit(1 if check_local(read_lock()) else 0)
    if args.verify:
        sys.exit(1 if verify_upstream(read_lock()) else 0)
    if args.list:
        rel = release_info(args.list, False)
        asset = pick_zip_asset(rel)
        zf = zipfile.ZipFile(io.BytesIO(http_get(asset["browser_download_url"])))
        for i in zf.infolist():
            print(f"{i.file_size:>10}  {i.filename}")
        return
    if args.tag or args.latest:
        fetch(args.tag, args.latest)
    else:
        fetch(read_lock()["version"], False)


if __name__ == "__main__":
    main()
