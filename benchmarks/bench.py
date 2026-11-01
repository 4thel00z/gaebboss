#!/usr/bin/env python3
"""Time gaebboss against pygaeb and raw lxml on a directory of GAEB files.

Two operations every library can do: parse the file, and walk every item
reading its Ordnungszahl, quantity and short text. Writes results.json.

Usage: python benchmarks/bench.py DIR [--repeat K] [--limit N]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import statistics
import time


def gaebboss_parse(path: pathlib.Path) -> int:
    import gaebboss

    return len(gaebboss.Document(path).warnings)


def gaebboss_items(path: pathlib.Path) -> int:
    import gaebboss

    return sum(len(i.oz) + len(i.short_text) for i in gaebboss.Document(path).items())


def pygaeb_parse(path: pathlib.Path) -> int:
    from pygaeb import GAEBParser

    return len(GAEBParser.parse(str(path)).warnings or [])


def pygaeb_items(path: pathlib.Path) -> int:
    from pygaeb import GAEBParser

    doc = GAEBParser.parse(str(path))
    return sum(len(i.full_oz or "") + len(i.short_text or "") for i in doc.iter_items())


def lxml_parse(path: pathlib.Path) -> int:
    from lxml import etree

    return len(etree.parse(str(path)).getroot())


def lxml_items(path: pathlib.Path) -> int:
    from lxml import etree

    root = etree.parse(str(path)).getroot()
    ns = root.nsmap.get(None, "")
    total = 0
    for item in root.iter(f"{{{ns}}}Item"):
        total += len(item.get("RNoPart", ""))
        for span in item.iter(f"{{{ns}}}span"):
            total += len(span.text or "")
    return total


LIBRARIES = {
    "gaebboss": (gaebboss_parse, gaebboss_items),
    "pygaeb": (pygaeb_parse, pygaeb_items),
    "lxml": (lxml_parse, lxml_items),
}


def time_one(fn, path: pathlib.Path, repeat: int) -> float | None:
    best = None
    for _ in range(repeat):
        started = time.perf_counter()
        try:
            fn(path)
        except Exception:
            return None
        elapsed = time.perf_counter() - started
        best = elapsed if best is None else min(best, elapsed)
    return best


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("dir")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    files = sorted(p for p in pathlib.Path(args.dir).rglob("*") if p.suffix.upper().startswith(".X"))
    if args.limit:
        files = files[: args.limit]
    total_bytes = sum(p.stat().st_size for p in files)
    results: dict[str, dict] = {}
    for name, (parse, items) in LIBRARIES.items():
        for op, fn in (("parse", parse), ("items", items)):
            times = [time_one(fn, p, args.repeat) for p in files]
            ok = [t for t in times if t is not None]
            if not ok:
                continue
            total = sum(ok)
            results[f"{name}/{op}"] = {
                "files": len(ok),
                "failed": len(times) - len(ok),
                "total_s": round(total, 3),
                "mb_per_s": round(total_bytes / 1e6 / total, 1) if len(ok) == len(times) else None,
                "median_ms": round(statistics.median(ok) * 1e3, 2),
            }
            print(f"{name:10} {op:6} {len(ok):4} files  {total:8.2f}s  {results[f'{name}/{op}']['mb_per_s']} MB/s")
    pathlib.Path("benchmarks/results.json").write_text(json.dumps({"files": len(files), "bytes": total_bytes, "results": results}, indent=2))


if __name__ == "__main__":
    main()
