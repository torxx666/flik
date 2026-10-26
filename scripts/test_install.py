#!/usr/bin/env python3
"""Offline installer contract tests: synthetic releases, all targets, no network."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

INSTALLER = Path(__file__).resolve().parent / "install.sh"
TARGETS = {
    ("Linux", "x86_64"): "x86_64-unknown-linux-musl",
    ("Linux", "aarch64"): "aarch64-unknown-linux-musl",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Darwin", "arm64"): "aarch64-apple-darwin",
}

with tempfile.TemporaryDirectory(prefix="flik-installer-tests-") as directory:
    root = Path(directory)
    mocks = root / "commands"
    mocks.mkdir()
    (mocks / "uname").write_text('#!/bin/sh\ncase "$1" in -s) printf "%s\\n" "$TEST_OS" ;; -m) printf "%s\\n" "$TEST_ARCH" ;; *) exit 1 ;; esac\n')
    (mocks / "curl").write_text('''#!/usr/bin/env python3
import os, pathlib, sys
args = sys.argv[1:]
url = args[-1]
assert url.startswith("https://github.com/example/flik/"), url
if "--head" in args:
    print("https://github.com/example/flik/releases/tag/v0.1.0", end="")
else:
    output = args[args.index("--output") + 1]
    source = pathlib.Path(os.environ["FIXTURE_ROOT"]) / url.rsplit("/", 1)[1]
    pathlib.Path(output).write_bytes(source.read_bytes())
''')
    for command in mocks.iterdir():
        command.chmod(0o755)
    base_env = dict(os.environ, PATH=f"{mocks}:{os.environ['PATH']}",
                    FLIK_REPO="example/flik", FIXTURE_ROOT=str(root),
                    TEST_OS="Linux", TEST_ARCH="x86_64", TMPDIR=str(root))

    def fixture(target, payload=b'#!/bin/sh\nprintf "flik 0.1.0\\n"\n'):
        archive = root / f"flik-v0.1.0-{target}.tar.gz"
        with tarfile.open(archive, "w:gz") as package:
            info = tarfile.TarInfo("flik")
            info.size = len(payload)
            info.mode = 0o755
            package.addfile(info, io.BytesIO(payload))
            license_bytes = b"Synthetic license fixture\n"
            license_info = tarfile.TarInfo("LICENSE")
            license_info.size = len(license_bytes)
            package.addfile(license_info, io.BytesIO(license_bytes))
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (root / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
        return archive

    def run(name, expected=0, **overrides):
        dest = root / f"install {name}"
        env = dict(base_env, FLIK_INSTALL_DIR=str(dest), **overrides)
        result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True, text=True)
        assert result.returncode == expected, (name, result.returncode, result.stdout, result.stderr)
        assert not list(root.glob("flik-install.*")), "temporary directory leaked"
        assert not list(root.glob("install */.flik*")), "staging file leaked"
        return dest, result

    for (system, arch), target in TARGETS.items():
        fixture(target)
        dest, _ = run(f"{system}-{arch}", TEST_OS=system, TEST_ARCH=arch)
        assert (dest / "flik").is_file()
        assert (dest / "flik.LICENSE").read_text() == "Synthetic license fixture\n"
        assert subprocess.check_output([str(dest / "flik"), "--version"], text=True).strip() == "flik 0.1.0"

    archive = fixture(TARGETS[("Linux", "x86_64")])
    run("pinned", FLIK_VERSION="0.1.0")
    run("bad-version", expected=1, FLIK_VERSION="v1.0.0;bad")
    run("missing-repo", expected=1, FLIK_REPO="")
    run("bad-repo", expected=1, FLIK_REPO="example/flik/extra")
    run("unsupported", expected=1, TEST_OS="Windows", TEST_ARCH="x86_64")
    archive.write_bytes(archive.read_bytes() + b"corrupt")
    dest, result = run("checksum", expected=1)
    assert "SHA-256 mismatch" in result.stderr
    assert not (dest / "flik").exists()
    fixture(TARGETS[("Linux", "x86_64")])
    dest, _ = run("already-exists")
    (dest / "flik").write_text("preserve this file")
    run("already-exists", expected=1)
    assert (dest / "flik").read_text() == "preserve this file"
    run("already-exists", FLIK_FORCE="1")
    assert subprocess.check_output([str(dest / "flik"), "--version"], text=True).strip() == "flik 0.1.0"
    link_dir = root / "install symlink"
    link_dir.mkdir()
    outside = root / "outside"
    outside.write_text("preserved")
    (link_dir / "flik").symlink_to(outside)
    run("symlink", expected=1, FLIK_FORCE="1")
    assert outside.read_text() == "preserved"
    fixture(TARGETS[("Linux", "x86_64")], b'#!/bin/sh\nprintf "flik 9.9.9\\n"\n')
    dest, result = run("wrong-binary", expected=1)
    assert "Binary version does not match" in result.stderr
    assert not (dest / "flik").exists()

print("OK: 4 platforms, latest/pinned, checksum rejection, validation, version check, overwrite protection and cleanup")
