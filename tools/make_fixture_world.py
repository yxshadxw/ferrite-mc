"""Generate the vanilla fixture world used by world/light/server tests.

Usage:  python tools/make_fixture_world.py [version]   (run tools/fetch_vanilla.py first)

Starts the official server jar once with a fixed seed, waits until spawn chunks are
generated, force-loads chunks (-2..17, -2..17) and saves, then stops it cleanly. Output
(gitignored):
  vanilla/<version>/fixture-world/world/...   (level.dat, region files)

Tests use the inner chunks (0..15, 0..15). The two-chunk margin keeps their light
comparable: vanilla can store stale light for a chunk at the edge of generation when a
later neighbour places blocks (for example a tree overhanging from the next chunk).

The seed and settings are fixed so tests that assert on this world (spawn position,
stored chunks) are reproducible on every machine.
"""

import shutil
import subprocess
import sys
import threading

from fetch_vanilla import ROOT, default_version

SEED = "ferrite-fixture"
GENERATE_SECONDS = 30
SPAWN = (0, 80, 112)  # asserted by ferrite-server's fixture tests
# Tests use chunks 0..15; -2..17 leaves a two-chunk generated margin around them.
FORCELOAD_COMMANDS = (
    "forceload add -32 -32 159 287",  # chunks x -2..9, z -2..17
    "forceload add 160 -32 287 287",  # chunks x 10..17, z -2..17
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

    def send(command: str) -> None:
        proc.stdin.write(command + "\n")
        proc.stdin.flush()

    scheduled = False
    for line in proc.stdout:
        if "Done (" in line:
            # Tests read chunk (0, 0) / r.0.0.mca, which spawn may not cover.
            # forceload allows at most 256 chunks per command.
            for command in FORCELOAD_COMMANDS:
                send(command)
        elif "Marked" in line and "to be force loaded" in line:
            if not scheduled:
                # Tests join at spawn and expect chunk (0, 7) first; pin it inside
                # the forced area.
                send(f"setworldspawn {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
                # Chunks generate asynchronously; the timer gives them time before saving.
                threading.Timer(GENERATE_SECONDS, send, ["save-all flush"]).start()
                scheduled = True
        elif "Saved the game" in line:
            send("stop")
        elif "Unable to" in line or "Too many chunks" in line:
            send("stop")
            sys.exit(f"forceload failed: {line.strip()}")
    if proc.wait() != 0:
        sys.exit(f"vanilla server exited with {proc.returncode}")
    print(f"ok: {out / 'world'} (seed {SEED!r})")


if __name__ == "__main__":
    main()
