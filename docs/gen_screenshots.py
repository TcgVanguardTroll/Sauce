#!/usr/bin/env python3
"""Render real `sauce` command output to SVG terminal screenshots.

No external dependencies: runs the binary with colour forced on, parses the
ANSI SGR codes the `colored` crate emits, and emits a self-contained SVG with
GitHub-dark styling and window chrome. Re-run after UI changes:

    cargo build && python3 docs/gen_screenshots.py
"""

import html
import os
import re
import subprocess
import tempfile

# Resolve the binary (prefer release).
BIN = next(
    (p for p in ("target/release/sauce", "target/debug/sauce") if os.path.exists(p)),
    "target/debug/sauce",
)

# GitHub-dark palette.
FG = {
    30: "#6e7681", 31: "#ff7b72", 32: "#3fb950", 33: "#d29922",
    34: "#58a6ff", 35: "#bc8cff", 36: "#39c5cf", 37: "#b1bac4",
    90: "#6e7681", 91: "#ffa198", 92: "#56d364", 93: "#e3b341",
    94: "#79c0ff", 95: "#d2a8ff", 96: "#56d4dd", 97: "#f0f6fc",
}
DEFAULT_FG = "#c9d1d9"
BG = "#0d1117"
HEADER = "#161b22"

CHAR_W = 8.4
LINE_H = 20.0
PAD = 16.0
HEADER_H = 36.0
FONT = "ui-monospace, 'SF Mono', 'DejaVu Sans Mono', Menlo, Consolas, monospace"

SGR = re.compile(r"\x1b\[([0-9;]*)m")


class Style:
    def __init__(self):
        self.fg = None
        self.bold = False
        self.underline = False

    def apply(self, params):
        for p in params:
            if p in (0, ""):
                self.fg, self.bold, self.underline = None, False, False
            elif p == 1:
                self.bold = True
            elif p == 4:
                self.underline = True
            elif p in FG:
                self.fg = FG[p]


def parse_line(line):
    """Yield (text, Style) runs for one line of ANSI-coded text."""
    style = Style()
    pos = 0
    runs = []
    for m in SGR.finditer(line):
        if m.start() > pos:
            runs.append((line[pos:m.start()], _snapshot(style)))
        params = [int(x) if x else 0 for x in m.group(1).split(";")]
        style.apply(params)
        pos = m.end()
    if pos < len(line):
        runs.append((line[pos:], _snapshot(style)))
    return runs


def _snapshot(style):
    s = Style()
    s.fg, s.bold, s.underline = style.fg, style.bold, style.underline
    return s


def to_svg(title, text):
    lines = text.rstrip("\n").split("\n")
    cols = max((visible_len(strip_ansi(l)) for l in lines), default=20)
    width = PAD * 2 + cols * CHAR_W
    height = HEADER_H + PAD + len(lines) * LINE_H + PAD

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" '
        f'height="{height:.0f}" viewBox="0 0 {width:.0f} {height:.0f}" '
        f'font-family="{FONT}" font-size="13.5">',
        f'<rect width="{width:.0f}" height="{height:.0f}" rx="8" fill="{BG}"/>',
        f'<rect width="{width:.0f}" height="{HEADER_H:.0f}" rx="8" fill="{HEADER}"/>',
        f'<rect y="{HEADER_H - 8:.0f}" width="{width:.0f}" height="8" fill="{HEADER}"/>',
        '<circle cx="18" cy="18" r="6" fill="#ff5f56"/>',
        '<circle cx="38" cy="18" r="6" fill="#ffbd2e"/>',
        '<circle cx="58" cy="18" r="6" fill="#27c93f"/>',
        f'<text x="{width/2:.0f}" y="22" fill="#8b949e" font-size="12" '
        f'text-anchor="middle">{html.escape(title)}</text>',
    ]
    y = HEADER_H + PAD + 4
    for line in lines:
        out.append(f'<text x="{PAD:.0f}" y="{y:.1f}" xml:space="preserve">')
        x = PAD
        for raw, style in parse_line(line):
            txt = html.escape(raw)
            attrs = [f'fill="{style.fg or DEFAULT_FG}"']
            if style.bold:
                attrs.append('font-weight="bold"')
            if style.underline:
                attrs.append('text-decoration="underline"')
            out.append(
                f'<tspan x="{x:.1f}" {" ".join(attrs)}>{txt}</tspan>'
            )
            x += visible_len(raw) * CHAR_W
        out.append("</text>")
        y += LINE_H
    out.append("</svg>\n")
    return "\n".join(out)


def strip_ansi(s):
    return SGR.sub("", s)


def visible_len(s):
    return len(s)


def run(env, args):
    res = subprocess.run(
        [BIN, *args], capture_output=True, text=True,
        env={**env, "CLICOLOR_FORCE": "1"},
    )
    return res.stdout


def main():
    os.makedirs("docs", exist_ok=True)
    with tempfile.TemporaryDirectory() as data:
        env = {**os.environ, "XDG_DATA_HOME": data}
        run(env, ["add", "Taiga Aisaka", "Rin Tohsaka", "Kurisu Makise",
                  "Rei Ayanami", "Yuki Nagato", "Levi Ackerman"])

        shots = [
            ("profile", "sauce profile", ["profile"]),
            ("recommend", "sauce recommend --limit 6", ["recommend", "--limit", "6"]),
            ("clusters", "sauce clusters", ["clusters"]),
            ("find", 'sauce find --vibe-like "Levi Ackerman" --limit 6',
             ["find", "--vibe-like", "Levi Ackerman", "--limit", "6"]),
        ]
        for name, title, args in shots:
            svg = to_svg(title, run(env, args))
            path = f"docs/{name}.svg"
            with open(path, "w", encoding="utf-8") as f:
                f.write(svg)
            print(f"wrote {path}")


if __name__ == "__main__":
    main()
