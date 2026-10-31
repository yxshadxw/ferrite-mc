"""Generate the vanilla fixture world used by world/light/server tests.

Usage:  python tools/make_fixture_world.py [version]   (run tools/fetch_vanilla.py first)

Starts the official server jar once with a fixed seed, force-loads chunks
(-3..18, -3..18), saves repeatedly until the region files stop changing, then stops it
cleanly. Output (gitignored):
  vanilla/<version>/fixture-world/world/...   (level.dat, region files)

Chunk generation is asynchronous and a feature in a neighbouring chunk (for example a
tree overhanging from the next chunk) can modify a chunk after it was written, leaving
stale sky light in the saved file. Waiting for two identical flushes instead of a fixed
delay guarantees the save that tests read is a fully generated, fully lit world.

Tests use the inner chunks (0..15, 0..15). The three-chunk margin keeps their light
comparable: vanilla can store stale light for a chunk near the edge of generation when a
later neighbour places blocks (for example a tree overhanging from the next chunk).
Only the inner chunks are compared; the margin itself may hold stale light.

The seed and settings are fixed so tests that assert on this world (spawn position,
stored chunks) are reproducible on every machine.
"""

import hashlib
import shutil
import subprocess
import sys
import threading
import time

from fetch_vanilla import ROOT, default_version

SEED = "ferrite-fixture"
# Flush, then flush again and compare: equal region files mean every chunk is
# generated, lit and stable. The timeout only guards against a stuck server.
GENERATE_POLL_SECONDS = 10
GENERATE_MAX_SECONDS = 300
SPAWN = (0, 80, 112)  # asserted by ferrite-server's fixture tests
# Tests compare chunks 0..15 with a three-chunk halo, so -3..18 must all be generated.
# forceload allows at most 256 chunks per command, so the area is force-loaded in
# z-strips.
FORCELOAD_COMMANDS = (
    "forceload add -48 -48 303 79",  # chunks x -3..18, z -3..4
    "forceload add -48 80 303 207",  # chunks x -3..18, z 5..12
    "forceload add -48 208 303 303",  # chunks x -3..18, z 13..18
)
PROPERTIES = f"""level-name=world
level-seed={SEED}
online-mode=false
server-port=25599
enable-rcon=false
spawn-protection=0
view-distance=4
simulation-distance=4
"""


def region_digest(out) -> str:
    """Hash the region files; a changing digest means chunks are still generating."""
    region_dir = out / "world/dimensions/minecraft/overworld/region"
    digest = hashlib.sha256()
    for path in sorted(region_dir.glob("*.mca")):
        digest.update(path.name.encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def main() -> None:
    version = sys.argv[1] if len(sys.argv) > 1 else default_version()
    base = ROOT / "vanilla" / version
    jar = base / "server.jar"
    if not jar.exists():
        sys.exit(f"missing {jar}; run python tools/fetch_vanilla.py first")
    out = base / "fixture-world"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    (out / "eula.txt").write_text("eula=true\n", encoding="utf-8")
    (out / "server.properties").write_text(PROPERTIES, encoding="utf-8")
    proc = subprocess.Popen(
        ["java", "-Xmx2G", "-jar", str(jar), "nogui"],
        cwd=out,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )

    send_lock = threading.Lock()

    def send(command: str) -> None:
        with send_lock:
            proc.stdin.write(command + "\n")
            proc.stdin.flush()

    def flush() -> None:
        send("save-all flush")

    started = time.monotonic()
    last_digest = None
    generating = False
    for line in proc.stdout:
        if "Done (" in line:
            # Tests read chunk (0, 0) / r.0.0.mca, which spawn may not cover.
            # forceload allows at most 256 chunks per command.
            for command in FORCELOAD_COMMANDS:
                send(command)
        elif "Marked" in line and "to be force loaded" in line:
            if not generating:
                # Tests join at spawn and expect chunk (0, 7) first; pin it inside
                # the forced area.
                send(f"setworldspawn {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
                generating = True
                flush()
        elif "Saved the game" in line and generating:
            digest = region_digest(out)
            elapsed = time.monotonic() - started
            if digest == last_digest:
                print(f"ok: world stable after {elapsed:.0f} s", flush=True)
                generating = False
                send("stop")
            elif elapsed >= GENERATE_MAX_SECONDS:
                print(
                    f"warning: world still changing after {elapsed:.0f} s; "
                    "fixture light may be stale",
                    file=sys.stderr,
                )
                generating = False
                send("stop")
            else:
                last_digest = digest
                threading.Timer(GENERATE_POLL_SECONDS, flush).start()
        elif "Unable to" in line or "Too many chunks" in line:
            send("stop")
            sys.exit(f"forceload failed: {line.strip()}")
    if proc.wait() != 0:
        sys.exit(f"vanilla server exited with {proc.returncode}")
    print(f"ok: {out / 'world'} (seed {SEED!r})")


if __name__ == "__main__":
    main()
