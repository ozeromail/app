#!/usr/bin/env python3
"""Generates Android strings.xml from i18n/*.json into the given res directory."""
import json
import shutil
import sys
from pathlib import Path
from xml.sax.saxutils import escape

I18N = Path(__file__).resolve().parent.parent / "i18n"


def android_value(text):
    text = escape(text).replace("\\", "\\\\").replace("'", "\\'").replace('"', '\\"')
    text = text.replace("\n", "\\n").replace("\t", "\\t")
    return "\\" + text if text[:1] in ("@", "?") else text


def values_dir(locale):
    """en → values, ru → values-ru, pt-BR → values-pt-rBR, zh-Hans → values-b+zh+Hans."""
    if locale == "en":
        return "values"
    parts = locale.split("-")
    if len(parts) == 1:
        return f"values-{locale}"
    if len(parts) == 2 and len(parts[1]) == 2:
        return f"values-{parts[0]}-r{parts[1]}"
    return "values-b+" + "+".join(parts)


def main():
    res = Path(sys.argv[1])
    shutil.rmtree(res, ignore_errors=True)
    for src in sorted(I18N.glob("*.json")):
        strings = json.loads(src.read_text(encoding="utf-8"))
        lines = ["<resources>"]
        lines += [f'    <string name="{k}">{android_value(v)}</string>' for k, v in strings.items()]
        lines.append("</resources>\n")
        out = res / values_dir(src.stem) / "strings.xml"
        out.parent.mkdir(parents=True)
        out.write_text("\n".join(lines), encoding="utf-8")


if __name__ == "__main__":
    main()
