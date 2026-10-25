# -*- coding: utf-8 -*-
"""gesturecraft 字体子集化：
保留 GB2312 一级常用字(3755) + 源码出现的全部字符 + ASCII 可打印区 + 常用符号，
从 C:\Windows\Fonts\msyh.ttc 生成 assets/font-subset.ttf（运行时 include_bytes! 嵌入）。
"""
import glob
import os
import sys

from fontTools import subset

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC_DIR = os.path.join(ROOT, "src")
OUT = os.path.join(ROOT, "assets", "font-subset.ttf")
FONT = r"C:\Windows\Fonts\msyh.ttc"

chars = set()

# 1) ASCII 可打印区
chars.update(chr(c) for c in range(0x20, 0x7F))

# 2) 源码里出现的全部非 ASCII 字符（含 UI 文案、箭头、✓✗×＋ 等）
for path in glob.glob(os.path.join(SRC_DIR, "*.rs")):
    with open(path, "r", encoding="utf-8") as f:
        chars.update(f.read())

# 3) GB2312 一级常用字（区 0xB0-0xD7，拼音序 3755 字）
for lead in range(0xB0, 0xD8):
    for trail in range(0xA1, 0xFF):
        try:
            chars.add(bytes([lead, trail]).decode("gb2312"))
        except UnicodeDecodeError:
            pass

# 4) 常用补充符号（二级常用字若出现在输入里会退回默认字体，属已知取舍）
chars.update("，。、；：？！“”‘’（）《》〈〉【】—…·～℃％‰①②③④⑤⑥⑦⑧⑨⑩")
chars.update("↑↓←→↖↗↙↘✓✗×＋±§※")

unicodes = sorted({ord(c) for c in chars if ord(c) >= 0x20})
print(f"charset size: {len(unicodes)}")

args = [
    FONT,
    f"--unicodes={','.join(f'U+{u:04X}' for u in unicodes)}",
    "--font-number=0",
    f"--output-file={OUT}",
    "--layout-features=",
    "--no-hinting",
    "--desubroutinize",
    "--drop-tables+=DSIG",
]
subset.main(args)
print("written:", OUT, os.path.getsize(OUT), "bytes")
