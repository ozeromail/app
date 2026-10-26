#!/usr/bin/env python3
"""Fails if a core crate depends on something its layer may not use.

The layer is the crate's top folder in core/:
  domain/    std and serde only
  app/       domain only
  adapters/  domain and external crates; gmail, graph and ews may also use http
  api/       anything
"""
import json
import subprocess
import sys
from pathlib import Path

CORE = Path(__file__).resolve().parent.parent / "core"
LAYERS = ("domain", "app", "adapters", "api")
HTTP_USERS = {"ozero-gmail", "ozero-graph", "ozero-ews"}


def allowed(crate, layer, dep, internal):
    if layer == "domain":
        return dep == "serde"
    if layer == "app":
        return dep == "ozero-domain"
    if layer == "adapters":
        if dep == "ozero-http":
            return crate in HTTP_USERS
        return dep == "ozero-domain" or dep not in internal
    return True


def main():
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=CORE))
    internal = {p["name"] for p in meta["packages"]}
    errors = []
    for pkg in meta["packages"]:
        parts = Path(pkg["manifest_path"]).parent.relative_to(CORE).parts
        layer = parts[0] if parts else ""
        if layer not in LAYERS:
            errors.append(f"{pkg['name']}: put it under one of core/{{{','.join(LAYERS)}}}/")
            continue
        for dep in pkg["dependencies"]:
            if dep.get("kind") is None and not allowed(pkg["name"], layer, dep["name"], internal):
                errors.append(f"{pkg['name']} ({layer}) must not depend on {dep['name']}")
    for e in errors:
        print(f"layer violation: {e}", file=sys.stderr)
    if errors:
        sys.exit(1)
    print(f"layers ok: {len(internal)} crates")


if __name__ == "__main__":
    main()
