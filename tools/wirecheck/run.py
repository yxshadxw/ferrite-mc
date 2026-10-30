"""Join a running Ferrite server (offline mode) like a client, record every clientbound packet of the
configuration and play phases, then decode each one with the REAL vanilla 26.3 codecs
(tools/wirecheck/PacketCheck.java, run against vanilla/<ver>/lib). Reports packets the vanilla client
would reject (exception, or bytes left over) — the same failures players see as "Network Protocol Error".

Usage:  python tools/wirecheck/run.py [--port 25565] [--seconds 3] [--name WireCheck]
Needs Java 25 (FERRITE_JAVA25 or the Temurin path below) and `python tools/fetch_vanilla.py` done once.
Nothing Mojang-owned is committed: the checker only runs against the locally extracted jar.
"""

import argparse
import json
import os
import pathlib
import socket
import struct
import subprocess
import sys
import time
import uuid
import zlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
VERSION = "26.3"
# 26.x class files need Java 25+; JAVA_HOME may point at an older JDK, so use a dedicated variable.
JAVA_HOME = os.environ.get("FERRITE_JAVA25") or "C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot"
PACKETS = json.loads((ROOT / f"vanilla/{VERSION}/generated/reports/packets.json").read_text(encoding="utf-8"))


def pid(state, bound, name):
    return PACKETS[state][bound][name]["protocol_id"]


def varint(v):
    v &= 0xFFFFFFFF
    out = b""
    while True:
        b = v & 0x7F
        v >>= 7
        out += bytes([b | (0x80 if v else 0)])
        if not v:
            return out


def string(s):
    b = s.encode()
    return varint(len(b)) + b


class Conn:
    def __init__(self, port):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=5)
        self.buf = b""
        self.compress = False

    def send(self, packet_id, body=b""):
        data = varint(packet_id) + body
        if self.compress:
            data = varint(0) + data
        self.s.sendall(varint(len(data)) + data)

    def _read_varint(self):
        v = shift = 0
        while True:
            while not self.buf:
                chunk = self.s.recv(65536)
                if not chunk:
                    raise EOFError("server closed the connection")
                self.buf += chunk
            b = self.buf[0]
            self.buf = self.buf[1:]
            v |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                return v

    def recv(self):
        n = self._read_varint()
        while len(self.buf) < n:
            chunk = self.s.recv(65536)
            if not chunk:
                raise EOFError("server closed the connection")
            self.buf += chunk
        frame, self.buf = self.buf[:n], self.buf[n:]
        if self.compress:
            # data length prefix
            i, shift, dl = 0, 0, 0
            while True:
                b = frame[i]
                i += 1
                dl |= (b & 0x7F) << shift
                shift += 7
                if not b & 0x80:
                    break
            frame = frame[i:] if dl == 0 else zlib.decompress(frame[i:])
        return frame


def split_id(frame):
    v = shift = i = 0
    while True:
        b = frame[i]
        i += 1
        v |= (b & 0x7F) << shift
        shift += 7
        if not b & 0x80:
            return v, frame[i:]


def capture(port, seconds, name):
    c = Conn(port)
    c.send(pid("handshake", "serverbound", "minecraft:intention"),
           varint(777) + string("localhost") + struct.pack(">H", port) + varint(2))
    c.send(pid("login", "serverbound", "minecraft:hello"),
           string(name) + uuid.uuid3(uuid.NAMESPACE_DNS, name).bytes)
    records = []  # (phase, full frame incl. id)
    phase = "login"
    deadline = None
    sent_chat = False
    names = {s: {v["protocol_id"]: k for k, v in PACKETS[s]["clientbound"].items()} for s in PACKETS if "clientbound" in PACKETS[s]}
    while True:
        if deadline and time.time() > deadline:
            break
        try:
            frame = c.recv()
        except (EOFError, socket.timeout) as e:
            records.append((phase, b"", f"[connection ended: {e}]"))
            break
        i, body = split_id(frame)
        n = names[phase].get(i, f"unknown_{i}")
        if phase == "login":
            if n == "minecraft:login_compression":
                c.compress = True
            elif n == "minecraft:login_finished":
                c.send(pid("login", "serverbound", "minecraft:login_acknowledged"))
                phase = "configuration"
            elif n == "minecraft:login_disconnect":
                records.append((phase, frame, n))
                break
            continue
        records.append((phase, frame, n))
        if phase == "configuration":
            if n == "minecraft:select_known_packs":
                c.send(pid("configuration", "serverbound", "minecraft:select_known_packs"),
                       varint(1) + string("minecraft") + string("core") + string(VERSION))
            elif n == "minecraft:keep_alive":
                c.send(pid("configuration", "serverbound", "minecraft:keep_alive"), body[:8])
            elif n == "minecraft:finish_configuration":
                c.send(pid("configuration", "serverbound", "minecraft:finish_configuration"))
                phase = "play"
                deadline = time.time() + seconds
        elif phase == "play":
            if n == "minecraft:keep_alive":
                c.send(pid("play", "serverbound", "minecraft:keep_alive"), body[:8])
            elif n == "minecraft:chunk_batch_finished":
                c.send(pid("play", "serverbound", "minecraft:chunk_batch_received"), struct.pack(">f", 25.0))
            elif n == "minecraft:player_position":
                # 26.3: the confirmation echoes the resulting position (x, y, z, yaw, pitch).
                tid, rest = split_varint(body)
                x, y, z = struct.unpack(">ddd", rest[:24])
                yaw, pitch = struct.unpack(">ff", rest[48:56])
                c.send(pid("play", "serverbound", "minecraft:accept_teleportation"),
                       varint(tid) + struct.pack(">dddff", x, y, z, yaw, pitch))
                if not sent_chat:
                    # Unsigned chat (offline path): no signature, empty last-seen update.
                    sent_chat = True
                    c.send(pid("play", "serverbound", "minecraft:chat"),
                           string("wirecheck chat")
                           + struct.pack(">qq", int(time.time() * 1000), 0)
                           + b"\x00"              # no signature
                           + b"\x00"              # last-seen offset 0
                           + b"\x00\x00\x00"      # 20 acknowledged bits
                           + b"\x00")             # checksum
    return records, names


def split_varint(b):
    v = shift = i = 0
    while True:
        x = b[i]
        i += 1
        v |= (x & 0x7F) << shift
        shift += 7
        if not x & 0x80:
            return v, b[i:]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=25565)
    ap.add_argument("--seconds", type=float, default=8)
    ap.add_argument("--name", default="WireCheck")
    a = ap.parse_args()
    records, _ = capture(a.port, a.seconds, a.name)
    out = ROOT / "target" / "wirecheck"
    out.mkdir(parents=True, exist_ok=True)
    dump = out / "packets.txt"
    with dump.open("w", encoding="utf-8") as f:
        for phase, frame, n in records:
            f.write(f"{phase} {n} {frame.hex() or '-'}\n")
    print(f"captured {len(records)} packets -> {dump}")
    java = pathlib.Path(JAVA_HOME) / "bin"
    lib = ROOT / f"vanilla/{VERSION}/lib"
    cp = os.pathsep.join([str(lib / f"server-{VERSION}.jar")] + [str(p) for p in (lib / "libraries").rglob("*.jar")])
    classes = out / "classes"
    classes.mkdir(exist_ok=True)
    src = pathlib.Path(__file__).with_name("PacketCheck.java")
    subprocess.run([str(java / "javac"), "-d", str(classes), "-cp", cp, str(src)], check=True)
    r = subprocess.run([str(java / "java"), "-cp", os.pathsep.join([str(classes), cp]), "PacketCheck", str(dump)],
                       capture_output=True, text=True, encoding="utf-8")
    lines = [l for l in r.stdout.splitlines() if l.startswith(("OK", "BAD", "SKIP", "SUMMARY"))]
    print("\n".join(lines))
    if r.returncode != 0 or any(l.startswith("BAD") for l in lines):
        sys.exit(1)


if __name__ == "__main__":
    main()
