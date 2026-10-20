"""Regenerate the lopress app icon rasters from the SVG masters.

Writes assets/icon/lopress.ico (16/24 from lopress-small.svg, 32 and up from
lopress.svg) and assets/icon/lopress-256.png (the runtime window icon).

Needs only the Python standard library and the `resvg` CLI
(`cargo install resvg`). The outputs are committed, so a normal build never
runs this.
"""

import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ICON_DIR = Path(__file__).resolve().parent.parent / "assets" / "icon"
FULL = ICON_DIR / "lopress.svg"
SMALL = ICON_DIR / "lopress-small.svg"
ICO_SIZES = [(16, SMALL), (24, SMALL), (32, FULL), (48, FULL), (64, FULL), (256, FULL)]


def render(svg: Path, size: int, out: Path) -> bytes:
    subprocess.run(
        ["resvg", "--width", str(size), "--height", str(size), str(svg), str(out)],
        check=True,
    )
    return out.read_bytes()


def pack_ico(images: list[tuple[int, bytes]]) -> bytes:
    """ICONDIR + one ICONDIRENTRY per image + PNG-encoded image data."""
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries, blobs = b"", b""
    for size, png in images:
        dim = 0 if size >= 256 else size  # 0 means 256 in an ICONDIRENTRY
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset)
        blobs += png
        offset += len(png)
    return header + entries + blobs


def main() -> int:
    if shutil.which("resvg") is None:
        print("resvg not found; install it with `cargo install resvg`", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory() as tmp:
        images = [
            (size, render(svg, size, Path(tmp) / f"{size}.png")) for size, svg in ICO_SIZES
        ]
    (ICON_DIR / "lopress.ico").write_bytes(pack_ico(images))
    (ICON_DIR / "lopress-256.png").write_bytes(dict(images)[256])
    print(f"wrote lopress.ico ({', '.join(str(s) for s, _ in images)}) and lopress-256.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
