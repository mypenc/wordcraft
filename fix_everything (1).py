#!/usr/bin/env python3
"""Apply every Android fix to the Wordcraft repo, then re-vendor winit 0.30.13.

Run from anywhere:  python3 fix_everything.py   (the repo root is the script's folder)
Safe to run twice.
"""
import base64
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zlib
from pathlib import Path

WINIT_VERSION = "0.30.13"
REPO = Path(__file__).resolve().parent
WINIT_PATCH = REPO / "android" / "patches" / f"winit-{WINIT_VERSION}-android-input.patch"
WINIT_DEST = REPO / "android" / "vendor" / "winit"
WINIT_URL = f"https://static.crates.io/crates/winit/winit-{WINIT_VERSION}.crate"

# Winit patch: Backspace/Enter/IME fixes and the Window-level soft-keyboard state.
WINIT_PATCH_B64 = """
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
"""

# Rust edits: table-picker drag-across, soft keyboard only while typing, Enter always on the bar.
RUST_PATCH_B64 = """
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
Um/O/gs/++Mf
"""


def decode(b64: str) -> bytes:
    return zlib.decompress(base64.b64decode("".join(b64.split())))


def apply_rust_edits() -> None:
    patch_bytes = decode(RUST_PATCH_B64)
    with tempfile.NamedTemporaryFile(suffix=".patch", delete=False) as f:
        f.write(patch_bytes)
        patch_file = f.name
    already = subprocess.run(
        ["patch", "-p1", "-R", "--dry-run", "-i", patch_file], cwd=REPO, capture_output=True, text=True
    )
    if already.returncode == 0:
        print("Rust edits already applied")
    else:
        result = subprocess.run(["patch", "-p1", "-i", patch_file], cwd=REPO, capture_output=True, text=True)
        sys.stdout.write(result.stdout)
        if result.returncode != 0:
            sys.stderr.write(result.stderr)
            raise SystemExit("Rust patch failed; check the files under crates/ui-egui/src")
        print("Rust edits applied")
    Path(patch_file).unlink()


def fix_gesture() -> None:
    f = REPO / "crates" / "ui-egui" / "src" / "touch" / "gesture.rs"
    text = f.read_text()
    bad = "if let Mode::Pending { origin, t0 } = self.mode {"
    if bad in text:
        f.write_text(text.replace(bad, "if let Mode::Pending { origin, t0, .. } = self.mode {"))
        print("gesture.rs pattern fixed")
    else:
        print("gesture.rs already fine")


def vendor_winit() -> None:
    WINIT_PATCH.parent.mkdir(parents=True, exist_ok=True)
    WINIT_PATCH.write_bytes(decode(WINIT_PATCH_B64))

    if WINIT_DEST.exists():
        shutil.rmtree(WINIT_DEST)
    WINIT_DEST.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        crate = tmp / "winit.crate"
        urllib.request.urlretrieve(WINIT_URL, crate)
        with tarfile.open(crate, "r:gz") as tar:
            tar.extractall(tmp)
        shutil.move(str(tmp / f"winit-{WINIT_VERSION}"), str(WINIT_DEST))

    result = subprocess.run(
        ["patch", "-p1", "-d", str(WINIT_DEST), "-i", str(WINIT_PATCH)], capture_output=True, text=True
    )
    sys.stdout.write(result.stdout)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit("winit patch failed")
    print(f"winit {WINIT_VERSION} patched in {WINIT_DEST}")


def main() -> int:
    apply_rust_edits()
    fix_gesture()
    vendor_winit()
    print("Done. Commit and build.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
