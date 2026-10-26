#!/usr/bin/env python3
"""Static scientific figure from the sealed growth summary; no new measurements."""
import json
import os
import tempfile
from pathlib import Path
os.environ.setdefault("MPLCONFIGDIR", str(Path(tempfile.gettempdir()) / "zil123-matplotlib"))
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

repo = Path(__file__).resolve().parents[3]
rows = json.loads((repo / "evidence/impl/123/results/growth.json").read_text())
fig, axes = plt.subplots(2, 3, figsize=(11.6, 6.5), constrained_layout=True)
for column, family in enumerate(["nested-if", "sequential-diamonds", "loop-body"]):
    data = sorted((r for r in rows if r["family"] == family), key=lambda r: r["n"])
    n = [r["n"] for r in data]
    top, bottom = axes[:, column]
    top.plot(n, [r["base_median_ns"] / 1e6 for r in data], "o-", color="#335c81", label="Frozen baseline")
    top.plot(n, [r["on_median_ns"] / 1e6 for r in data], "s--", color="#bd6c2f", markerfacecolor="white", label="Forward candidate")
    top.set_title(family)
    top.set_ylabel("Complete process (ms)")
    top.set_ylim(0, 30)
    bottom.errorbar(n, [r["point"] for r in data],
        yerr=[[r["point"] - r["lower"] for r in data], [r["upper"] - r["point"] for r in data]],
        fmt="o", color="#335c81", capsize=4)
    bottom.axhline(1, color="#68717a", linewidth=1)
    bottom.axhline(1.01, color="#b74845", linewidth=1, linestyle="--")
    bottom.set_ylabel("Paired ON / BASE")
    bottom.set_ylim(0.94, 1.08)
    for axis in [top, bottom]:
        axis.set_xticks(n)
        axis.set_xlabel("n")
        axis.grid(alpha=0.18)
        axis.spines[["top", "right"]].set_visible(False)
axes[0, 0].legend(frameon=False, fontsize=9)
fig.suptitle("123: fixed 16 / 32 / 64 CFG growth\nMedians and simultaneous intervals; complete process endpoints only", fontsize=13)
output = repo / "docs/research/123"
fig.savefig(output / "growth.svg")
fig.savefig(output / "growth.png", dpi=170)
plt.close(fig)
