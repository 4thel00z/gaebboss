# Benchmarks

`bench.py` times gaebboss, pygaeb and raw lxml over a directory of GAEB
files: open and parse, then walk every item reading its Ordnungszahl,
quantity and short text. Best of `--repeat` runs per file; the throughput
is the directory's bytes over the summed best times.

```bash
make develop                                   # gaebboss into .venv
make bench-py GAEBBOSS_CORPUS=~/gaeb-files     # bench.py --repeat 3 --limit 40, then plot.py
```

`bench.py` writes `results.json` next to itself and `plot.py` renders
`results.png` from it. The committed chart is from 2026-11-01 (gaebboss
after 0.1.0, pygaeb 1.21.0, lxml 6.1.3; aarch64 workstation, Cortex-X925,
single-threaded, 40 third-party files, 22 MB).
