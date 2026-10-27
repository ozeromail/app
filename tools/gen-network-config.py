#!/usr/bin/env python3
"""Generates Android's network security config from the certificate verifier's.

gen-network-config.py <verifier aar> <res dir> writes xml/network_security_config.xml.

The verifier's config lets Android fetch revocation lists over plain HTTP from the CAs that
publish them so; an app's own config replaces it whole. This keeps its list, kept in step
with the verifier's version, and adds trust in the certificate authorities installed on the
device: company mail servers often use one, by a work profile or by the person.
"""
import sys
import zipfile
from pathlib import Path

NAME = "network_security_config.xml"
BASE = '<base-config cleartextTrafficPermitted="false" />'
TRUST = """<base-config cleartextTrafficPermitted="false">
    <trust-anchors>
      <certificates src="system" />
      <certificates src="user" />
    </trust-anchors>
  </base-config>"""


def main(aar, res):
    with zipfile.ZipFile(aar) as archive:
        config = archive.read(f"res/xml/{NAME}").decode()
    if config.count(BASE) != 1:
        sys.exit(f"{aar}: {NAME} has no plain {BASE}; check what the verifier changed")
    out = Path(res) / "xml"
    out.mkdir(parents=True, exist_ok=True)
    (out / NAME).write_text(config.replace(BASE, TRUST))


if __name__ == "__main__":
    main(*sys.argv[1:])
