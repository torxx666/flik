#!/usr/bin/env python3
"""Check real terminal encodings for pane focus, including modal/menu cases.
Run after cargo build: python3 scripts/smoke_pane_focus.py
Override FLIK_BIN to exercise a release binary.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time

binary = os.environ.get("FLIK_BIN", str(Path(__file__).resolve().parents[1] / "target/debug/flik"))
for width in (200, 80):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        first, second = root / "left.log", root / "right.log"
        first.write_text("leftneedle leftmouse\n")
        second.write_text("rightneedle rightf12\n")
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, width, 0, 0))
        process = subprocess.Popen([binary, str(first), str(second)], stdin=slave, stdout=slave, stderr=slave, env=dict(os.environ, TERM="xterm-256color"))
        os.close(slave)
        output = bytearray()
        def drain(seconds=0.25):
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
        try:
            drain(0.6)
            send(b"3leftneedle")  # Leave a draft open on the left.
            send(b"\x1b[1;5C")  # Ctrl+Right, through crossterm's real parser.
            send(b"3rightneedle\r")
            send(b"\x1b[1;5D")  # Ctrl+Left resumes the left draft.
            send(b"\r")
            send(b"\x1d")  # Ctrl+]: focus right.
            send(b"m")  # Keep the right menu open before clicking left.
            send(b"\x1b[<0;11;1M\x1b[<0;11;1m")  # SGR mouse click on left title.
            send(b"3leftmouse\r")
            send(b"\x1b[24~")  # F12: focus right.
            send(b"3rightf12\r")
            session = root / "focus session.json"
            send(b"\x0b")  # Ctrl+K.
            send(f"save {session}\r".encode())
            deadline = time.monotonic() + 8
            while not session.exists() and time.monotonic() < deadline:
                drain(0.1)
            assert session.exists(), bytes(output[-2000:])
            saved = json.loads(session.read_text())
            assert saved["active"] == 1, saved
            assert [rule["pattern"] for rule in saved["panes"][0]["rules"]] == ["leftneedle", "leftmouse"], saved
            assert [rule["pattern"] for rule in saved["panes"][1]["rules"]] == ["rightneedle", "rightf12"], saved
            send(b"\x1b")
            send(b"Q")
            assert process.wait(timeout=5) == 0
            print(f"OK ({width} columns): Ctrl+arrows, Ctrl+], F12, SGR click with menu open, filter drafts retained")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
