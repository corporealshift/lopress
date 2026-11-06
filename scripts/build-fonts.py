"""Regenerate the bundled editor fonts from the upstream variable fonts.

Writes the static faces and licences to assets/fonts/. floem 0.2's text stack
only uses a face when the weight and style match exactly, so every combination
the editor uses gets its own file.

Newsreader Display's Regular and Italic are really weight 500, marked 400. The
block editor can't ask for a weight, and headings want 500. See
docs/superpowers/specs/2026-11-06-paper-theme-design.md.

Needs `pip install fonttools`. The outputs are committed, so a normal build
never runs this.
"""

import shutil
import sys
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

OUT = Path(__file__).resolve().parent.parent / "assets" / "fonts"

RAW = "https://raw.githubusercontent.com/google/fonts"
NEWSREADER_COMMIT = "8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5"
INSTRUMENT_COMMIT = "0b58fb370093f9a9f4ff785d94405710b79de67c"

SOURCES = {
    "newsreader": (NEWSREADER_COMMIT, "ofl/newsreader/Newsreader[opsz,wght].ttf"),
    "newsreader-italic": (
        NEWSREADER_COMMIT,
        "ofl/newsreader/Newsreader-Italic[opsz,wght].ttf",
    ),
    "instrument": (
        INSTRUMENT_COMMIT,
        "ofl/instrumentsans/InstrumentSans[wdth,wght].ttf",
    ),
    "instrument-italic": (
        INSTRUMENT_COMMIT,
        "ofl/instrumentsans/InstrumentSans-Italic[wdth,wght].ttf",
    ),
}

LICENCES = {
    "OFL-Newsreader.txt": (NEWSREADER_COMMIT, "ofl/newsreader/OFL.txt"),
    "OFL-InstrumentSans.txt": (INSTRUMENT_COMMIT, "ofl/instrumentsans/OFL.txt"),
}

TEXT_OPSZ = 16
DISPLAY_OPSZ = 32

# source, axes, family, subfamily, marked weight, italic, file name
FACES = [
    ("newsreader", {"opsz": TEXT_OPSZ, "wght": 400}, "Newsreader", "Regular", 400, False, "Newsreader-Regular.ttf"),
    ("newsreader-italic", {"opsz": TEXT_OPSZ, "wght": 400}, "Newsreader", "Italic", 400, True, "Newsreader-Italic.ttf"),
    ("newsreader", {"opsz": TEXT_OPSZ, "wght": 700}, "Newsreader", "Bold", 700, False, "Newsreader-Bold.ttf"),
    ("newsreader-italic", {"opsz": TEXT_OPSZ, "wght": 700}, "Newsreader", "Bold Italic", 700, True, "Newsreader-BoldItalic.ttf"),
    ("newsreader", {"opsz": DISPLAY_OPSZ, "wght": 500}, "Newsreader Display", "Regular", 400, False, "NewsreaderDisplay-Regular.ttf"),
    ("newsreader-italic", {"opsz": DISPLAY_OPSZ, "wght": 500}, "Newsreader Display", "Italic", 400, True, "NewsreaderDisplay-Italic.ttf"),
    ("newsreader", {"opsz": DISPLAY_OPSZ, "wght": 700}, "Newsreader Display", "Bold", 700, False, "NewsreaderDisplay-Bold.ttf"),
    ("newsreader-italic", {"opsz": DISPLAY_OPSZ, "wght": 700}, "Newsreader Display", "Bold Italic", 700, True, "NewsreaderDisplay-BoldItalic.ttf"),
    ("instrument", {"wdth": 100, "wght": 400}, "Instrument Sans", "Regular", 400, False, "InstrumentSans-Regular.ttf"),
    ("instrument-italic", {"wdth": 100, "wght": 400}, "Instrument Sans", "Italic", 400, True, "InstrumentSans-Italic.ttf"),
    ("instrument", {"wdth": 100, "wght": 500}, "Instrument Sans", "Medium", 500, False, "InstrumentSans-Medium.ttf"),
    ("instrument", {"wdth": 100, "wght": 600}, "Instrument Sans", "SemiBold", 600, False, "InstrumentSans-SemiBold.ttf"),
    ("instrument", {"wdth": 100, "wght": 700}, "Instrument Sans", "Bold", 700, False, "InstrumentSans-Bold.ttf"),
]

ITALIC_BIT = 1 << 0
BOLD_BIT = 1 << 5
REGULAR_BIT = 1 << 6
MAC_BOLD = 1 << 0
MAC_ITALIC = 1 << 1


def fetch(commit: str, path: str, dest: Path) -> None:
    url = f"{RAW}/{commit}/{urllib.parse.quote(path)}"
    with urllib.request.urlopen(url) as response, dest.open("wb") as out:
        shutil.copyfileobj(response, out)


def set_names(font: TTFont, family: str, subfamily: str) -> None:
    name = font["name"]
    for name_id in (16, 17, 21, 22, 25):
        name.removeNames(nameID=name_id)
    full = f"{family} {subfamily}"
    for platform, encoding, language in ((3, 1, 0x409), (1, 0, 0)):
        name.setName(family, 1, platform, encoding, language)
        name.setName(subfamily, 2, platform, encoding, language)
        name.setName(full, 4, platform, encoding, language)
        name.setName(full.replace(" ", "-"), 6, platform, encoding, language)


def set_style(font: TTFont, weight: int, italic: bool) -> None:
    os2 = font["OS/2"]
    os2.usWeightClass = weight
    selection = os2.fsSelection & ~(ITALIC_BIT | BOLD_BIT | REGULAR_BIT)
    if italic:
        selection |= ITALIC_BIT
    if weight >= 700:
        selection |= BOLD_BIT
    if not italic and weight < 700:
        selection |= REGULAR_BIT
    os2.fsSelection = selection
    font["head"].macStyle = (MAC_BOLD if weight >= 700 else 0) | (
        MAC_ITALIC if italic else 0
    )


def build_face(source: Path, axes: dict, family: str, subfamily: str, weight: int, italic: bool, dest: Path) -> None:
    font = instancer.instantiateVariableFont(TTFont(source), axes)
    set_names(font, family, subfamily)
    set_style(font, weight, italic)
    if "STAT" in font:
        del font["STAT"]
    font.save(dest)


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        downloads = {}
        for key, (commit, path) in SOURCES.items():
            downloads[key] = Path(tmp) / f"{key}.ttf"
            fetch(commit, path, downloads[key])
        for source, axes, family, subfamily, weight, italic, file_name in FACES:
            build_face(downloads[source], axes, family, subfamily, weight, italic, OUT / file_name)
            print(file_name)
    for file_name, (commit, path) in LICENCES.items():
        fetch(commit, path, OUT / file_name)
        print(file_name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
