#!/usr/bin/env python3
"""Exercise the six investigation workflows in a real pseudo-terminal (Unix).
Run after cargo build: python3 scripts/smoke_investigation.py
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import tempfile
import termios
import time

binary = Path(__file__).resolve().parents[1] / "target/debug/flik"
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    first, second = root / "api.log", root / "worker.log"
    first.write_text("\n".join(json.dumps(event) for event in [
        {"timestamp": "2026-10-21T10:00:00Z", "level": "info", "msg": "before"},
        {"timestamp": "2026-10-21T10:01:00Z", "level": "error", "trace_id": "req-42", "password": "PRIVATE_ONE", "msg": "failure"},
        {"timestamp": "2026-10-21T10:02:00Z", "level": "info", "msg": "after"},
    ]) + "\n")
    second.write_text(json.dumps({"timestamp": "2026-10-21T10:01:30Z", "trace_id": "req-42", "token": "PRIVATE_TWO", "msg": "retry"}) + "\n")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 45, 200, 0, 0))
    process = subprocess.Popen([str(binary), str(first), str(second)], stdin=slave, stdout=slave, stderr=slave, env=dict(os.environ, TERM="xterm-256color"))
    os.close(slave)
    output = bytearray()
    def drain(seconds=0.3):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.03)[0]:
                try:
                    output.extend(os.read(master, 65536))
                except OSError:
                    break
    def send(data):
        os.write(master, data)
        drain()
    def wait_file(path):
        until = time.monotonic() + 8
        while not path.exists() and time.monotonic() < until:
            drain(0.1)
        assert path.exists(), clean()[-3000:]

    def clean():
        return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", output).decode(errors="replace")
    def command(text):
        send(b"\x0b")  # Ctrl+K
        send(text.encode() + b"\r")
        drain(0.3)
    try:
        drain(0.6)
        send(b"3")
        send(b'(level:error OR status>=500) AND NOT msg:"normal output"\r')
        send(b"\x07")  # Ctrl+G
        assert "Context" in clean()
        send(b"\x1b")
        command("time 2026-10-21T10:00:00Z..2026-10-21T10:02:00Z")
        command("jump 2026-10-21T10:01:00Z")
        command("untime")
        command("merge")
        assert "Merged snapshot" in clean()
        command("trace req-42")
        session = root / "saved session.json"
        command(f"save {session}")
        wait_file(session)
        command(f"load {session}")
        incident = root / "incident.jsonl"
        command(f"export {incident}")
        wait_file(incident)
        data = incident.read_text()
        assert "PRIVATE_ONE" not in data and "PRIVATE_TWO" not in data
        events = [json.loads(line) for line in data.splitlines()]
        assert events[0]["type"] == "flik_investigation"
        assert len(events[0]["sources"]) == 2 and len(events) == 5
        send(b"\x1b")
        send(b"Q")
        assert process.wait(timeout=5) == 0
        print("OK: Boolean filter, context, time range/jump, merge/trace, session save/load, redacted export, clean exit")
    except Exception:
        Path("/tmp/flik-investigation-smoke.txt").write_text(clean())
        raise
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
