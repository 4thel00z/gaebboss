# Benchmarks

`bench.py` times gaebboss, pygaeb and raw lxml over a directory of GAEB
files: open and parse, then walk every item reading its Ordnungszahl,
quantity and short text. Best of `--repeat` runs per file; the throughput
is the directory's bytes over the summed best times.

```bash
make develop                                   # gaebboss into .venv
uv pip install --python .venv/bin/python pygaeb lxml matplotlib
.venv/bin/python -I benchmarks/bench.py ~/gaeb-files --repeat 3 --limit 40
```

`results.json` is written next to the script; `results.png` in this
directory was rendered from it on 2026-11-01 (aarch64 workstation,
Cortex-X925, single-threaded, 40 third-party files, 22 MB).
