#!/usr/bin/env python3
"""Generates platform strings from i18n/*.json.

Android: gen-strings.py <res dir> writes values*/strings.xml.
Apple:   gen-strings.py --apple <dir> writes <locale>.lproj/Localizable.strings.
"""
import json
import re
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


def android(strings, locale):
    lines = ["<resources>"]
    lines += [f'    <string name="{k}">{android_value(v)}</string>' for k, v in strings.items()]
    lines.append("</resources>\n")
    return [(Path(values_dir(locale)) / "strings.xml", "\n".join(lines))]


def apple_value(text):
    """Android's %1$s and %1$d are %1$@ and %1$ld for Foundation's String(format:)."""
    return re.sub(r"%(\d+\$)?([sd])", lambda m: "%" + (m.group(1) or "") + ("@" if m.group(2) == "s" else "ld"), text)


def apple(strings, locale):
    # A JSON string literal is also a valid .strings value.
    lines = [f'"{k}" = {json.dumps(apple_value(v), ensure_ascii=False)};' for k, v in strings.items()]
    name = "".join(
        f'"{key}" = {json.dumps(strings[k], ensure_ascii=False)};\n'
        for key, k in [("CFBundleDisplayName", "app_name"), ("NSContactsUsageDescription", "contacts_usage")]
    )
    folder = Path(f"{locale}.lproj")
    return [(folder / "Localizable.strings", "\n".join(lines) + "\n"), (folder / "InfoPlist.strings", name)]


def main():
    args = sys.argv[1:]
    generate = apple if args[0] == "--apple" else android
    out_dir = Path(args[-1])
    if generate is android:
        # Android owns the whole generated res dir; on Apple it is the app bundle.
        shutil.rmtree(out_dir, ignore_errors=True)
    for src in sorted(I18N.glob("*.json")):
        for path, text in generate(json.loads(src.read_text(encoding="utf-8")), src.stem):
            out = out_dir / path
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
