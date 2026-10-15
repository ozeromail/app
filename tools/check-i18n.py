#!/usr/bin/env python3
"""Checks that keys are snake_case and every locale has exactly the keys of en.json."""
import json
import re
import sys
from pathlib import Path

I18N = Path(__file__).resolve().parent.parent / "i18n"
KEY = re.compile(r"[a-z][a-z0-9_]*")


def main():
    source = json.loads((I18N / "en.json").read_text(encoding="utf-8"))
    errors = []
    errors += [f"en.json: '{k}' must be snake_case" for k in source if not KEY.fullmatch(k)]
    for path in sorted(I18N.glob("*.json")):
        if path.name == "en.json":
            continue
        strings = json.loads(path.read_text(encoding="utf-8"))
        for key in sorted(source.keys() - strings.keys()):
            errors.append(f"{path.name}: missing '{key}'")
        for key in sorted(strings.keys() - source.keys()):
            errors.append(f"{path.name}: '{key}' is not in en.json")
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        sys.exit(1)
    print(f"i18n ok: {len(source)} keys")


if __name__ == "__main__":
    main()
