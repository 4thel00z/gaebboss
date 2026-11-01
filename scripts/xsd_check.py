#!/usr/bin/env python3
"""Validate GAEB DA XML files against the official schemas with lxml.

The phase schema is picked from the file's namespace (DA83/3.3 and so on);
3.1 files use the single 2009-12 schema. Exit code 1 when any file is
invalid, with every error printed.

Usage: uv run --with lxml python -I scripts/xsd_check.py STANDARD_DIR FILE...
"""

from __future__ import annotations

import pathlib
import re
import sys

from lxml import etree

SCHEMA_CACHE: dict[pathlib.Path, etree.XMLSchema] = {}


def schema_for(standard: pathlib.Path, namespace: str) -> pathlib.Path | None:
    match = re.fullmatch(r"http://www\.gaeb\.de/GAEB_DA_XML/DA([0-9A-Za-z.]+)/(3\.[23])", namespace)
    if match:
        code, version = match.groups()
        candidates = sorted((standard / version).glob(f"GAEB_DA_XML_{code}_{version}_*.xsd"))
        return candidates[0] if candidates else None
    if namespace == "http://www.gaeb.de/GAEB_DA_XML/200407":
        return standard / "3.1" / "GAEB_DA_XML 3.1 2009-12.xsd"
    return None


def main() -> int:
    standard = pathlib.Path(sys.argv[1])
    failures = 0
    for name in sys.argv[2:]:
        path = pathlib.Path(name)
        tree = etree.parse(str(path))
        namespace = etree.QName(tree.getroot()).namespace or ""
        xsd = schema_for(standard, namespace)
        if xsd is None:
            print(f"{path}: no schema for namespace {namespace!r}")
            failures += 1
            continue
        schema = SCHEMA_CACHE.get(xsd)
        if schema is None:
            schema = etree.XMLSchema(etree.parse(str(xsd)))
            SCHEMA_CACHE[xsd] = schema
        if schema.validate(tree):
            print(f"{path}: valid against {xsd.name}")
            continue
        failures += 1
        print(f"{path}: INVALID against {xsd.name}")
        for error in list(schema.error_log)[:20]:
            print(f"  line {error.line}: {error.message}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
