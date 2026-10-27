"""Download the official vanilla server jar and run its data generator.

Usage:  python tools/fetch_vanilla.py [version]      (default: version in ferrite-protocol)

Output (gitignored):
  vanilla/<version>/server.jar
  vanilla/<version>/generated/reports/{packets,registries,blocks,items,...}.json
  vanilla/<version>/generated/data/minecraft/...   (worldgen, loot tables, recipes, tags)

Nothing produced here may be committed; the repo only ships code that reads it.
"""

import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"


def default_version() -> str:
    src = (ROOT / "crates/ferrite-protocol/src/lib.rs").read_text(encoding="utf-8")
    return re.search(r'GAME_VERSION: &str = "([^"]+)"', src).group(1)


def fetch_json(url: str) -> dict:
    with urllib.request.urlopen(url) as r:
        return json.load(r)


def download(url: str, dest: pathlib.Path, sha1: str) -> None:
    if dest.exists() and hashlib.sha1(dest.read_bytes()).hexdigest() == sha1:
        return
    print(f"downloading {url}")
    with urllib.request.urlopen(url) as r:
        data = r.read()
    if hashlib.sha1(data).hexdigest() != sha1:
        sys.exit(f"sha1 mismatch for {url}")
    dest.write_bytes(data)


def find_java(major: int) -> str:
    """Prefer JAVA_HOME, then PATH; warn if the major version is too old."""
    java = shutil.which("java")
    if os.environ.get("JAVA_HOME"):
        java = str(pathlib.Path(os.environ["JAVA_HOME"]) / "bin" / "java")
    if not java:
        sys.exit("java not found")
    out = subprocess.run([java, "-version"], capture_output=True, text=True).stderr
    found = int(re.search(r'version "(\d+)', out).group(1))
    if found < major:
        sys.exit(f"{version_label} needs Java {major}+, found {found} at {java}")
    return java


def main() -> None:
    global version_label
    version = sys.argv[1] if len(sys.argv) > 1 else default_version()
    version_label = f"Minecraft {version}"
    entry = next((v for v in fetch_json(MANIFEST)["versions"] if v["id"] == version), None)
    if entry is None:
        sys.exit(f"unknown version {version}")
    meta = fetch_json(entry["url"])
    out = ROOT / "vanilla" / version
    out.mkdir(parents=True, exist_ok=True)
    jar = out / "server.jar"
    server = meta["downloads"]["server"]
    download(server["url"], jar, server["sha1"])

    java = find_java(meta.get("javaVersion", {}).get("majorVersion", 21))
    gen = out / "generated"
    if gen.exists():
        shutil.rmtree(gen)
    subprocess.run(
        [java, "-DbundlerMainClass=net.minecraft.data.Main", "-jar", str(jar),
         "--reports", "--server", "--output", str(gen)],
        cwd=out, check=True,
    )
    print(f"ok: {gen}")


if __name__ == "__main__":
    main()
