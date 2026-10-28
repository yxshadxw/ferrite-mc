"""Download official plugin releases into the ignored compatibility fixture directory."""
from __future__ import annotations

import json
import os
from pathlib import Path
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
DESTINATION = ROOT / "plugins-test"
HEADERS = {"User-Agent": "Ferrite-compatibility-test", "Accept": "application/vnd.github+json"}


def read_json(url: str) -> dict:
    with urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=30) as response:
        return json.load(response)


def release_asset(repository: str, matches) -> str:
    release = read_json(f"https://api.github.com/repos/{repository}/releases/latest")
    assets = [asset["browser_download_url"] for asset in release["assets"] if matches(asset["name"])]
    if len(assets) != 1:
        raise RuntimeError(f"Expected one jar for {repository}, got {len(assets)}")
    return assets[0]


def main() -> None:
    urls = {
        "Vault.jar": release_asset("MilkBowl/Vault", lambda name: name == "Vault.jar"),
        "LuckPerms.jar": read_json("https://metadata.luckperms.net/data/all")["downloads"]["bukkit"],
        "EssentialsX.jar": release_asset("EssentialsX/Essentials", lambda name: name.startswith("EssentialsX-") and name.endswith(".jar")),
        "PlaceholderAPI.jar": release_asset("PlaceholderAPI/PlaceholderAPI", lambda name: name.startswith("PlaceholderAPI-") and name.endswith(".jar")),
    }
    DESTINATION.mkdir(exist_ok=True)
    for name, url in urls.items():
        print(f"Downloading {name} from {url}", flush=True)
        temporary = None
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": HEADERS["User-Agent"]}), timeout=60) as response:
                with tempfile.NamedTemporaryFile(dir=DESTINATION, delete=False) as output:
                    temporary = Path(output.name)
                    while chunk := response.read(1024 * 1024):
                        output.write(chunk)
            os.replace(temporary, DESTINATION / name)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
