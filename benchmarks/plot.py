#!/usr/bin/env python3
"""Render benchmarks/results.png from benchmarks/results.json."""

from __future__ import annotations

import json
import pathlib

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt

HERE = pathlib.Path(__file__).resolve().parent
COLORS = {"gaebboss": "#2a78d6", "pygaeb": "#eb6834", "lxml": "#1baf7a"}
PANELS = {
    "parse": "Open and parse",
    "items": "Walk every item: Ordnungszahl, quantity, short text",
}
BACKGROUND = "#fcfcfb"
RULE = "#dad9d4"
MUTED = "#555555"


def throughput(value: float) -> str:
    return f"{value:.0f} MB/s" if value >= 10 else f"{value:.1f} MB/s"


def main() -> None:
    data = json.loads((HERE / "results.json").read_text())
    results = data["results"]
    figure, axes = plt.subplots(1, 2, figsize=(12, 3.6), dpi=160, facecolor=BACKGROUND)
    figure.suptitle(
        f"gaebboss vs. pygaeb and lxml over {data['files']} real-world GAEB files "
        f"({data['bytes'] / 1e6:.0f} MB)",
        x=0.01,
        ha="left",
        fontweight="bold",
    )
    for axis, (op, title) in zip(axes, PANELS.items()):
        values = [results[f"{name}/{op}"]["mb_per_s"] for name in COLORS]
        if None in values:
            raise RuntimeError(f"{op}: a library failed on some files, no throughput to plot")
        bars = axis.barh(list(COLORS), values, color=list(COLORS.values()), height=0.55)
        axis.invert_yaxis()
        axis.bar_label(bars, labels=[throughput(v) for v in values], padding=6)
        axis.set_title(title, loc="left")
        axis.set_xlabel("MB/s, higher is faster", color=MUTED)
        axis.set_xlim(0, max(values) * 1.22)
        axis.set_facecolor(BACKGROUND)
        axis.grid(axis="x", color=RULE)
        axis.set_axisbelow(True)
        for side in ("top", "right", "left"):
            axis.spines[side].set_visible(False)
        axis.spines["bottom"].set_color(RULE)
        axis.tick_params(axis="x", colors=MUTED)
        axis.tick_params(axis="y", left=False)
    figure.tight_layout()
    figure.savefig(HERE / "results.png", facecolor=BACKGROUND)


if __name__ == "__main__":
    main()
