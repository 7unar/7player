#!/usr/bin/env python3
"""Render a `tmux capture-pane -e` dump into a PNG.

This is a development tool for the README screenshots. It implements just
enough of the ANSI SGR / cursor subset that tmux emits, then paints the cell
grid with Pillow. Colours are truecolor when the terminal emitted `38;2;...`
sequences (the repo's screenshot script enables tmux RGB support).
"""

from __future__ import annotations

import argparse
import re
import unicodedata
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

THEMES = {
    "tokyo-night": ("#1a1b26", "#c0caf5"),
    "catppuccin-mocha": ("#1e1e2e", "#cdd6f4"),
    "dracula": ("#282a36", "#f8f8f2"),
    "nord": ("#2e3440", "#eceff4"),
    "gruvbox-dark": ("#282828", "#ebdbb2"),
    "solarized-dark": ("#002b36", "#eee8d5"),
    "rose-pine": ("#232136", "#e0def4"),
    "one-dark": ("#282c34", "#abb2bf"),
    "catppuccin-latte": ("#eff1f5", "#4c4f69"),
    "solarized-light": ("#fdf6e3", "#586e75"),
}

ANSI_RE = re.compile(r"\x1b\[([0-9;:?]*)([@-~])")
OSC_RE = re.compile(r"\x1b\].*?(?:\x07|\x1b\\)", re.DOTALL)

BASE16 = [
    "#000000", "#cd3131", "#0dbc79", "#e5e510",
    "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
    "#666666", "#f14c4c", "#23d18b", "#f5f543",
    "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
]

SYMBOLS = set("◉♪♫⌕▤⏸⇄⟲↻▶▸♥●○◈✓…")


@dataclass
class Cell:
    ch: str = " "
    fg: tuple[int, int, int] | None = None
    bg: tuple[int, int, int] | None = None
    bold: bool = False
    reverse: bool = False


def hex_rgb(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return int(value[0:2], 16), int(value[2:4], 16), int(value[4:6], 16)


def xterm256(index: int) -> tuple[int, int, int]:
    if index < 16:
        return hex_rgb(BASE16[index])
    if index < 232:
        index -= 16
        r, g, b = index // 36, (index % 36) // 6, index % 6
        conv = [0, 95, 135, 175, 215, 255]
        return conv[r], conv[g], conv[b]
    level = 8 + (index - 232) * 10
    return level, level, level


def cell_width(ch: str) -> int:
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def parse_ansi(data: str, cols: int, rows: int) -> list[list[Cell]]:
    grid = [[Cell() for _ in range(cols)] for _ in range(rows)]
    fg: tuple[int, int, int] | None = None
    bg: tuple[int, int, int] | None = None
    bold = False
    reverse = False
    x = 0
    y = 0
    i = 0
    n = len(data)

    def put(ch: str) -> None:
        nonlocal x
        width = cell_width(ch)
        if width == 0:
            return
        if y < rows and x < cols:
            grid[y][x] = Cell(ch=ch, fg=fg, bg=bg, bold=bold, reverse=reverse)
            if width == 2 and x + 1 < cols:
                grid[y][x + 1] = Cell(ch="", fg=fg, bg=bg, bold=bold, reverse=reverse)
        x += width

    while i < n:
        ch = data[i]
        if ch == "\x1b":
            if data.startswith("\x1b]", i):
                match = OSC_RE.match(data, i)
                if match:
                    i = match.end()
                    continue
            match = ANSI_RE.match(data, i)
            if not match:
                i += 2
                continue
            params, final = match.group(1), match.group(2)
            i = match.end()
            if final == "m":
                values = [int(p) if p else 0 for p in params.replace(":", ";").split(";")]
                if not values:
                    values = [0]
                j = 0
                while j < len(values):
                    value = values[j]
                    if value == 0:
                        fg = bg = None
                        bold = reverse = False
                    elif value == 1:
                        bold = True
                    elif value == 22:
                        bold = False
                    elif value == 7:
                        reverse = True
                    elif value == 27:
                        reverse = False
                    elif 30 <= value <= 37:
                        fg = hex_rgb(BASE16[value - 30])
                    elif 90 <= value <= 97:
                        fg = hex_rgb(BASE16[value - 90 + 8])
                    elif value == 39:
                        fg = None
                    elif 40 <= value <= 47:
                        bg = hex_rgb(BASE16[value - 40])
                    elif 100 <= value <= 107:
                        bg = hex_rgb(BASE16[value - 100 + 8])
                    elif value == 49:
                        bg = None
                    elif value in (38, 48):
                        color = None
                        if j + 2 < len(values) and values[j + 1] == 5:
                            color = xterm256(values[j + 2])
                            j += 2
                        elif j + 4 < len(values) and values[j + 1] == 2:
                            color = (values[j + 2], values[j + 3], values[j + 4])
                            j += 4
                        if value == 38:
                            fg = color
                        else:
                            bg = color
                    j += 1
            elif final == "K":
                mode = int(params) if params else 0
                if y < rows:
                    start = 0 if mode == 2 else x
                    end = cols if mode in (0, 2) else min(x + 1, cols)
                    for cx in range(max(0, start), max(0, end)):
                        grid[y][cx] = Cell()
            # Cursor movement is not emitted by capture-pane in practice.
            continue
        if ch == "\n":
            y += 1
            x = 0
            i += 1
            continue
        if ch == "\r":
            x = 0
            i += 1
            continue
        if ch == "\t":
            x = (x // 8 + 1) * 8
            i += 1
            continue
        if ch == "\b":
            x = max(0, x - 1)
            i += 1
            continue
        if ch < " ":
            i += 1
            continue
        put(ch)
        i += 1
    # tmux trims trailing spaces from captured lines. If the app painted a
    # full-width background, the last styled cell's bg is the row background;
    # propagate it to the trimmed tail so gradient rows do not look striped.
    for row in grid:
        last_bg = None
        for cell in row:
            if cell.bg is not None:
                last_bg = cell.bg
        if last_bg is not None:
            for cell in row:
                if cell.bg is None:
                    cell.bg = last_bg

    return grid


def load_font(path: Path, size: int, index: int = 0) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(path), size=size, index=index)


def render(
    grid: list[list[Cell]],
    out: Path,
    theme: str,
    scale: int,
    font_size: int,
    bg_override: tuple[int, int, int] | None = None,
) -> None:
    default_bg = bg_override or hex_rgb(THEMES[theme][0])
    default_fg = hex_rgb(THEMES[theme][1])
    cols = len(grid[0])
    rows = len(grid)

    latin_path = Path("/mnt/c/Windows/Fonts/consola.ttf")
    latin_bold_path = Path("/mnt/c/Windows/Fonts/consolab.ttf")
    cjk_path = Path("/mnt/c/Windows/Fonts/msyh.ttc")
    cjk_bold_path = Path("/mnt/c/Windows/Fonts/msyhbd.ttc")
    symbol_path = Path("/mnt/c/Windows/Fonts/seguisym.ttf")
    if not latin_path.exists():
        raise SystemExit(f"font not found: {latin_path}")
    size = font_size * scale
    latin = load_font(latin_path, size)
    latin_bold = load_font(latin_bold_path, size) if latin_bold_path.exists() else latin
    symbol = load_font(symbol_path, size) if symbol_path.exists() else latin

    cell_w = max(1, int(round(size * 0.64)))
    cell_h = max(1, int(round(size * 1.45)))
    cjk_size = max(1, int(round(cell_w * 2 * 0.9)))
    cjk = load_font(cjk_path, cjk_size) if cjk_path.exists() else latin
    cjk_bold = load_font(cjk_bold_path, cjk_size) if cjk_bold_path.exists() else cjk
    pad = 12 * scale
    width = cols * cell_w + pad * 2
    height = rows * cell_h + pad * 2
    image = Image.new("RGB", (width, height), default_bg)
    draw = ImageDraw.Draw(image)

    for row, line in enumerate(grid):
        for col, cell in enumerate(line):
            bg = cell.bg if cell.bg is not None else default_bg
            fg = cell.fg if cell.fg is not None else default_fg
            if cell.reverse:
                bg, fg = fg, bg
            if bg != default_bg and not (cell.ch == "" and cell.bg is not None):
                x0 = pad + col * cell_w
                y0 = pad + row * cell_h
                draw.rectangle([x0, y0, x0 + cell_w, y0 + cell_h], fill=bg)

    for row, line in enumerate(grid):
        for col, cell in enumerate(line):
            if not cell.ch or cell.ch == " ":
                continue
            bg = cell.bg if cell.bg is not None else default_bg
            fg = cell.fg if cell.fg is not None else default_fg
            if cell.reverse:
                bg, fg = fg, bg
            x0 = pad + col * cell_w
            y0 = pad + row * cell_h + cell_h // 2
            if cell.ch in "█▀▄":
                top = pad + row * cell_h
                if cell.ch == "█":
                    draw.rectangle([x0, top, x0 + cell_w, top + cell_h], fill=fg)
                elif cell.ch == "▀":
                    draw.rectangle([x0, top, x0 + cell_w, top + cell_h // 2], fill=fg)
                else:
                    draw.rectangle(
                        [x0, top + cell_h // 2, x0 + cell_w, top + cell_h],
                        fill=fg,
                    )
                continue
            code = ord(cell.ch)
            if 0x2800 <= code <= 0x28FF:
                # Render Braille patterns as actual dots so screenshots do not
                # depend on the host font having U+28xx glyphs.
                bits = code - 0x2800
                top = pad + row * cell_h
                radius = max(1, cell_w // 6)
                positions = [
                    (0.25, 0.125, 0x01),
                    (0.25, 0.375, 0x02),
                    (0.25, 0.625, 0x04),
                    (0.75, 0.125, 0x08),
                    (0.75, 0.375, 0x10),
                    (0.75, 0.625, 0x20),
                    (0.25, 0.875, 0x40),
                    (0.75, 0.875, 0x80),
                ]
                for px, py, bit in positions:
                    if bits & bit:
                        cx = x0 + int(cell_w * px)
                        cy = top + int(cell_h * py)
                        draw.ellipse(
                            [cx - radius, cy - radius, cx + radius, cy + radius],
                            fill=fg,
                        )
                continue
            wide = cell_width(cell.ch) == 2
            if wide:
                font = cjk_bold if cell.bold else cjk
            elif cell.ch in SYMBOLS:
                font = symbol
            else:
                font = latin_bold if cell.bold else latin
            draw.text((x0, y0), cell.ch, font=font, fill=fg, anchor="lm")

    image.save(out)
    print(f"wrote {out} ({width}x{height})")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--theme", default="tokyo-night", choices=sorted(THEMES))
    parser.add_argument("--bg", default=None, help="override default background, e.g. #0b0a18")
    parser.add_argument("--cols", type=int, default=120)
    parser.add_argument("--rows", type=int, default=40)
    parser.add_argument("--scale", type=int, default=2)
    parser.add_argument("--font-size", type=int, default=14)
    args = parser.parse_args()

    raw = args.input.read_text(encoding="utf-8", errors="replace")
    grid = parse_ansi(OSC_RE.sub("", raw), args.cols, args.rows)
    bg = hex_rgb(args.bg) if args.bg else None
    render(grid, args.output, args.theme, args.scale, args.font_size, bg)


if __name__ == "__main__":
    main()
